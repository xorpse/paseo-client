use std::sync::Arc;

use async_net::{TcpListener, TcpStream};
use async_tungstenite::WebSocketStream;
use async_tungstenite::tungstenite::Message;
use futures::StreamExt;
use futures_rustls::rustls::crypto::ring;
use futures_rustls::rustls::{ClientConfig, RootCertStore};
use paseo_client::protocol::agents::CreationKind;
use paseo_client::protocol::checkout::{
    ForgeAuthState, ForgeItemKind, PullRequestTimelineItem, ReviewState, TimelineErrorKind,
};
use paseo_client::protocol::daemon::DaemonFeature;
use paseo_client::protocol::diff::CheckoutErrorCode;
use paseo_client::protocol::review::{
    CommentContext, CommentSide, FileReference, ReviewCommentInput,
};
use paseo_client::{ConnectOptions, ConnectTarget, PaseoClient, PaseoError};
use serde_json::{Value, json};

struct FakeDaemon {
    socket: WebSocketStream<TcpStream>,
}

impl FakeDaemon {
    async fn request(&mut self) -> Value {
        loop {
            let message = self
                .socket
                .next()
                .await
                .expect("the client stays connected")
                .expect("a frame");
            let Message::Text(text) = message else {
                continue;
            };
            let envelope = serde_json::from_str::<Value>(text.as_str()).expect("json");
            if envelope["type"] == "session" {
                return envelope["message"].clone();
            }
        }
    }

    async fn reply(&mut self, request: &Value, kind: &str, mut payload: Value) {
        payload["requestId"] = request["requestId"].clone();
        let text = json!({"type": "session", "message": {"type": kind, "payload": payload}});
        self.socket
            .send(Message::text(text.to_string()))
            .await
            .expect("reply");
    }
}

fn tls() -> Arc<ClientConfig> {
    let provider = Arc::new(ring::default_provider());
    Arc::new(
        ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth(),
    )
}

async fn connect(features: Value) -> (PaseoClient, FakeDaemon) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let daemon = smol::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = async_tungstenite::accept_async(stream)
            .await
            .expect("websocket");
        socket.next().await.expect("hello").expect("hello frame");
        let info = json!({"type": "session", "message": {"type": "status", "payload": {
            "status": "server_info", "serverId": "fake", "features": features}}});
        socket
            .send(Message::text(info.to_string()))
            .await
            .expect("server info");
        FakeDaemon { socket }
    });
    let target = format!("127.0.0.1:{port}")
        .parse::<ConnectTarget>()
        .expect("target");
    let options = ConnectOptions::new("paseo-client-wire-test", tls()).with_outdated_daemons();
    let client = PaseoClient::connect(&target, &options)
        .await
        .expect("connect");
    smol::spawn({
        let client = client.clone();
        async move { client.run().await }
    })
    .detach();
    (client, daemon.await)
}

#[test]
fn a_local_credential_authenticates_the_hello() {
    smol::block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let daemon = smol::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = async_tungstenite::accept_async(stream)
                .await
                .expect("websocket");
            let Message::Text(hello) = socket.next().await.expect("hello").expect("hello frame")
            else {
                panic!("the hello is a text frame");
            };
            let info = json!({"type": "session", "message": {"type": "status", "payload": {
                "status": "server_info", "serverId": "fake", "features": {}}}});
            socket
                .send(Message::text(info.to_string()))
                .await
                .expect("server info");
            serde_json::from_str::<Value>(hello.as_str()).expect("json")
        });
        let target = format!("127.0.0.1:{port}")
            .parse::<ConnectTarget>()
            .expect("target")
            .with_password(Some("ignored".to_owned()))
            .with_local_credential(Some("token".to_owned()));
        assert_eq!(target.password(), None);
        let options = ConnectOptions::new("paseo-client-wire-test", tls()).with_outdated_daemons();
        let client = PaseoClient::connect(&target, &options)
            .await
            .expect("connect");
        assert_eq!(
            daemon.await["auth"],
            json!({"kind": "localCredential", "token": "token"})
        );
        client.close();
    });
}

#[test]
fn a_pull_request_timeline_keeps_thread_identity_and_skips_unknown_kinds() {
    smol::block_on(async {
        let (client, mut daemon) = connect(json!({})).await;
        let timeline = smol::spawn({
            let client = client.clone();
            async move {
                client
                    .pull_request_timeline("/w", 42, "parentOwner", "parentRepo")
                    .await
            }
        });
        let request = daemon.request().await;
        daemon
            .reply(
                &request,
                "pull_request_timeline_response",
                json!({"cwd": "/w", "prNumber": 42, "truncated": true, "error": null,
                       "githubFeaturesEnabled": true, "items": [
                    {"kind": "review", "id": "PRR_empty_commented", "author": "unknown",
                     "authorUrl": null, "avatarUrl": null, "body": "",
                     "createdAt": 1_775_137_800_000u64,
                     "url": "https://github.com/parentOwner/parentRepo/pull/42#pullrequestreview-2",
                     "reviewState": "commented"},
                    {"id": "x1", "kind": "merge_event", "body": "merged"},
                    {"kind": "comment", "id": "PRRC_1", "author": "inline-reviewer",
                     "authorUrl": "https://github.com/inline-reviewer",
                     "avatarUrl": "https://avatars.githubusercontent.com/u/3?v=4",
                     "body": "This should include line context.",
                     "createdAt": 1_775_137_860_000u64,
                     "url": "https://github.com/parentOwner/parentRepo/pull/42#discussion_r1",
                     "reviewId": "PRR_empty_commented",
                     "location": {"path": "packages/app/src/git/pull-request-panel/data.ts",
                                  "line": 24, "startLine": 20, "threadId": "PRRT_1",
                                  "isResolved": true, "isOutdated": false}},
                    {"kind": "comment", "id": "note-1", "author": "gitlab-user", "body": "General",
                     "createdAt": 0, "url": "", "threadId": "disc-2", "threadIsResolved": true}
                ]}),
            )
            .await;
        let timeline = timeline.await.expect("timeline");
        assert_eq!(timeline.pr_number(), Some(42));
        assert!(timeline.truncated() && timeline.error().is_none());
        let items = timeline.items();
        assert_eq!(items.len(), 3);
        let PullRequestTimelineItem::Review(review) = &items[0] else {
            panic!("expected a review, got {:?}", items[0]);
        };
        assert_eq!(
            (review.id(), review.review_state(), review.avatar_url()),
            ("PRR_empty_commented", ReviewState::Commented, None)
        );
        let PullRequestTimelineItem::Comment(inline) = &items[1] else {
            panic!("expected a comment, got {:?}", items[1]);
        };
        assert_eq!(inline.review_id(), Some(review.id()));
        assert_eq!(
            inline.avatar_url(),
            Some("https://avatars.githubusercontent.com/u/3?v=4")
        );
        let location = inline.location().expect("location");
        assert_eq!(
            (location.start_line(), location.line(), location.thread_id()),
            (Some(20), Some(24), Some("PRRT_1"))
        );
        assert_eq!(
            (location.is_resolved(), location.is_outdated()),
            (Some(true), Some(false))
        );
        let PullRequestTimelineItem::Comment(general) = &items[2] else {
            panic!("expected a comment, got {:?}", items[2]);
        };
        assert_eq!(
            (general.thread_id(), general.thread_is_resolved()),
            (Some("disc-2"), Some(true))
        );
        assert_eq!((general.created_at(), general.url()), (None, None));
    });
}

#[test]
fn a_refused_pull_request_timeline_reports_why_and_the_auth_state() {
    smol::block_on(async {
        let (client, mut daemon) = connect(json!({})).await;
        let timeline = smol::spawn({
            let client = client.clone();
            async move { client.pull_request_timeline("/w", 7, "o", "r").await }
        });
        let request = daemon.request().await;
        daemon
            .reply(
                &request,
                "pull_request_timeline_response",
                json!({"cwd": "/w", "prNumber": 7, "items": [],
                       "error": {"kind": "rate_limited", "message": "GitHub CLI is unavailable"},
                       "githubFeaturesEnabled": false, "authState": "cli_missing"}),
            )
            .await;
        let timeline = timeline.await.expect("timeline");
        let error = timeline.error().expect("error");
        assert_eq!(
            (error.kind(), error.message()),
            (TimelineErrorKind::Unknown, "GitHub CLI is unavailable")
        );
        assert_eq!(timeline.auth_state(), ForgeAuthState::CliMissing);
    });
}

#[test]
fn a_rejected_checkout_mutation_keeps_its_code() {
    smol::block_on(async {
        let (client, mut daemon) = connect(json!({})).await;
        let merged = smol::spawn({
            let client = client.clone();
            async move { client.checkout_merge_from_base("/w", None, true).await }
        });
        let request = daemon.request().await;
        daemon
            .reply(
                &request,
                "checkout_merge_from_base_response",
                json!({"cwd": "/w", "success": false,
                       "error": {"code": "MERGE_CONFLICT", "message": "Merge conflict in a.txt"}}),
            )
            .await;
        match merged.await {
            Err(PaseoError::Checkout(rejection)) => assert_eq!(
                (rejection.code(), rejection.message()),
                (CheckoutErrorCode::MergeConflict, "Merge conflict in a.txt")
            ),
            other => panic!("expected the daemon's rejection, got {other:?}"),
        }
    });
}

#[test]
fn a_commit_list_larger_than_sixteen_mebibytes_arrives_whole() {
    smol::block_on(async {
        let (client, mut daemon) = connect(json!({"commitsList": true})).await;
        let listed = smol::spawn({
            let client = client.clone();
            async move { client.list_commits("/w").await }
        });
        let request = daemon.request().await;
        let files = (0..10)
            .map(|file| {
                json!({"path": format!("packages/server/src/generated/module-{file:04}.ts"),
                       "additions": 1, "deletions": 1, "status": "modified"})
            })
            .collect::<Vec<_>>();
        let commits = (0..24_000)
            .map(|commit| {
                json!({"sha": format!("{commit:040x}"), "shortSha": format!("{commit:07x}"),
                       "subject": "chore: sync upstream", "authorName": "Paseo",
                       "authorDate": "2026-11-17T00:00:00Z", "isOnRemote": true,
                       "isOnBase": false, "files": files})
            })
            .collect::<Vec<_>>();
        let payload = json!({"cwd": "/w", "baseRef": "main", "commits": commits, "error": null});
        assert!(payload.to_string().len() > 20 << 20);
        daemon
            .reply(&request, "checkout.commits.list.response", payload)
            .await;
        let list = listed.await.expect("the whole commit list");
        assert_eq!(list.commits().len(), 24_000);
        assert!(client.is_connected());
    });
}

#[test]
fn a_pre_forge_daemon_is_searched_with_github_kinds_and_its_feature_flags() {
    smol::block_on(async {
        let (client, mut daemon) = connect(json!({})).await;
        let replies = [
            (
                json!({"featuresEnabled": false}),
                ForgeAuthState::Unauthenticated,
            ),
            (
                json!({"githubFeaturesEnabled": false}),
                ForgeAuthState::Unauthenticated,
            ),
            (json!({}), ForgeAuthState::Authenticated),
            (
                json!({"featuresEnabled": true, "authState": "cli_missing"}),
                ForgeAuthState::CliMissing,
            ),
        ];
        for (flags, expected) in replies {
            let search = smol::spawn({
                let client = client.clone();
                async move {
                    client
                        .search_forge(
                            "/w",
                            "bug",
                            &[ForgeItemKind::ChangeRequest, ForgeItemKind::Issue],
                            None,
                        )
                        .await
                }
            });
            let request = daemon.request().await;
            assert_eq!(request["type"], "github_search_request");
            assert_eq!(request["kinds"], json!(["github-pr", "github-issue"]));
            let mut payload = json!({"items": [
                {"kind": "pr", "number": 3, "title": "Fix", "url": "u", "state": "open",
                 "body": null, "labels": []},
                {"kind": "discussion", "number": 4}
            ]});
            payload
                .as_object_mut()
                .expect("payload")
                .extend(flags.as_object().expect("flags").clone());
            daemon
                .reply(&request, "github_search_response", payload)
                .await;
            let results = search.await.expect("search");
            assert_eq!(results.auth_state(), expected);
            assert_eq!(results.items().len(), 1);
            assert_eq!(results.items()[0].kind(), ForgeItemKind::ChangeRequest);
        }
    });
}

#[test]
fn a_refused_creation_subscription_reports_the_daemon_error() {
    smol::block_on(async {
        let (client, mut daemon) = connect(json!({"creationLifecycle": true})).await;
        let observed = smol::spawn({
            let client = client.clone();
            async move { client.observe_creation(CreationKind::Agent, "key").await }
        });
        let request = daemon.request().await;
        assert_eq!(request["type"], "creation.subscribe.request");
        daemon
            .reply(
                &request,
                "creation.subscribe.response",
                json!({"snapshot": null, "error": "creation not found"}),
            )
            .await;
        match observed.await {
            Err(PaseoError::Rpc(error)) => assert_eq!(error.message(), "creation not found"),
            Err(error) => panic!("expected the daemon's error, got {error:?}"),
            Ok(_) => panic!("expected the daemon's error"),
        }
    });
}

#[test]
fn requests_a_daemon_does_not_advertise_are_refused() {
    smol::block_on(async {
        let (client, _daemon) = connect(json!({})).await;
        let searched = client.search_timeline("agent", "query", None).await;
        assert!(
            matches!(
                searched,
                Err(PaseoError::UnsupportedDaemon {
                    feature: "agentHistorySearch"
                })
            ),
            "{searched:?}"
        );
        let observed = client.observe_creation(CreationKind::Agent, "key").await;
        assert!(
            matches!(
                observed,
                Err(PaseoError::UnsupportedDaemon {
                    feature: "creationLifecycle"
                })
            ),
            "{:?}",
            observed.err()
        );
    });
}

#[test]
fn a_removed_line_comment_is_sent_with_its_side_only_to_a_daemon_that_keeps_it() {
    smol::block_on(async {
        let comment = ReviewCommentInput::new(
            "comment-1".to_owned(),
            FileReference::new("/repo".to_owned(), "/repo/gone.rs".to_owned()),
            11..=13,
            CommentContext::new(String::new(), "fn gone() {}".to_owned(), String::new()),
            "Why?".to_owned(),
        )
        .with_side(CommentSide::Old);

        let (client, _daemon) = connect(json!({"agentReview": true})).await;
        let refused = client.save_review_comment("agent-1", &comment).await;
        assert!(
            matches!(
                refused,
                Err(PaseoError::UnsupportedDaemon {
                    feature: "agentReviewRemovedLines"
                })
            ),
            "{refused:?}"
        );

        let (client, mut daemon) =
            connect(json!({"agentReview": true, "agentReviewRemovedLines": true})).await;
        let saved = smol::spawn({
            let client = client.clone();
            let comment = comment.clone();
            async move { client.save_review_comment("agent-1", &comment).await }
        });
        let request = daemon.request().await;
        assert_eq!(request["type"], "agent.review.save_comment.request");
        assert_eq!(
            (
                &request["comment"]["side"],
                &request["comment"]["startLine"]
            ),
            (&json!("old"), &json!(11))
        );
        daemon
            .reply(
                &request,
                "agent.review.save_comment.response",
                json!({"agentId": "agent-1", "error": null}),
            )
            .await;
        saved.await.expect("saved");
    });
}

#[test]
fn a_file_at_revision_arrives_as_text_or_its_typed_error() {
    smol::block_on(async {
        let (client, _daemon) = connect(json!({})).await;
        let refused = client.file_at_revision("/w", "a.rs", "HEAD", None).await;
        assert!(
            matches!(
                refused,
                Err(PaseoError::UnsupportedDaemon {
                    feature: "checkoutFileAtRevision"
                })
            ),
            "{refused:?}"
        );

        let (client, mut daemon) = connect(json!({"checkoutFileAtRevision": true})).await;
        let read = smol::spawn({
            let client = client.clone();
            async move {
                client
                    .file_at_revision("/w", "src/a.rs", "HEAD", None)
                    .await
            }
        });
        let request = daemon.request().await;
        assert_eq!(
            request,
            json!({"type": "checkout.file_at_revision.request", "cwd": "/w",
                   "path": "src/a.rs", "revision": "HEAD", "requestId": request["requestId"]})
        );
        daemon
            .reply(
                &request,
                "checkout.file_at_revision.response",
                json!({"cwd": "/w", "path": "src/a.rs", "revision": "HEAD",
                       "content": "fn a() {}\r\n", "error": null}),
            )
            .await;
        assert_eq!(read.await.expect("the text"), "fn a() {}\r\n");

        let read = smol::spawn({
            let client = client.clone();
            async move {
                client
                    .file_at_revision("/w", "logo.png", "HEAD", None)
                    .await
            }
        });
        let request = daemon.request().await;
        daemon
            .reply(
                &request,
                "checkout.file_at_revision.response",
                json!({"cwd": "/w", "path": "logo.png", "revision": "HEAD", "content": null,
                       "error": {"code": "BINARY", "message": "logo.png at HEAD is a binary file"}}),
            )
            .await;
        match read.await {
            Err(PaseoError::Checkout(rejection)) => {
                assert_eq!(rejection.code(), CheckoutErrorCode::Binary);
            }
            other => panic!("expected the daemon's rejection, got {other:?}"),
        }
    });
}

#[test]
fn push_tokens_register_by_message_and_unregister_by_request() {
    smol::block_on(async {
        let token = "apns:sandbox:ABCDE12345:sh.ghost.paseo.ios:a1a1";
        let (client, _daemon) = connect(json!({})).await;
        assert!(!client.server_info().supports(DaemonFeature::ApnsPush));
        let refused = client.unregister_push_token(token).await;
        assert!(
            matches!(
                refused,
                Err(PaseoError::UnsupportedDaemon {
                    feature: "pushTokenRevocation"
                })
            ),
            "{refused:?}"
        );

        let (client, mut daemon) =
            connect(json!({"apnsPush": true, "pushTokenRevocation": true})).await;
        assert!(client.server_info().supports(DaemonFeature::ApnsPush));
        client.register_push_token(token).expect("register");
        assert_eq!(
            daemon.request().await,
            json!({"type": "register_push_token", "token": token})
        );
        let unregistering = smol::spawn({
            let client = client.clone();
            async move { client.unregister_push_token(token).await }
        });
        let request = daemon.request().await;
        assert_eq!(
            request,
            json!({"type": "push.unregister.request", "token": token,
                   "requestId": request["requestId"]})
        );
        daemon
            .reply(&request, "push.unregister.response", json!({}))
            .await;
        unregistering.await.expect("unregistered");
    });
}
