//! Actual Editor, SQLite, Owner, operation and request builder; fake HTTP only.
use super::*;
use crate::drafts::{network::Attempt, schedule::test_once};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};

fn attempt(outcome: Outcome) -> Attempt {
    Attempt {
        outcome,
        cooldown_until_ms: None,
        issue: None,
        dispatched: true,
        raw: None,
    }
}

fn assert_legacy_schedule_blocked(view: crate::drafts::View, status: &str) {
    use crate::drafts::{integration::legacy_allowed_in, session};
    let scope = view.stored.as_ref().unwrap().state.scope().clone();
    assert_eq!(session::status(&view), status);
    let mut shell = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(scope.account_id.clone());
    shell.core.account_epoch = 9;
    shell.managed_drafts = session::Session {
        account: Some(scope.account_id.clone()),
        epoch: 9,
        ready: true,
        ..Default::default()
    };
    assert!(
        shell
            .managed_drafts
            .apply_view(&scope.account_id, 9, scope.key.clone(), view)
    );
    for (fixture, managed_schedule) in [(false, false), (true, true)] {
        assert!(
            legacy_allowed_in(&shell, &scope.key, fixture, managed_schedule).is_err(),
            "Managed scheduling must not fall back to legacy: {status}"
        );
        let other = DraftKey {
            channel: "unmanaged".into(),
            ..scope.key.clone()
        };
        assert!(legacy_allowed_in(&shell, &other, fixture, managed_schedule).is_ok());
        shell.managed_drafts.ready = false;
        assert!(legacy_allowed_in(&shell, &other, fixture, managed_schedule).is_err());
        shell.managed_drafts.ready = true;
    }
    // Ordinary rendering fixtures retain their explicitly isolated legacy UI.
    assert!(legacy_allowed_in(&shell, &scope.key, true, false).is_ok());
}

#[tokio::test]
async fn managed_schedule_pending_and_uncertain_keep_legacy_unavailable_with_newer_local() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let frozen = f
        .editor
        .consume_reviewed_schedule(lease, visit, raw, 500)
        .await
        .unwrap();
    let intent = frozen.state.consumption().unwrap().clone();
    let owner = f.editor.owner();
    assert_legacy_schedule_blocked(owner.load().await.unwrap(), "Saved schedule waiting");
    test_once(&owner, &intent, |_| async {
        assert_legacy_schedule_blocked(
            owner.load().await.unwrap(),
            "Waiting for schedule confirmation",
        );
        attempt(Outcome::Uncertain)
    })
    .await
    .unwrap();
    assert_legacy_schedule_blocked(owner.load().await.unwrap(), "Schedule unconfirmed");
    f.editor.refresh_receipt().await.unwrap();
    let current = local("Independent newer composer text");
    f.editor.submit(current.clone());
    f.editor.flush().await.unwrap();
    let view = owner.load().await.unwrap();
    assert!(same_local(&view.local, &current));
    assert_eq!(
        view.stored.as_ref().unwrap().state.consumption(),
        Some(&intent)
    );
    assert_legacy_schedule_blocked(view, "Schedule unconfirmed");
}
fn wire(operation: &Operation, raw: &RawSnapshot) {
    assert!(operation.known_schedule_check());
    assert_eq!(operation.snapshot.as_ref(), Some(raw));
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Disconnected".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-token".into(),
    };
    let prepared =
        api::prepare_draft_operation(&SlackClient::default(), &workspace, operation, 1000).unwrap();
    assert!(prepared.request.url.contains("/api/drafts.update"));
    let RequestBody::Form(fields) = prepared.request.body else {
        panic!()
    };
    let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(fields["draft_id"], "Dr1");
    assert!(!fields.contains_key("client_msg_id")); // Known-ID update keeps UUID in the owner, not this wire form.
    assert_eq!(fields["client_last_updated_ts"], "1000");
    assert_eq!(fields["is_from_composer"], "true");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&fields["blocks"]).unwrap(),
        serde_json::json!(raw.blocks)
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&fields["destinations"]).unwrap(),
        serde_json::json!(raw.destinations)
    );
    assert_eq!(fields["date_scheduled"], "500");
    assert!(!fields.contains_key("consume_id"));
}

#[tokio::test]
async fn actual_schedule_pipeline_freezes_rich_channel_and_thread_and_retains_newer_unsupported_local()
 {
    for thread in [false, true] {
        let f = Fixture::new(thread).await;
        let (lease, visit, raw) = f.lease().await;
        let frozen = f
            .editor
            .consume_reviewed_schedule(lease, visit, raw.clone(), 500)
            .await
            .unwrap();
        let intent = frozen.state.consumption().unwrap().clone();
        let editor = f.editor.clone();
        let owner = editor.owner();
        let (receipt, _) = test_once(&owner, &intent, |operation| async move {
            wire(&operation, &raw);
            assert_eq!(
                editor
                    .owner()
                    .load()
                    .await
                    .unwrap()
                    .stored
                    .unwrap()
                    .state
                    .pending(),
                Some(&operation)
            );
            editor.submit(newer());
            editor.flush().await.unwrap();
            attempt(positive(&operation))
        })
        .await
        .unwrap();
        assert!(receipt.applied);
        let after = f.editor.refresh_receipt().await.unwrap();
        assert!(same_local(&after.view.local, &newer()));
        assert!(
            after
                .view
                .stored
                .as_ref()
                .unwrap()
                .state
                .consumption()
                .is_none()
        );
        assert!(after.view.stored.is_some()); // No implicit generic release.
    }
}

#[tokio::test]
async fn uncertain_schedule_pipeline_never_replays_frozen_intent() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let frozen = f
        .editor
        .consume_reviewed_schedule(lease, visit, raw, 500)
        .await
        .unwrap();
    let intent = frozen.state.consumption().unwrap().clone();
    test_once(&f.editor.owner(), &intent, |_| async {
        attempt(Outcome::Uncertain)
    })
    .await
    .unwrap();
    let called = std::sync::atomic::AtomicBool::new(false);
    assert!(
        test_once(&f.editor.owner(), &intent, |_| async {
            called.store(true, std::sync::atomic::Ordering::SeqCst);
            attempt(Outcome::Uncertain)
        })
        .await
        .is_err()
    );
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        f.editor
            .owner()
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .consumption(),
        Some(&intent)
    );
}

#[tokio::test]
async fn definite_schedule_rejection_retries_only_explicit_same_rich_intent_with_new_attempt() {
    let f = Fixture::new(true).await;
    let (lease, visit, raw) = f.lease().await;
    let frozen = f
        .editor
        .consume_reviewed_schedule(lease, visit, raw.clone(), 500)
        .await
        .unwrap();
    let intent = frozen.state.consumption().unwrap().clone();
    let (first, _) = test_once(&f.editor.owner(), &intent, |_| async {
        attempt(Outcome::Rejected)
    })
    .await
    .unwrap();
    f.editor.refresh_receipt().await.unwrap();
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let retry = f
        .editor
        .apply_rejected_schedule_review(
            lease,
            visit,
            super_platinum_core::drafts::sync::RejectedScheduleChoice::Retry,
            200,
        )
        .await
        .unwrap();
    assert_eq!(retry.state.consumption(), Some(&intent));
    let (second, _) = test_once(&f.editor.owner(), &intent, |operation| async move {
        wire(&operation, &raw);
        attempt(Outcome::Rejected)
    })
    .await
    .unwrap();
    assert_ne!(first.operation.id, second.operation.id);
    assert_eq!(first.operation.consume_id, second.operation.consume_id);
    assert_eq!(first.operation.snapshot, second.operation.snapshot);
}
