use paseo_client::protocol::agents::{
    AgentDirectoryQuery, AgentFeature, AgentSessionConfig, AgentSnapshot, AgentStatus, AgentUpdate,
    AttentionReason, ModelDefinition, PermissionAllow, PermissionDeny, PermissionKind,
    PermissionResponse, SlashCommand, SlashCommandKind,
};
use paseo_client::protocol::diff::{CheckoutStatus, DiffLineKind};
use paseo_client::protocol::stream::{AgentStreamEvent, AgentStreamMessage, TimelineUpdate};
use paseo_client::protocol::terminals::{TerminalActivityState, TerminalInfo};
use paseo_client::protocol::timeline::{TimelineItem, ToolCallDetail, ToolCallStatus};
use paseo_client::protocol::workspaces::{
    DirectorySuggestionsQuery, ProjectCheckout, WorkspaceDescriptor, WorkspaceKind,
};
use paseo_client::{ArgumentError, SessionEvent};
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
