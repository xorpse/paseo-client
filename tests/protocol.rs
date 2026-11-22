use std::num::NonZeroU64;

use paseo_client::protocol::agents::{
    AgentAttachment, AgentDirectoryQuery, AgentFeature, AgentMessage, AgentSessionConfig,
    AgentSnapshot, AgentStatus, AgentUpdate, AttentionReason, ForgeChangeRequestAttachment,
    ForgeIssueAttachment, ModelDefinition, PermissionAllow, PermissionDeny, PermissionKind,
    PermissionResponse, SlashCommand, SlashCommandKind,
};
use paseo_client::protocol::checkout::{
    CheckDetails, CheckStatus, ChecksSummary, ForgeAuthState, ForgeFacts, ForgeItem, ForgeItemKind,
    ForgeStatus, MergeMethod, MergeStateStatus, Mergeability, PipelineStatus, ReviewDecision,
    WorkspacePullRequest,
};
use paseo_client::protocol::config::{
    DaemonConfig, DaemonConfigPatch, ProjectConfigError, ProviderConfig,
};
use paseo_client::protocol::daemon::{ClientHeartbeat, DaemonFeature, DeviceType};
use paseo_client::protocol::diff::{CheckoutStatus, DiffLineKind};
use paseo_client::protocol::files::{
    FileContent, FileKind, FileState, FileWrite, FileWriteOutcome,
};
use paseo_client::protocol::hosts::{HostDirectory, HostUpdate};
use paseo_client::protocol::labels::{LabelCatalogue, LabelColour, LabelUpdate};
use paseo_client::protocol::review::{
    AgentReview, CommentContext, CommentSide, CommentState, FileReference, ReviewCommentInput,
    SeenFile, TurnOrigin,
};
use paseo_client::protocol::schedules::{
    Schedule, ScheduleAgentConfig, ScheduleAgentPatch, ScheduleCadence, ScheduleCreate,
    ScheduleRun, ScheduleRunStatus, ScheduleStatus, ScheduleTarget, ScheduleUpdate,
};
use paseo_client::protocol::skills::{SavedSkills, SkillOperation, SkillSelection};
use paseo_client::protocol::stream::{AgentStreamEvent, AgentStreamMessage, TimelineUpdate};
use paseo_client::protocol::terminals::{TerminalActivityState, TerminalInfo};
use paseo_client::protocol::timeline::{TimelineItem, ToolCallDetail, ToolCallStatus};
use paseo_client::protocol::usage::{
    ProviderUsageList, ProviderUsageStatus, UsageProblem, UsageReport, UsageReportEntry,
    UsageReportsQuery,
};
use paseo_client::protocol::workspaces::{
    ChangeRequestCheckout, DirectorySuggestionsQuery, ProjectCheckout, SetupStatus,
    WorkspaceDescriptor, WorkspaceDirectoryPage, WorkspaceKind, WorkspaceSource, WorkspaceUpdate,
    WorktreeAction, WorktreeSource,
};
use paseo_client::{
    ArgumentError, DaemonUpdate, PaseoClient, PaseoError, SessionEvent, StatusEvent, Subscription,
    UsageReportFetch,
};
use serde_json::{Value, json};

fn snapshot_json() -> Value {
    json!({
        "id": "agent-1",
        "provider": "claude",
        "cwd": "/work/repo",
        "workspaceId": "wks_0123456789abcdef",
        "model": "sonnet",
        "features": [
            {"type": "toggle", "id": "plan", "label": "Plan", "value": true},
            {"type": "select", "id": "speed", "label": "Speed", "value": null,
             "options": [{"id": "fast", "label": "Fast", "isDefault": true}]}
        ],
        "thinkingOptionId": "high",
        "createdAt": "2026-10-01T00:00:00Z",
        "updatedAt": "2026-10-01T00:01:00Z",
        "lastUserMessageAt": null,
        "status": "running",
        "activeTurn": {"turnId": "turn-9", "startedAt": null},
        "capabilities": {
            "supportsStreaming": true,
            "supportsSessionPersistence": true,
            "supportsDynamicModes": false,
            "supportsMcpServers": true,
            "supportsReasoningStream": true,
            "supportsToolInvocations": true,
            "supportsSomethingNew": true
        },
        "currentModeId": "default",
        "availableModes": [{"id": "default", "label": "Default", "icon": "Sparkles"}],
        "pendingPermissions": [{
            "id": "perm-1",
            "provider": "claude",
            "name": "AskUserQuestion",
            "kind": "question",
            "input": {"questions": [{"header": "Pick"}]},
            "actions": [{"id": "ok", "label": "OK", "behavior": "allow", "variant": "primary"}]
        }],
        "persistence": null,
        "title": null,
        "labels": {"team": "core"},
        "requiresAttention": true,
        "attentionReason": "permission",
        "aFieldFromTheFuture": {"nested": 1}
    })
}

#[test]
fn agent_snapshot_parses_with_unknown_fields() {
    let snapshot = serde_json::from_value::<AgentSnapshot>(snapshot_json()).expect("snapshot");
    assert_eq!(snapshot.status(), AgentStatus::Running);
    assert_eq!(
        snapshot.active_turn().expect("active turn").turn_id(),
        "turn-9"
    );
    assert_eq!(
        snapshot.attention_reason(),
        Some(AttentionReason::Permission)
    );
    assert_eq!(
        snapshot.pending_permissions()[0].kind(),
        PermissionKind::Question
    );
    assert!(matches!(&snapshot.features()[0], AgentFeature::Toggle(toggle) if toggle.value()));
    assert!(
        matches!(&snapshot.features()[1], AgentFeature::Select(select) if select.value().is_none())
    );
    assert!(!snapshot.capabilities().supports_rewind_both());
    assert_eq!(snapshot.pinned_at(), None);
    assert_eq!(snapshot.last_response_at(), None);

    let mut organised = snapshot_json();
    organised["pinnedAt"] = json!("2026-11-13T10:00:00.000Z");
    organised["lastResponseAt"] = json!("2026-11-13T09:00:00.000Z");
    let organised = serde_json::from_value::<AgentSnapshot>(organised).expect("snapshot");
    assert_eq!(organised.pinned_at(), Some("2026-11-13T10:00:00.000Z"));
    assert_eq!(
        organised.last_response_at(),
        Some("2026-11-13T09:00:00.000Z")
    );
}

#[test]
fn agent_update_distinguishes_upsert_and_remove() {
    let upsert = serde_json::from_value::<AgentUpdate>(json!({
        "subscriptionId": "sub-1",
        "kind": "upsert",
        "agent": snapshot_json(),
        "project": {
            "projectKey": "key",
            "projectName": "repo",
            "checkout": {"cwd": "/work/repo", "isGit": true, "currentBranch": "main", "remoteUrl": null,
                         "isPaseoOwnedWorktree": false, "mainRepoRoot": null}
        },
        "seq": 3
    }))
    .expect("upsert");
    let AgentUpdate::Upsert(upsert) = upsert else {
        panic!("expected upsert");
    };
    let ProjectCheckout::Git(checkout) = upsert.project().expect("project").checkout() else {
        panic!("expected git checkout");
    };
    assert_eq!(checkout.worktree_root(), "/work/repo");

    let remove =
        serde_json::from_value::<AgentUpdate>(json!({"kind": "remove", "agentId": "agent-1"}))
            .expect("remove");
    assert!(matches!(remove, AgentUpdate::Remove(ref removal) if removal.agent_id() == "agent-1"));
}

#[test]
fn failed_tool_call_keeps_its_error_and_running_does_not_invent_one() {
    let failed = serde_json::from_value::<TimelineItem>(json!({
        "type": "tool_call",
        "callId": "call-1",
        "name": "Bash",
        "status": "failed",
        "error": {"message": "exit 1"},
        "detail": {"type": "shell", "command": "false", "exitCode": 1}
    }))
    .expect("failed tool call");
    let TimelineItem::ToolCall(call) = failed else {
        panic!("expected tool call");
    };
    assert_eq!(
        call.status(),
        &ToolCallStatus::Failed(json!({"message": "exit 1"}))
    );
    assert!(matches!(call.detail(), ToolCallDetail::Shell(shell) if shell.exit_code() == Some(1)));

    let running = serde_json::from_value::<TimelineItem>(json!({
        "type": "tool_call",
        "callId": "call-2",
        "name": "Edit",
        "status": "running",
        "error": null,
        "detail": {"type": "edit", "filePath": "src/main.rs", "unifiedDiff": "@@"}
    }))
    .expect("running tool call");
    let TimelineItem::ToolCall(call) = running else {
        panic!("expected tool call");
    };
    assert_eq!(call.status(), &ToolCallStatus::Running);
}

#[test]
fn every_timeline_item_kind_parses() {
    let items = json!([
        {"type": "user_message", "text": "hi", "messageId": "m1", "clientMessageId": "c1"},
        {"type": "assistant_message", "text": "hello"},
        {"type": "reasoning", "text": "thinking"},
        {"type": "todo", "items": [{"text": "a", "completed": false, "status": "in_progress"}]},
        {"type": "error", "message": "boom"},
        {"type": "notification", "level": "warning", "message": "careful"},
        {"type": "compaction", "status": "completed", "trigger": "auto", "preTokens": 1000},
        {"type": "tool_call", "callId": "c", "name": "Task", "status": "completed", "error": null,
         "detail": {"type": "sub_agent", "log": "", "actions": []}},
        {"type": "tool_call", "callId": "w", "name": "setup", "status": "completed", "error": null,
         "detail": {"type": "worktree_setup", "worktreePath": "/w", "branchName": "b", "log": "",
                    "commands": [{"index": 1, "command": "npm i", "cwd": "/w", "status": "completed", "exitCode": 0}]}},
        {"type": "tool_call", "callId": "u", "name": "mystery", "status": "canceled", "error": null,
         "detail": {"type": "unknown", "input": null, "output": [1, 2]}}
    ]);
    let parsed = serde_json::from_value::<Vec<TimelineItem>>(items).expect("timeline items");
    assert_eq!(parsed.len(), 10);
}

#[test]
fn stream_message_keeps_sequence_identity_and_usage() {
    let message = serde_json::from_value::<AgentStreamMessage>(json!({
        "agentId": "agent-1",
        "timestamp": "2026-10-01T00:00:00Z",
        "seq": 41,
        "epoch": "epoch-2",
        "event": {"type": "turn_completed", "provider": "codex", "turnId": "t-1",
                  "usage": {"inputTokens": 10, "outputTokens": 5, "totalCostUsd": 0.02}}
    }))
    .expect("stream message");
    assert_eq!(
        (message.seq(), message.epoch()),
        (Some(41), Some("epoch-2"))
    );
    let AgentStreamEvent::TurnCompleted(completed) = message.event() else {
        panic!("expected completed turn");
    };
    assert_eq!(completed.turn_id(), Some("t-1"));
    let usage = completed.usage().expect("usage");
    assert_eq!(usage.output_tokens(), Some(5));
    assert_eq!(usage.total_cost_usd(), Some(0.02));
}

#[test]
fn timeline_subscription_messages_are_adjacently_tagged() {
    let replacement = serde_json::from_value::<TimelineUpdate>(json!({
        "type": "agent.timeline.replacement",
        "payload": {"subscriptionId": "s", "agentId": "agent-1", "epoch": "e2"}
    }))
    .expect("replacement");
    assert!(matches!(replacement, TimelineUpdate::Replacement(ref r) if r.epoch() == "e2"));
}

#[test]
fn permission_responses_serialise_with_the_behaviour_tag() {
    let allow = PermissionResponse::Allow(PermissionAllow::default().with_selected_action_id("ok"));
    assert_eq!(
        serde_json::to_value(&allow).expect("serialise"),
        json!({"behavior": "allow", "selectedActionId": "ok"})
    );
    let deny = PermissionResponse::Deny(
        PermissionDeny::default()
            .with_message("no")
            .with_interrupt(true),
    );
    assert_eq!(
        serde_json::to_value(&deny).expect("serialise"),
        json!({"behavior": "deny", "message": "no", "interrupt": true})
    );
}

#[test]
fn a_change_request_worktree_serialises_to_the_daemon_shape() {
    let number = NonZeroU64::new(42).expect("not zero");
    let source = WorkspaceSource::Worktree(
        WorktreeSource::default()
            .with_cwd("/repo")
            .with_action(WorktreeAction::Checkout)
            .with_ref_name("feature")
            .with_checkout_source(ChangeRequestCheckout::new(number).with_forge("github")),
    );
    assert_eq!(
        serde_json::to_value(&source).expect("serialise"),
        json!({
            "kind": "worktree",
            "cwd": "/repo",
            "action": "checkout",
            "refName": "feature",
            "checkoutSource": {"kind": "change_request", "forge": "github", "number": 42}
        })
    );
}

#[test]
fn checkout_status_union_maps_to_optional_repository() {
    let not_git = serde_json::from_value::<CheckoutStatus>(json!({
        "cwd": "/tmp", "error": null, "requestId": "r", "isGit": false, "isPaseoOwnedWorktree": false,
        "repoRoot": null, "currentBranch": null, "isDirty": null, "baseRef": null, "aheadBehind": null,
        "aheadOfOrigin": null, "behindOfOrigin": null, "hasRemote": false, "remoteUrl": null
    }))
    .expect("not git");
    assert!(not_git.repository().is_none());

    let git = serde_json::from_value::<CheckoutStatus>(json!({
        "cwd": "/work/repo", "error": null, "requestId": "r", "isGit": true, "isPaseoOwnedWorktree": true,
        "repoRoot": "/work/repo", "mainRepoRoot": "/main", "currentBranch": "feature", "isDirty": true,
        "baseRef": "main", "aheadBehind": {"ahead": 2, "behind": 0}, "aheadOfOrigin": 1,
        "behindOfOrigin": null, "hasRemote": true, "remoteUrl": "git@example:repo.git",
        "upstreamRef": "refs/remotes/origin/feature"
    }))
    .expect("git");
    let repository = git.repository().expect("repository");
    assert!(repository.is_dirty() && repository.is_paseo_owned_worktree());
    let ahead_behind = repository.ahead_behind().expect("ahead/behind");
    assert_eq!((ahead_behind.ahead(), ahead_behind.behind()), (2, 0));
    assert_eq!(repository.ahead_of_origin(), Some(1));
    assert_eq!(git.upstream_ref(), Some("refs/remotes/origin/feature"));
    assert_eq!(repository.remote_host().as_deref(), Some("example"));
    let remote_host = |remote_url: &str| {
        let status = json!({
            "cwd": "/w", "error": null, "isGit": true, "repoRoot": "/w", "isDirty": false,
            "hasRemote": true, "remoteUrl": remote_url,
            "prStatus": {"status": null, "githubFeaturesEnabled": false, "authState": "unauthenticated",
                         "forge": "gitlab", "error": null}
        });
        let status = serde_json::from_value::<CheckoutStatus>(status).expect("status");
        assert_eq!(
            status.forge_status().map(|forge| forge.auth_state()),
            Some(ForgeAuthState::Unauthenticated)
        );
        status
            .repository()
            .and_then(|repository| repository.remote_host())
    };
    assert_eq!(
        remote_host("ssh://git@Git.Example.COM.:2222/group/sub/repo.git").as_deref(),
        Some("git.example.com")
    );
    assert_eq!(
        remote_host("https://gitlab.com/group/repo").as_deref(),
        Some("gitlab.com")
    );
    assert_eq!(remote_host("git://example.com/repo"), None);
    assert_eq!(remote_host("/srv/git/repo.git"), None);

    let broken = serde_json::from_value::<CheckoutStatus>(json!({
        "cwd": "/w", "error": null, "isGit": true, "hasRemote": false, "isDirty": false
    }));
    assert!(broken.is_err());
}

#[test]
fn workspace_descriptor_resolves_its_directory() {
    let workspace = serde_json::from_value::<WorkspaceDescriptor>(json!({
        "id": "wks_1", "projectId": "p", "projectDisplayName": "Repo", "projectRootPath": "/work/repo",
        "projectKind": "git", "workspaceKind": "worktree", "name": "feature", "status": "running",
        "activityAt": null, "scripts": [{"scriptName": "web", "hostname": "web.repo", "port": 3000,
        "lifecycle": "running", "health": "healthy"}],
        "gitRuntime": {"currentBranch": "feature", "aheadBehind": null},
        "githubRuntime": {"pullRequest": null}
    }))
    .expect("workspace");
    assert_eq!(workspace.workspace_kind(), WorkspaceKind::Worktree);
    assert_eq!(workspace.scripts()[0].port(), Some(3000));
    assert_eq!(workspace.directory(), "/work/repo");
    assert_eq!(workspace.branch(), Some("feature"));
}

#[test]
fn lenient_enums_follow_the_schema_catch_rules() {
    let terminal = serde_json::from_value::<TerminalInfo>(json!({
        "id": "t", "name": "zsh",
        "activity": {"state": "something-new", "attentionReason": "also-new", "changedAt": 1}
    }))
    .expect("terminal");
    let activity = terminal.activity().expect("activity");
    assert_eq!(activity.state(), TerminalActivityState::Idle);
    assert_eq!(activity.attention_reason(), None);

    let command = serde_json::from_value::<SlashCommand>(json!({
        "name": "review", "description": "", "argumentHint": "", "kind": "macro"
    }))
    .expect("command");
    assert_eq!(command.kind(), SlashCommandKind::Command);
}

#[test]
fn session_events_decode_from_whole_messages() {
    let event = serde_json::from_value::<SessionEvent>(json!({
        "type": "agent_permission_resolved",
        "payload": {"subscriptionId": "s", "agentId": "a", "requestId": "p",
                    "resolution": {"behavior": "deny", "interrupt": false}}
    }))
    .expect("event");
    assert!(
        matches!(event, SessionEvent::AgentPermissionResolved(ref resolved) if resolved.request_id() == "p")
    );

    let diff = serde_json::from_value::<paseo_client::protocol::diff::CheckoutDiff>(json!({
        "subscriptionId": "s", "cwd": "/w", "error": null,
        "files": [{"path": "a.rs", "isNew": false, "isDeleted": false, "additions": 1, "deletions": 0,
                   "hunks": [{"oldStart": 1, "oldCount": 0, "newStart": 1, "newCount": 1,
                              "lines": [{"type": "add", "content": "x", "tokens": []}]}]}]
    }))
    .expect("diff");
    let file = &diff.files()[0];
    assert_eq!((file.additions(), file.deletions()), (1, 0));
    let hunk = &file.hunks()[0];
    assert_eq!(
        (
            hunk.old_start(),
            hunk.old_count(),
            hunk.new_start(),
            hunk.new_count()
        ),
        (1, 0, 1, 1)
    );
    assert_eq!(hunk.lines()[0].kind(), DiffLineKind::Add);
}

fn tool_detail(update: &TimelineUpdate) -> &ToolCallDetail {
    let TimelineUpdate::Stream(message) = update else {
        panic!("expected an agent stream message");
    };
    let AgentStreamEvent::Timeline(timeline) = message.event() else {
        panic!("expected a timeline event");
    };
    let TimelineItem::ToolCall(call) = timeline.item() else {
        panic!("expected a tool call");
    };
    call.detail()
}

#[test]
fn upstream_schema_fixtures_decode() {
    let stream =
        serde_json::from_str::<Vec<TimelineUpdate>>(include_str!("fixtures/agent_stream.json"))
            .expect("agent_stream fixtures");
    let ToolCallDetail::Read(read) = tool_detail(&stream[0]) else {
        panic!("expected a read");
    };
    assert_eq!((read.offset(), read.limit()), (None, None));
    let ToolCallDetail::Search(search) = tool_detail(&stream[1]) else {
        panic!("expected a search");
    };
    assert_eq!(
        (search.num_files(), search.num_matches()),
        (Some(1), Some(0))
    );
    let ToolCallDetail::Fetch(fetch) = tool_detail(&stream[2]) else {
        panic!("expected a fetch");
    };
    assert_eq!((fetch.code(), fetch.bytes()), (None, Some(1024)));
    let TimelineUpdate::Stream(compaction) = &stream[4] else {
        panic!("expected a compaction stream message");
    };
    assert!(matches!(
        compaction.event(),
        AgentStreamEvent::Timeline(timeline)
            if matches!(timeline.item(), TimelineItem::Compaction(item) if item.pre_tokens() == Some(1234))
    ));
    let TimelineUpdate::Stream(completed) = &stream[5] else {
        panic!("expected a completed turn");
    };
    let AgentStreamEvent::TurnCompleted(completed) = completed.event() else {
        panic!("expected a completed turn");
    };
    let usage = completed.usage().expect("usage");
    assert_eq!(usage.cached_input_tokens(), Some(0));
    assert_eq!(usage.context_window_max_tokens(), Some(200000));

    let updates = serde_json::from_str::<Vec<Value>>(include_str!("fixtures/agent_update.json"))
        .expect("agent_update fixtures");
    let AgentUpdate::Upsert(upsert) =
        serde_json::from_value::<AgentUpdate>(updates[0]["payload"].clone()).expect("upsert")
    else {
        panic!("expected an upsert");
    };
    let agent = upsert.agent();
    assert_eq!(agent.pending_permissions().len(), 1);
    assert_eq!(
        agent.last_usage().and_then(|usage| usage.output_tokens()),
        Some(0)
    );

    let models = serde_json::from_str::<Vec<Value>>(include_str!(
        "fixtures/list_provider_models_response.json"
    ))
    .expect("models fixtures");
    let definitions =
        serde_json::from_value::<Vec<ModelDefinition>>(models[0]["payload"]["models"].clone())
            .expect("model definitions");
    assert_eq!(definitions[0].context_window_max_tokens(), Some(1000000));
}

#[test]
fn request_builders_enforce_the_upstream_ranges() {
    for limit in [0, 201] {
        assert!(matches!(
            AgentDirectoryQuery::default().with_page(limit, None),
            Err(ArgumentError::LimitOutOfRange { value, max: 200, .. }) if value == limit
        ));
    }
    assert!(AgentDirectoryQuery::default().with_page(200, None).is_ok());
    assert!(matches!(
        DirectorySuggestionsQuery::directories("~").with_limit(101),
        Err(ArgumentError::LimitOutOfRange { max: 100, .. })
    ));
    let config = || AgentSessionConfig::new("claude", "/work");
    assert!(matches!(
        config().with_title("   "),
        Err(ArgumentError::TitleBlank)
    ));
    assert!(matches!(
        config().with_title("é".repeat(201)),
        Err(ArgumentError::TitleTooLong {
            length: 201,
            max: 200
        })
    ));
    assert!(
        config()
            .with_title(format!("  {}  ", "é".repeat(200)))
            .is_ok()
    );
}

#[test]
fn whole_number_getters_reject_what_cannot_be_a_line_or_exit_code() {
    let read = serde_json::from_value::<ToolCallDetail>(
        json!({"type": "read", "filePath": "a.rs", "offset": 10, "limit": 20}),
    )
    .expect("read");
    let ToolCallDetail::Read(read) = read else {
        panic!("expected a read");
    };
    assert_eq!((read.offset(), read.limit()), (Some(10), Some(20)));

    let shell = serde_json::from_value::<ToolCallDetail>(
        json!({"type": "shell", "command": "x", "exitCode": 1.5}),
    )
    .expect("shell");
    let ToolCallDetail::Shell(shell) = shell else {
        panic!("expected a shell");
    };
    assert_eq!(shell.exit_code(), None);
}

#[test]
fn an_agent_review_decodes_from_the_daemon_shape() {
    let review = serde_json::from_str::<AgentReview>(include_str!("fixtures/agent_review.json"))
        .expect("agent review fixture");

    let nested = &review.repositories()[1];
    assert_eq!(nested.belongs_to(), Some("/repo"));
    let subagent_call = &nested.files()[0].tool_calls()[0];
    assert_eq!(
        (subagent_call.subagent_id(), subagent_call.turn_index()),
        (Some("child-1"), None)
    );
    let origins = review
        .turns()
        .iter()
        .map(|turn| (turn.origin(), turn.message_id()))
        .collect::<Vec<_>>();
    assert_eq!(
        origins,
        [
            (TurnOrigin::Prompt, Some("message-1")),
            (TurnOrigin::Autonomous, None)
        ]
    );
    let states = review
        .comments()
        .iter()
        .map(|comment| comment.state().clone())
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        [
            CommentState::Sent {
                message_id: "message-1".to_owned()
            },
            CommentState::Resolved { message_id: None },
            CommentState::Draft,
            CommentState::Draft
        ]
    );
    assert_eq!(review.comments()[0].lines(), 3..=4);
    assert_eq!(review.comments()[1].parent_id(), Some("comment-1"));
    let sides = review
        .comments()
        .iter()
        .map(|comment| comment.side())
        .collect::<Vec<_>>();
    assert_eq!(
        sides,
        [
            CommentSide::New,
            CommentSide::New,
            CommentSide::Old,
            CommentSide::Unknown
        ]
    );
}

#[test]
fn review_requests_serialise_to_the_daemon_shape() {
    let comment = ReviewCommentInput::new(
        "comment-1".to_owned(),
        FileReference::new("/repo".to_owned(), "/repo/src/main.ts".to_owned()),
        3..=4,
        CommentContext::new(
            "fn main() {".to_owned(),
            "  run();".to_owned(),
            "}".to_owned(),
        ),
        "Handle the error here.".to_owned(),
    )
    .with_fingerprint("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".to_owned())
    .with_state(CommentState::Sent {
        message_id: "message-1".to_owned(),
    });
    assert_eq!(
        serde_json::to_value(&comment).expect("comment"),
        json!({
            "id": "comment-1",
            "parentId": null,
            "repoRoot": "/repo",
            "path": "/repo/src/main.ts",
            "startLine": 3,
            "endLine": 4,
            "context": {"before": "fn main() {", "target": "  run();", "after": "}"},
            "fingerprint": "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
            "body": "Handle the error here.",
            "state": {"type": "sent", "messageId": "message-1"}
        })
    );
    let removed = comment.clone().with_side(CommentSide::Old);
    let wire = serde_json::to_value(&removed).expect("removed-line comment");
    assert_eq!(wire["side"], "old");
    assert_eq!(
        serde_json::to_value(removed.with_side(CommentSide::New)).expect("comment"),
        serde_json::to_value(&comment).expect("comment")
    );
    assert_eq!(
        serde_json::to_value(CommentState::Draft).expect("draft"),
        json!({"type": "draft"})
    );
    assert_eq!(
        serde_json::to_value(SeenFile::new(
            "/repo".to_owned(),
            "/repo/gone.rs".to_owned(),
            None
        ))
        .expect("seen file"),
        json!({"repoRoot": "/repo", "path": "/repo/gone.rs", "fingerprint": null})
    );
}

#[test]
fn heartbeats_serialise_to_the_daemon_shape() {
    let heartbeat = ClientHeartbeat::new(
        DeviceType::Web,
        Some("agent-1".to_owned()),
        "2026-11-13T10:00:00.000Z".to_owned(),
        true,
    )
    .with_app_visibility_changed_at("2026-11-13T09:59:00.000Z".to_owned());
    assert_eq!(
        serde_json::to_value(&heartbeat).unwrap(),
        json!({
            "deviceType": "web",
            "focusedAgentId": "agent-1",
            "focusedTerminalId": null,
            "lastActivityAt": "2026-11-13T10:00:00.000Z",
            "appVisible": true,
            "appVisibilityChangedAt": "2026-11-13T09:59:00.000Z"
        })
    );
}

#[test]
fn pull_request_status_carries_checks_ids_and_forge_facts() {
    let status = serde_json::from_value::<ForgeStatus>(json!({
        "cwd": "/repo",
        "status": {
            "forge": "github",
            "projectPath": "getpaseo/paseo",
            "number": 993,
            "url": "https://github.com/getpaseo/paseo/pull/993",
            "title": "Auto-merge UX",
            "state": "open",
            "repoOwner": "getpaseo",
            "repoName": "paseo",
            "baseRefName": "main",
            "headRefName": "github-pr-auto-merge-ux",
            "isMerged": false,
            "isDraft": false,
            "mergeable": "MERGEABLE",
            "checks": [{
                "name": "server tests",
                "status": "pending",
                "url": "https://github.com/getpaseo/paseo/actions/runs/993",
                "workflow": "CI",
                "checkRunId": 41_234_567_890u64,
                "workflowRunId": 9_930_001u64
            }],
            "checksStatus": "pending",
            "reviewDecision": "approved",
            "forgeSpecific": {
                "forge": "github",
                "mergeStateStatus": "BLOCKED",
                "autoMergeRequest": null,
                "viewerCanEnableAutoMerge": true,
                "viewerCanDisableAutoMerge": false,
                "viewerCanMergeAsAdmin": false,
                "viewerCanUpdateBranch": true,
                "repository": {
                    "autoMergeAllowed": true,
                    "mergeCommitAllowed": false,
                    "squashMergeAllowed": true,
                    "rebaseMergeAllowed": false,
                    "viewerDefaultMergeMethod": "SQUASH"
                },
                "isMergeQueueEnabled": false,
                "isInMergeQueue": false
            },
            "github": {"mergeStateStatus": "BLOCKED"}
        },
        "githubFeaturesEnabled": true,
        "authState": "authenticated",
        "forge": "github",
        "error": null,
        "requestId": "r"
    }))
    .expect("pull request status");
    assert_eq!(status.auth_state(), ForgeAuthState::Authenticated);
    let pull_request = status.pull_request().expect("pull request");
    assert_eq!(pull_request.project_path(), Some("getpaseo/paseo"));
    let check = &pull_request.checks()[0];
    assert_eq!(
        (check.check_run_id(), check.workflow_run_id()),
        (Some(41_234_567_890), Some(9_930_001))
    );
    let Some(ForgeFacts::Github(github)) = pull_request.forge_facts() else {
        panic!(
            "expected GitHub facts, got {:?}",
            pull_request.forge_facts()
        );
    };
    assert_eq!(github.merge_state_status(), Some(MergeStateStatus::Blocked));
    assert!(github.viewer_can_enable_auto_merge() && github.auto_merge_request().is_none());
    let repository = github.repository();
    assert!(repository.allows(MergeMethod::Squash) && !repository.allows(MergeMethod::Merge));
    assert_eq!(
        repository.viewer_default_merge_method(),
        Some(MergeMethod::Squash)
    );
}

#[test]
fn forge_facts_follow_the_facts_family_and_the_legacy_mirror() {
    let pull_request = |extra: Value| {
        let mut status = json!({
            "url": "https://gitlab.com/group/subgroup/repo/-/merge_requests/1",
            "title": "Add sample change", "state": "open", "baseRefName": "main",
            "headRefName": "feat/sample-change", "isMerged": false
        });
        status
            .as_object_mut()
            .expect("status")
            .extend(extra.as_object().expect("extra").clone());
        serde_json::from_value::<WorkspacePullRequest>(status).expect("pull request")
    };
    let gitlab = pull_request(json!({"forgeSpecific": {
        "forge": "gitlab",
        "detailedMergeStatus": "mergeable",
        "hasConflicts": false,
        "blockingDiscussionsResolved": true,
        "approvalsRequired": 1,
        "approvalsGiven": 1,
        "pipelineStatus": "waiting_for_resource",
        "pipelineId": 306,
        "pipelineUrl": "https://gitlab.com/group/subgroup/repo/-/pipelines/306",
        "mergeWhenPipelineSucceeds": false
    }}));
    let Some(ForgeFacts::Gitlab(facts)) = gitlab.forge_facts() else {
        panic!("expected GitLab facts, got {:?}", gitlab.forge_facts());
    };
    assert!(facts.is_mergeable());
    assert_eq!(facts.pipeline_id(), Some(306));
    assert!(
        facts
            .pipeline_status()
            .is_some_and(PipelineStatus::is_active)
    );
    let legacy = pull_request(json!({"forgeSpecific": {
        "forge": "gitlab", "detailedMergeStatus": null, "mergeStatus": "can_be_merged",
        "hasConflicts": true
    }}));
    let Some(ForgeFacts::Gitlab(facts)) = legacy.forge_facts() else {
        panic!("expected GitLab facts");
    };
    assert!(!facts.is_mergeable());
    let mirrored = pull_request(json!({"github": {"autoMergeRequest": {
        "enabledAt": "2026-04-02T13:50:00Z", "mergeMethod": "REBASE", "enabledBy": "octocat"
    }}}));
    let Some(ForgeFacts::Github(github)) = mirrored.forge_facts() else {
        panic!("expected mirrored GitHub facts");
    };
    let request = github.auto_merge_request().expect("auto-merge");
    assert_eq!(
        (request.merge_method(), request.enabled_by()),
        (Some(MergeMethod::Rebase), Some("octocat"))
    );
    assert_eq!(
        pull_request(json!({"forgeSpecific": {"forge": "bitbucket", "x": 1}})).forge_facts(),
        Some(&ForgeFacts::Unknown)
    );
    assert_eq!(
        pull_request(json!({"forgeSpecific": {"forge": "github", "repository": 7}})).forge_facts(),
        None
    );
}

#[test]
fn legacy_pull_request_status_infers_the_auth_state_from_its_flag() {
    let status = |payload: Value| {
        serde_json::from_value::<ForgeStatus>(payload)
            .expect("status")
            .auth_state()
    };
    assert_eq!(
        status(json!({"status": null, "githubFeaturesEnabled": false, "error": null})),
        ForgeAuthState::Unauthenticated
    );
    assert_eq!(
        status(json!({"status": null, "githubFeaturesEnabled": true, "authState": "magic_link"})),
        ForgeAuthState::Authenticated
    );
    assert_eq!(
        status(json!({"status": null, "githubFeaturesEnabled": false, "authState": "cli_missing"})),
        ForgeAuthState::CliMissing
    );
}

#[test]
fn provider_usage_decodes_from_the_daemon_shape() {
    let usage = serde_json::from_value::<ProviderUsageList>(json!({
        "requestId": "r",
        "fetchedAt": "2026-11-14T10:00:00.000Z",
        "providers": [{
            "providerId": "claude",
            "displayName": "Claude",
            "status": "available",
            "planLabel": "Max",
            "windows": [{"id": "5h", "label": "5 hours", "shortLabel": "5h", "summary": true,
                         "usedPct": 41.5, "remainingPct": 58.5,
                         "resetsAt": "2026-11-14T12:00:00.000Z", "tone": "ok"}],
            "balances": [{"id": "credit", "label": "Credit", "used": 3.5, "remaining": null,
                          "limit": 10, "unit": "usd"}],
            "details": [{"id": "d", "label": "Seat", "value": "pro"}],
            "error": null
        }]
    }))
    .expect("provider usage");
    assert_eq!(usage.fetched_at(), Some("2026-11-14T10:00:00.000Z"));
    let provider = &usage.providers()[0];
    assert_eq!(provider.provider_id(), "claude");
    assert_eq!(provider.status(), ProviderUsageStatus::Available);
    assert_eq!(provider.plan_label(), Some("Max"));
    let window = &provider.windows()[0];
    assert_eq!(
        (window.label(), window.short_label()),
        ("5 hours", Some("5h"))
    );
    assert!(window.summary());
    assert_eq!(window.used_pct(), Some(41.5));
    let balance = &provider.balances()[0];
    assert_eq!(
        (balance.used(), balance.remaining(), balance.limit()),
        (Some(3.5), None, Some(10.0))
    );
    assert_eq!(balance.unit(), Some("usd"));
}

#[test]
fn workspace_scripts_report_whether_they_run() {
    let script =
        serde_json::from_value::<paseo_client::protocol::workspaces::WorkspaceScript>(json!({
            "scriptName": "web",
            "type": "service",
            "hostname": "web.main.app.localhost",
            "port": 5173,
            "localProxyUrl": "http://web.main.app.localhost:6767",
            "lifecycle": "running",
            "health": "healthy",
            "terminalId": "term-1"
        }))
        .expect("workspace script");
    assert!(script.is_service() && script.is_running());
    assert_eq!(script.port(), Some(5173));
    assert_eq!(script.terminal_id(), Some("term-1"));
}

#[test]
fn forge_attachments_serialise_to_the_daemon_shapes() {
    let message = AgentMessage::new("look")
        .with_attachment(AgentAttachment::ForgeIssue(
            ForgeIssueAttachment::new("github", 7, "Crash", "https://example.com/7")
                .with_body("It crashes"),
        ))
        .with_attachment(AgentAttachment::ForgeChangeRequest(
            ForgeChangeRequestAttachment::new("gitlab", 9, "Fix", "https://example.com/9")
                .with_project_path("group/repo")
                .with_head_ref_name("fix"),
        ));
    assert_eq!(
        serde_json::to_value(&message).unwrap()["attachments"],
        json!([
            {"type": "forge_issue", "mimeType": "application/paseo-forge-issue", "forge": "github",
             "number": 7, "title": "Crash", "url": "https://example.com/7", "body": "It crashes"},
            {"type": "forge_change_request", "mimeType": "application/paseo-forge-change-request",
             "forge": "gitlab", "number": 9, "title": "Fix", "url": "https://example.com/9",
             "projectPath": "group/repo", "headRefName": "fix"}
        ])
    );
}

#[test]
fn schedules_round_trip_the_daemon_shapes() {
    let mut config = ScheduleAgentConfig::new("claude", "/work/repo");
    config.set_model("opus");
    config.set_title("Nightly triage");
    let create = ScheduleCreate::new(
        "Triage new issues",
        ScheduleCadence::Cron {
            expression: "0 9 * * *".to_owned(),
            timezone: Some("Europe/London".to_owned()),
        },
        ScheduleTarget::NewAgent(config),
    )
    .with_name("triage")
    .with_run_on_create(false);
    assert_eq!(
        serde_json::to_value(&create).unwrap(),
        json!({
            "prompt": "Triage new issues",
            "name": "triage",
            "cadence": {"type": "cron", "expression": "0 9 * * *", "timezone": "Europe/London"},
            "target": {"type": "new-agent",
                       "config": {"provider": "claude", "cwd": "/work/repo", "model": "opus",
                                  "title": "Nightly triage"}},
            "runOnCreate": false
        })
    );
    assert_eq!(
        serde_json::to_value(ScheduleTarget::Agent {
            agent_id: "a-1".to_owned()
        })
        .unwrap(),
        json!({"type": "agent", "agentId": "a-1"})
    );

    let schedule = serde_json::from_value::<Schedule>(json!({
        "id": "sch-1",
        "name": null,
        "prompt": "Check the deploy",
        "cadence": {"type": "every", "everyMs": 3_600_000},
        "target": {"type": "self", "agentId": "8c1f0b0e-6d3b-4d7a-9a52-7f2d9a8f6c11"},
        "status": "paused",
        "createdAt": "2026-11-14T10:00:00.000Z",
        "updatedAt": "2026-11-14T10:00:00.000Z",
        "nextRunAt": null,
        "lastRunAt": "2026-11-14T09:00:00.000Z",
        "pausedAt": "2026-11-14T10:00:00.000Z",
        "expiresAt": null,
        "maxRuns": null,
        "runs": []
    }))
    .expect("schedule");
    assert_eq!(schedule.name(), None);
    assert_eq!(schedule.status(), ScheduleStatus::Paused);
    assert_eq!(
        schedule.cadence(),
        &ScheduleCadence::Every {
            every_ms: 3_600_000
        }
    );
    assert_eq!(
        schedule.target(),
        &ScheduleTarget::Agent {
            agent_id: "8c1f0b0e-6d3b-4d7a-9a52-7f2d9a8f6c11".to_owned()
        }
    );
    assert_eq!(schedule.last_run_at(), Some("2026-11-14T09:00:00.000Z"));

    let run = serde_json::from_value::<ScheduleRun>(json!({
        "id": "run-1",
        "scheduledFor": "2026-11-14T09:00:00.000Z",
        "startedAt": "2026-11-14T09:00:01.000Z",
        "endedAt": null,
        "status": "running",
        "agentId": null,
        "workspaceId": null,
        "output": null,
        "error": null
    }))
    .expect("schedule run");
    assert_eq!(
        (run.id(), run.status()),
        ("run-1", ScheduleRunStatus::Running)
    );
    assert_eq!(run.ended_at(), None);
}

#[test]
fn workspace_forge_runtime_and_sort_times_decode_from_the_daemon_shape() {
    let page = serde_json::from_str::<WorkspaceDirectoryPage>(include_str!(
        "fixtures/fetch_workspaces_runtime.json"
    ))
    .expect("workspace page fixture");
    let workspace = &page.entries()[0];
    assert_eq!(workspace.created_at(), Some("2026-04-10T09:30:00.000Z"));
    assert_eq!(
        workspace.last_user_message_at(),
        Some("2026-04-11T17:45:00.000Z")
    );
    assert_eq!(workspace.setup_status(), Some(SetupStatus::Completed));
    assert_eq!(workspace.sync_seq(), Some(7));
    assert!(workspace.project_custom_icon_revision().is_some());
    let runtime = workspace.forge_runtime().expect("forge runtime");
    assert_eq!(runtime.refreshed_at(), Some("2026-04-12T00:00:00.000Z"));
    let pull_request = runtime.pull_request().expect("pull request");
    assert_eq!(
        (pull_request.number(), pull_request.title()),
        (Some(123), "Runtime payloads")
    );
    assert_eq!(pull_request.mergeable(), Some(Mergeability::Unknown));
    assert_eq!(pull_request.checks_status(), Some(ChecksSummary::Pending));
    assert_eq!(
        pull_request.review_decision(),
        Some(ReviewDecision::ChangesRequested)
    );
    let checks = pull_request
        .checks()
        .iter()
        .map(|check| (check.name(), check.status(), check.traits().len()))
        .collect::<Vec<_>>();
    assert_eq!(
        checks,
        [
            ("legacy", CheckStatus::Success, 0),
            ("deploy", CheckStatus::Pending, 3)
        ]
    );
}

#[test]
fn pipeline_check_details_decode_from_the_daemon_shape() {
    let response =
        serde_json::from_str::<Value>(include_str!("fixtures/check_details_pipeline.json"))
            .expect("check details fixture");
    let details =
        serde_json::from_value::<CheckDetails>(response["details"].clone()).expect("details");
    assert_eq!(
        (details.check_run_id(), details.name()),
        (306, "Pipeline (feat/x)")
    );
    let pipeline = details.pipeline().expect("pipeline");
    assert_eq!(pipeline.git_ref(), Some("feat/x"));
    let job = &pipeline.stages()[0].jobs()[0];
    assert_eq!(
        (job.name(), job.duration_seconds(), job.allow_failure()),
        ("unit", Some(4.0), false)
    );
}

#[test]
fn newer_daemon_values_degrade_instead_of_failing_the_message() {
    let mut snapshot = snapshot_json();
    snapshot["status"] = json!("hibernating");
    snapshot["features"]
        .as_array_mut()
        .expect("features")
        .push(json!({"type": "slider", "id": "temperature", "label": "Temperature"}));
    let snapshot = serde_json::from_value::<AgentSnapshot>(snapshot).expect("snapshot");
    assert_eq!(snapshot.status(), AgentStatus::Unknown);
    assert_eq!(
        snapshot
            .features()
            .iter()
            .map(AgentFeature::id)
            .collect::<Vec<_>>(),
        ["plan", "speed"]
    );

    let updates = serde_json::from_value::<Vec<TimelineUpdate>>(json!([
        {"type": "agent_stream", "payload": {"agentId": "a", "timestamp": "t", "seq": 1, "epoch": "e",
         "event": {"type": "timeline", "provider": "claude",
                   "item": {"type": "plugin", "id": "p1", "pluginId": "x", "kind": "chart", "version": 1, "data": {}}}}},
        {"type": "agent_stream", "payload": {"agentId": "a", "timestamp": "t", "seq": 2, "epoch": "e",
         "event": {"type": "timeline", "provider": "claude",
                   "item": {"type": "tool_call", "callId": "c", "name": "Diagram", "status": "completed",
                            "detail": {"type": "diagram", "nodes": []}}}}},
        {"type": "agent_stream", "payload": {"agentId": "a", "timestamp": "t", "seq": 3, "epoch": "e",
         "event": {"type": "turn_paused", "provider": "claude"}}}
    ]))
    .expect("timeline updates");
    let TimelineUpdate::Stream(plugin) = &updates[0] else {
        panic!("expected a stream message");
    };
    assert!(matches!(
        plugin.event(),
        AgentStreamEvent::Timeline(timeline) if matches!(timeline.item(), TimelineItem::Unknown)
    ));
    assert!(matches!(
        tool_detail(&updates[1]),
        ToolCallDetail::Unknown(detail) if detail.input().is_null()
    ));
    let TimelineUpdate::Stream(paused) = &updates[2] else {
        panic!("expected a stream message");
    };
    assert!(matches!(paused.event(), AgentStreamEvent::Unknown));

    let events = serde_json::from_value::<Vec<SessionEvent>>(json!([
        {"type": "status", "payload": {"subscriptionId": "s", "status": "plugin_catalog_changed", "pluginId": "x"}},
        {"type": "workspace_setup_progress", "payload": {"subscriptionId": "s", "workspaceId": "w",
         "status": "paused", "detail": {"type": "worktree_setup", "worktreePath": "/w", "branchName": "b",
                                        "log": "", "commands": []}, "error": null}}
    ]))
    .expect("session events");
    assert!(matches!(
        events[0],
        SessionEvent::Status(StatusEvent::Unknown)
    ));
    assert!(matches!(
        &events[1],
        SessionEvent::WorkspaceSetup(progress) if progress.snapshot().status() == SetupStatus::Unknown
    ));
}

#[test]
fn status_events_carry_the_server_info_and_daemon_config() {
    let events = serde_json::from_value::<Vec<SessionEvent>>(json!([
        {"type": "status", "payload": {"subscriptionId": "s", "status": "server_info",
         "serverId": "srv_1", "hostname": "box", "version": "0.13.2",
         "features": {"workspaceLabels": true}}},
        {"type": "status", "payload": {"subscriptionId": "s", "status": "daemon_config_changed",
         "config": {"mcp": {"injectIntoAgents": true}, "appendSystemPrompt": "Be terse",
                    "providers": {"claude": {"enabled": false, "paseoTools": {"disabledTools": ["x"]}}},
                    "agentProfiles": [{"id": "p", "name": "Reviewer", "provider": "claude",
                                       "modeId": "plan", "futureField": 1}],
                    "skills": {"selection": {"mode": "custom", "skills": ["paseo"]}}}}}
    ]))
    .expect("status events");
    let SessionEvent::Status(StatusEvent::ServerInfo(info)) = &events[0] else {
        panic!("expected server info, got {:?}", events[0]);
    };
    assert!(info.supports(DaemonFeature::WorkspaceLabels));
    let SessionEvent::Status(StatusEvent::DaemonConfigChanged { config }) = &events[1] else {
        panic!("expected a config change, got {:?}", events[1]);
    };
    assert!(config.mcp().inject_into_agents());
    assert_eq!(config.append_system_prompt(), "Be terse");
    assert_eq!(config.providers()["claude"].enabled(), Some(false));
    assert_eq!(
        config.skill_selection(),
        Some(&SkillSelection::Custom {
            skills: vec!["paseo".to_owned()]
        })
    );
    let profile = &config.agent_profiles().expect("profiles")[0];
    assert_eq!(profile.mode_id(), Some("plan"));
    let patch = DaemonConfigPatch::default().with_agent_profiles(vec![profile.clone()]);
    assert_eq!(
        serde_json::to_value(&patch).expect("patch")["agentProfiles"][0]["futureField"],
        json!(1),
        "fields this client does not know survive a round trip"
    );
}

#[test]
fn file_write_outcomes_cover_every_daemon_result() {
    let outcomes = serde_json::from_value::<Vec<FileWriteOutcome>>(json!([
        {"status": "written", "modifiedAt": "m2", "size": 6, "revision": "r2"},
        {"status": "conflict", "version": {"status": "ready", "cwd": "/w", "path": "a.txt",
         "size": 9, "modifiedAt": "m3", "revision": "r3", "kind": "text"}},
        {"status": "conflict", "version": {"status": "missing", "cwd": "/w", "path": "a.txt"}},
        {"status": "error", "error": "EACCES"},
        {"status": "queued"}
    ]))
    .expect("outcomes");
    assert!(matches!(&outcomes[0], FileWriteOutcome::Written(written)
            if written.size() == 6 && written.revision().revision() == Some("r2")));
    let FileWriteOutcome::Conflict { version } = &outcomes[1] else {
        panic!("expected a conflict");
    };
    let FileState::Ready(ready) = version.state() else {
        panic!("expected a ready version");
    };
    assert_eq!(
        (ready.size(), ready.revision().modified_at(), ready.kind()),
        (9, "m3", Some(FileKind::Text))
    );
    assert!(
        matches!(&outcomes[2], FileWriteOutcome::Conflict { version } if *version.state() == FileState::Missing)
    );
    assert!(matches!(&outcomes[3], FileWriteOutcome::Error { error } if error == "EACCES"));
    assert!(matches!(outcomes[4], FileWriteOutcome::Unknown));

    let write = FileWrite::bytes("logo.png", &[0, 255], ready.revision());
    assert_eq!(
        serde_json::to_value(&write).expect("write"),
        json!({"path": "logo.png", "content": "AP8=", "contentEncoding": "base64",
               "expectedModifiedAt": "m3", "expectedRevision": "r3"})
    );
}

#[test]
fn forge_and_usage_payloads_decode_from_the_daemon_shapes() {
    let items = serde_json::from_value::<Vec<ForgeItem>>(json!([
        {"kind": "pr", "number": 7, "title": "Fix", "url": "u", "state": "OPEN", "body": null,
         "labels": ["bug"], "baseRefName": "main", "headRefName": "fix"},
        {"kind": "change_request", "forge": "gitlab", "number": 8, "title": "MR", "url": "u",
         "state": "opened", "body": "b", "labels": []}
    ]))
    .expect("forge items");
    assert_eq!(
        items
            .iter()
            .map(|item| (item.kind(), item.number()))
            .collect::<Vec<_>>(),
        [
            (ForgeItemKind::ChangeRequest, 7),
            (ForgeItemKind::ChangeRequest, 8)
        ]
    );

    let entry = serde_json::from_value::<UsageReportEntry>(json!({
        "id": "claude:me", "account": {"label": "me@example.com"}, "fetchedAt": "f",
        "sourceId": "claude", "sourceLabel": "Claude",
        "report": {"status": "unavailable", "problem": {"kind": "expired", "expiresAt": "2026-01-01T00:00:00Z"}},
        "lastSuccessful": {"fetchedAt": "f0", "report": {"status": "available", "planLabel": "Max",
                           "windows": [{"id": "5h", "label": "5 hours", "usedPct": 40}]}},
        "loginErrors": [{"harness": "codex", "report": {"status": "unavailable",
                         "problem": {"kind": "revoked"}}}]
    }))
    .expect("usage entry");
    assert_eq!(entry.account_label(), Some("me@example.com"));
    assert!(matches!(
        entry.report(),
        UsageReport::Unavailable { problem: UsageProblem::Expired { expires_at, .. } } if expires_at == "2026-01-01T00:00:00Z"
    ));
    let last = entry.last_successful().expect("last successful");
    assert_eq!(
        (
            last.report().plan_label(),
            last.report().windows()[0].used_pct()
        ),
        (Some("Max"), Some(40.0))
    );
    assert!(matches!(
        entry.login_errors()[0].report(),
        UsageReport::Unavailable {
            problem: UsageProblem::Unknown
        }
    ));
    assert_eq!(
        serde_json::to_value(UsageReportsQuery::for_agent("a").with_force_refresh(true))
            .expect("query"),
        json!({"agentId": "a", "forceRefresh": true})
    );
}

#[test]
fn label_catalogue_changes_decode_and_resume_from_their_cursor() {
    let catalogue = serde_json::from_value::<LabelCatalogue>(json!({
        "requestId": "r", "subscriptionId": "s",
        "labels": [{"name": "Backend", "color": "sky"}, {"name": "Later", "color": "chartreuse"}],
        "sync": {"mode": "snapshot", "generation": "g1", "headSeq": 4, "removals": []}
    }))
    .expect("catalogue");
    assert_eq!(catalogue.labels()[1].colour(), LabelColour::Unknown);
    assert_eq!(
        serde_json::to_value(catalogue.sync().cursor()).expect("cursor"),
        json!({"generation": "g1", "afterSeq": 4})
    );
    let updates = serde_json::from_value::<Vec<LabelUpdate>>(json!([
        {"subscriptionId": "s", "kind": "upsert", "label": {"name": "API", "color": "teal"},
         "previousName": "Backend", "generation": "g1", "seq": 5},
        {"subscriptionId": "s", "kind": "remove", "name": "Later", "generation": "g1", "seq": 6}
    ]))
    .expect("label updates");
    assert!(
        matches!(&updates[0], LabelUpdate::Upsert(upserted) if upserted.previous_name() == Some("Backend"))
    );
    assert!(matches!(&updates[1], LabelUpdate::Remove(removed) if removed.seq() == 6));
}

#[test]
fn partial_updates_distinguish_clearing_from_keeping() {
    let update = ScheduleUpdate::default()
        .with_name(None)
        .with_max_runs(Some(2))
        .with_new_agent_config(ScheduleAgentPatch::default().with_model(None));
    assert_eq!(
        serde_json::to_value(&update).expect("schedule update"),
        json!({"name": null, "maxRuns": 2, "newAgentConfig": {"model": null}})
    );
    let host = HostUpdate::default().with_label(None).with_enabled(false);
    assert_eq!(
        serde_json::to_value(&host).expect("host update"),
        json!({"label": null, "enabled": false})
    );
    let patch = DaemonConfigPatch::default()
        .with_require_protected_security(true)
        .with_removed_providers(vec!["hoplite".to_owned()])
        .with_provider("claude", ProviderConfig::default().with_enabled(false));
    assert_eq!(
        serde_json::to_value(&patch).expect("config patch"),
        json!({"relay": {"requireProtectedSecurity": true}, "removeProviders": ["hoplite"],
               "providers": {"claude": {"enabled": false}}})
    );
}

#[test]
fn project_config_and_skill_results_keep_their_typed_failures() {
    let stale = serde_json::from_value::<ProjectConfigError>(json!({
        "code": "stale_project_config", "currentRevision": {"mtimeMs": 1_731_000_000_123.5_f64, "size": 42}
    }))
    .expect("stale");
    let ProjectConfigError::StaleProjectConfig {
        current_revision: Some(revision),
    } = stale
    else {
        panic!("expected a stale config, got {stale:?}");
    };
    assert_eq!(
        serde_json::to_value(revision).expect("revision"),
        json!({"mtimeMs": 1_731_000_000_123.5_f64, "size": 42.0}),
        "a fractional modification time is sent back exactly"
    );
    let saved = serde_json::from_value::<SavedSkills>(json!({
        "requestId": "r", "state": "drift", "ops": [{"kind": "delete", "name": "old"}, {"kind": "rename", "name": "x"}],
        "available": ["paseo"], "installed": ["old"], "selection": {"mode": "all"},
        "confirmationRequired": {"removals": ["old"]}
    }))
    .expect("saved skills");
    assert_eq!(saved.removals_to_confirm(), Some(&["old".to_owned()][..]));
    assert_eq!(
        saved.status().operations(),
        [
            SkillOperation::Delete {
                name: "old".to_owned()
            },
            SkillOperation::Unknown
        ]
    );
}

#[test]
fn public_models_can_cross_threads() {
    fn shareable<T: Send + Sync>() {}
    shareable::<PaseoError>();
    shareable::<PaseoClient>();
    shareable::<Subscription<WorkspaceDirectoryPage, WorkspaceUpdate>>();
    shareable::<DaemonUpdate>();
    shareable::<UsageReportFetch>();
    shareable::<AgentSnapshot>();
    shareable::<WorkspaceDescriptor>();
    shareable::<DaemonConfig>();
    shareable::<HostDirectory>();
    shareable::<FileContent>();
    shareable::<SessionEvent>();
}
