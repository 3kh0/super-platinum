//! The HTTP Attempt is synthetic. Storage, Editor, owner, operation builder and
//! single-attempt dispatch/receipt path are the real shared implementation.
use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    drafts::sync::{RemoteDraft, Scope},
    slack::{SlackClient, api, client::RequestBody},
};

const ROOT: &str = "100.000001";
const BASE: &str = "Previous acknowledged text";
const FROZEN: &str = "Frozen send \u{65e5}\u{672c}\u{8a9e}\nsecond line";

fn local_at(text: &str, thread: Option<&str>) -> LocalDraft {
    let mut value = local(text);
    value.composer.reply_broadcast = thread.is_some();
    value
}

impl Fixture {
    async fn acknowledged(account_id: &str, thread: Option<&str>) -> Self {
        let id =
            api::ForwardMessage::new("C1".into(), ROOT.into(), "C2".into(), vec![]).client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-acknowledged-send-{id}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: thread.map(str::to_owned),
        };
        let original = local_at(BASE, thread);
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), original.clone())])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: account_id.into(),
            path: path.clone(),
        });
        let owner = account.owner(key.clone());
        let remote = RemoteDraft {
            scope: Scope {
                account_id: account_id.into(),
                key: key.clone(),
            },
            id: "DrAcknowledged".into(),
            client_msg_id: id,
            last_updated_ts: "100.000002".into(),
            snapshot: crate::drafts::codec::encode_composer(&key, &original.composer).unwrap(),
        };
        owner.enroll(original, Some(remote)).await.unwrap();
        Self {
            path,
            account,
            owner,
        }
    }
    async fn frozen(&self, queue: &mut Queue) -> (Arc<Editor>, ConsumeIntent) {
        let editor = self.editor(queue).await;
        let local = local_at(FROZEN, self.owner.scope.key.thread.as_deref());
        editor.submit(local.clone());
        let view = editor.flush().await.unwrap();
        assert!(can_consume(&view));
        let consumed = editor.consume(local, ConsumeKind::Send).await.unwrap();
        (editor, consumed.state.consumption().unwrap().clone())
    }
}

fn assert_request(op: &Operation, intent: &ConsumeIntent) {
    assert_eq!(op.kind, OperationKind::Send);
    assert_eq!(op.scope, intent.scope);
    assert_eq!(op.snapshot.as_ref(), Some(&intent.snapshot));
    assert_eq!(op.base.as_ref(), intent.base.as_ref());
    assert_eq!(op.client_msg_id, intent.client_msg_id);
    assert_eq!(op.consume_id.as_deref(), Some(intent.id.as_str()));
    assert_eq!(op.base.as_ref().unwrap().id, "DrAcknowledged");
    assert_ne!(op.base.as_ref().unwrap().snapshot, intent.snapshot);
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Disconnected fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-a-token".into(),
    };
    let prepared =
        api::prepare_draft_operation(&SlackClient::default(), &workspace, op, 1_800_000_000_000)
            .unwrap();
    assert!(prepared.request.url.contains("/api/chat.postMessage"));
    let RequestBody::Form(fields) = prepared.request.body else {
        panic!()
    };
    let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(fields["draft_id"], "DrAcknowledged");
    assert_eq!(fields["client_msg_id"], intent.client_msg_id);
    assert_eq!(fields["channel"], intent.scope.key.channel);
    assert_eq!(fields.get("thread_ts"), intent.scope.key.thread.as_ref());
    if intent.scope.key.thread.is_some() {
        assert_eq!(fields["reply_broadcast"], "true");
    } else {
        assert!(!fields.contains_key("reply_broadcast"));
    }
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&fields["blocks"]).unwrap(),
        serde_json::json!(intent.snapshot.blocks)
    );
    for unexpected in [
        "files",
        "date_scheduled",
        "is_from_composer",
        "operation_id",
        "consume_id",
    ] {
        assert!(!fields.contains_key(unexpected));
    }
}

#[tokio::test]
async fn acknowledged_idle_admission_keeps_alternate_and_pending_operation_barriers() {
    let f = Fixture::acknowledged(ACCOUNT, None).await;
    let view = f.owner.load().await.unwrap();
    assert!(can_consume(&view));
    let mut blocked = view.clone();
    blocked.uncommitted = Some(local("Uncommitted"));
    assert!(!can_consume(&blocked));
    let mut blocked = view.clone();
    blocked.blocked = Some(super_platinum_core::drafts::store::sync::BlockedLocal {
        token: "fixture".into(),
        draft: local("Alternate"),
    });
    assert!(!can_consume(&blocked));
    let mut pending = view.clone();
    let state = &mut pending.stored.as_mut().unwrap().state;
    state.edit(
        &f.owner.scope,
        Some(crate::drafts::codec::encode(&f.owner.scope.key, FROZEN).unwrap()),
    );
    assert_eq!(
        state.begin_operation(&f.owner.scope).unwrap().kind,
        OperationKind::Update
    );
    assert!(!can_consume(&pending));
    let mut consuming = view.clone();
    let state = &mut consuming.stored.as_mut().unwrap().state;
    state
        .request_consume(&f.owner.scope, state.local_revision(), ConsumeKind::Send)
        .unwrap();
    assert!(!can_consume(&consuming));
    let op = consuming
        .stored
        .as_mut()
        .unwrap()
        .state
        .begin_operation(&f.owner.scope)
        .unwrap();
    consuming
        .stored
        .as_mut()
        .unwrap()
        .state
        .reject(&f.owner.scope, &op.id);
    assert!(!can_consume(&consuming));
    let mut conflict = view;
    let state = &mut conflict.stored.as_mut().unwrap().state;
    let mut remote = state.acknowledged().unwrap().clone();
    remote.last_updated_ts = "101.000001".into();
    assert!(state.observe_conflict(remote));
    assert!(!can_consume(&conflict));
    let local_only = Fixture::new(ACCOUNT).await;
    let mut creating = local_only.owner.load().await.unwrap();
    assert_eq!(
        creating
            .stored
            .as_mut()
            .unwrap()
            .state
            .begin_operation(&local_only.owner.scope)
            .unwrap()
            .kind,
        OperationKind::Create
    );
    assert!(!can_consume(&creating));
}

#[tokio::test]
async fn acknowledged_channel_and_thread_send_freeze_payload_and_preserve_newer_local_input() {
    for thread in [None, Some(ROOT)] {
        let f = Fixture::acknowledged(ACCOUNT, thread).await;
        let (editor, intent) = f.frozen(&mut Queue::default()).await;
        let calls = AtomicUsize::new(0);
        let calls_ref = &calls;
        let expected = &intent;
        let owner = &f.owner;
        let writer = editor.clone();
        let (receipt, outcome) = dispatch_once_with(owner, &intent, |operation| async move {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            assert_request(&operation, expected);
            assert_eq!(
                owner.load().await.unwrap().stored.unwrap().state.pending(),
                Some(&operation)
            );
            writer.submit(local_at("Newer local input", thread));
            writer.flush().await.unwrap();
            assert_eq!(
                owner.load().await.unwrap().stored.unwrap().state.pending(),
                Some(&operation)
            );
            sent(&operation)
        })
        .await
        .unwrap();
        assert!(receipt.applied);
        assert!(matches!(outcome, Outcome::Sent { .. }));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let saved = f.owner.load().await.unwrap();
        assert_eq!(saved.local.composer.text, "Newer local input");
        let state = saved.stored.unwrap().state;
        assert!(state.acknowledged().is_none());
        assert!(state.consumption().is_none());
        assert_ne!(state.client_msg_id(), intent.client_msg_id);
        editor.release().await.unwrap();
        assert_eq!(
            f.owner.load().await.unwrap().local.composer.text,
            "Newer local input"
        );
    }
}

#[tokio::test]
async fn acknowledged_lost_ack_retains_original_identity_and_never_dispatches_again_after_restart()
{
    for thread in [None, Some(ROOT)] {
        let f = Fixture::acknowledged(ACCOUNT, thread).await;
        let (_, intent) = f.frozen(&mut Queue::default()).await;
        let (receipt, _) = dispatch_once_with(&f.owner, &intent, |operation| async move {
            let mut attempt = sent(&operation);
            attempt.outcome = Outcome::Uncertain;
            attempt
        })
        .await
        .unwrap();
        let restarted = f.restart();
        let saved = restarted.load().await.unwrap();
        assert!(!can_consume(&saved));
        assert_eq!(
            saved.stored.as_ref().unwrap().state.pending(),
            Some(&receipt.operation)
        );
        assert_eq!(
            saved.stored.as_ref().unwrap().state.consumption(),
            Some(&intent)
        );
        let calls = AtomicUsize::new(0);
        let count = &calls;
        assert!(matches!(
            dispatch_once_with(&restarted, &intent, |operation| async move {
                count.fetch_add(1, Ordering::SeqCst);
                sent(&operation)
            })
            .await,
            Err(Fault::Pending)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn acknowledged_receipt_disk_failure_preserves_prepared_operation_without_replay() {
    for thread in [None, Some(ROOT)] {
        let f = Fixture::acknowledged(ACCOUNT, thread).await;
        let (_, intent) = f.frozen(&mut Queue::default()).await;
        let backup = f.path.with_extension("backup");
        let mut dispatched = None;
        let result = dispatch_once_with(&f.owner, &intent, |operation| {
            dispatched = Some(operation.clone());
            let path = &f.path;
            let backup = &backup;
            async move {
                std::fs::rename(path, backup).unwrap();
                std::fs::create_dir(path).unwrap();
                sent(&operation)
            }
        })
        .await;
        std::fs::remove_dir(&f.path).unwrap();
        std::fs::rename(&backup, &f.path).unwrap();
        assert!(matches!(result, Err(Fault::Storage)));
        let restarted = f.restart();
        let saved = restarted.load().await.unwrap();
        assert_eq!(
            saved.stored.as_ref().unwrap().state.pending(),
            dispatched.as_ref()
        );
        assert!(matches!(
            saved.stored.as_ref().unwrap().state.workflow(),
            Workflow::Uncertain(_)
        ));
        let calls = AtomicUsize::new(0);
        let count = &calls;
        assert!(
            dispatch_once_with(&restarted, &intent, |operation| async move {
                count.fetch_add(1, Ordering::SeqCst);
                sent(&operation)
            })
            .await
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn acknowledged_prepare_disk_failure_never_calls_http_and_keeps_durable_intent() {
    let f = Fixture::acknowledged(ACCOUNT, None).await;
    let (_, intent) = f.frozen(&mut Queue::default()).await;
    let backup = f.path.with_extension("backup");
    std::fs::rename(&f.path, &backup).unwrap();
    std::fs::create_dir(&f.path).unwrap();
    let calls = AtomicUsize::new(0);
    let count = &calls;
    let result = dispatch_once_with(&f.owner, &intent, |operation| async move {
        count.fetch_add(1, Ordering::SeqCst);
        sent(&operation)
    })
    .await;
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&backup, &f.path).unwrap();
    assert!(matches!(result, Err(Fault::Storage)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let saved = f.owner.load().await.unwrap().stored.unwrap();
    assert_eq!(saved.state.consumption(), Some(&intent));
    assert!(saved.state.pending().is_none());
}

#[tokio::test]
async fn stale_acknowledged_click_cannot_consume_newer_checkpointed_input() {
    for thread in [None, Some(ROOT)] {
        let f = Fixture::acknowledged(ACCOUNT, thread).await;
        let editor = f.editor(&mut Queue::default()).await;
        editor.submit(local_at("Newer checkpoint", thread));
        editor.flush().await.unwrap();
        assert!(matches!(
            editor
                .consume(local_at(BASE, thread), ConsumeKind::Send)
                .await,
            Err(Fault::Stale)
        ));
        let saved = f.owner.load().await.unwrap();
        assert_eq!(saved.local.composer.text, "Newer checkpoint");
        assert!(saved.stored.as_ref().unwrap().state.consumption().is_none());
        assert!(saved.stored.unwrap().state.pending().is_none());
    }
}

#[tokio::test]
async fn captured_acknowledged_owner_dispatches_after_account_switch_without_touching_replacement()
{
    for thread in [None, Some(ROOT)] {
        let first = Fixture::acknowledged(ACCOUNT, thread).await;
        let second = Fixture::acknowledged("00000000-0000-4000-8000-000000000099", thread).await;
        let mut queue = Queue::default();
        let (_, intent) = first.frozen(&mut queue).await;
        let replacement = second.editor(&mut queue).await;
        replacement.submit(local_at("Other account input", thread));
        replacement.flush().await.unwrap();
        let before = second.owner.load().await.unwrap().stored.unwrap();
        let expected = &intent;
        let (receipt, _) = dispatch_once_with(&first.owner, &intent, |operation| async move {
            assert_eq!(operation.scope.account_id, ACCOUNT);
            assert_request(&operation, expected);
            sent(&operation)
        })
        .await
        .unwrap();
        assert!(receipt.applied);
        let after = second.owner.load().await.unwrap();
        assert_eq!(after.local.composer.text, "Other account input");
        let after = after.stored.unwrap();
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.state, before.state);
    }
}
