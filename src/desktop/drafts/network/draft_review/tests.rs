use super::*;
use serde_json::json;
use std::cell::Cell;
use super_platinum_core::{
    ComposerState,
    drafts::{
        DraftKey, LocalDraft, codec,
        store::sync::Stored,
        sync::{DraftSync, RemoteDraft, Scope},
    },
};
fn fixture() -> (Binding, LocalReview, Value) {
    let binding = Binding {
        account: super_platinum_core::drafts::store::LocalWriter::default()
            .id()
            .into(),
        epoch: 7,
        transport: Arc::new(Transport::new("fixture-only-cookie").unwrap()),
        client: SlackClient::default(),
        workspace: WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only-token".into(),
        },
        cooldown_until_ms: Arc::new(Mutex::new(0)),
    };
    let scope = Scope {
        account_id: binding.account.clone(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let local = LocalDraft {
        composer: ComposerState::with_text("local"),
        attachments: vec![],
    };
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: super_platinum_core::drafts::store::LocalWriter::default()
            .id()
            .into(),
        last_updated_ts: "10.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &local.composer).unwrap(),
    };
    let state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    let latest = codec::encode_composer(&scope.key, &ComposerState::with_text("remote")).unwrap();
    let raw = json!({"ok":true,"draft":{"id":base.id,"client_msg_id":base.client_msg_id,"last_updated_ts":"11.000001","is_sent":false,"is_deleted":false,"is_from_composer":false,"date_scheduled":0,"blocks":latest.blocks,"file_ids":latest.file_ids,"destinations":latest.destinations}});
    (
        binding,
        LocalReview {
            scope,
            local,
            stored: Some(Stored { revision: 1, state }),
            blocked: None,
            alternates: vec![],
        },
        raw,
    )
}
#[tokio::test]
async fn positive_info_is_one_identity_only_read_with_exact_inventory_and_raw() {
    let (binding, review, raw) = fixture();
    let calls = Cell::new(0);
    let result = binding
        .read_remote_review_with(review.clone(), 1000, |request| {
            calls.set(calls.get() + 1);
            assert_eq!(
                request.url.split('?').next(),
                Some("https://fixture.slack.com/api/drafts.info")
            );
            assert!(!request.retry_safe());
            assert_eq!(
                request.body,
                super_platinum_core::slack::client::RequestBody::Form(vec![
                    ("token".into(), "fixture-only-token".into()),
                    ("draft_id".into(), "D1".into())
                ])
            );
            std::future::ready(Ok(raw.clone()))
        })
        .await;
    assert_eq!(calls.get(), 1);
    assert!(result.requested && result.issue.is_none());
    let proof = result.proof.unwrap();
    assert_eq!(proof.review().raw_info(), &raw);
    assert_eq!(
        proof.review().inventory().stored.as_ref().unwrap().state,
        review.stored.unwrap().state
    );
}
#[tokio::test]
async fn wrong_account_team_and_pending_state_have_no_request() {
    for case in 0..4 {
        let (binding, mut review, _) = fixture();
        match case {
            0 => review.scope.account_id = "other".into(),
            1 => review.scope.key.team = "T2".into(),
            2 => review.stored = None,
            _ => {
                let scope = review.scope.clone();
                let state = &mut review.stored.as_mut().unwrap().state;
                state.edit(&scope, None);
                assert!(state.begin_operation(&scope).is_some());
            }
        }
        let result = binding
            .read_remote_review_with(review, 1000, |_| async { panic!("must not read") })
            .await;
        assert!(!result.requested && result.proof.is_none());
    }
}
#[tokio::test]
async fn absence_error_contradiction_and_partial_payload_never_resolve() {
    for case in 0..4 {
        let (binding, review, mut raw) = fixture();
        match case {
            0 => raw = json!({"ok":false,"error":"draft_not_found"}),
            1 => raw["error"] = json!("failed"),
            2 => {
                raw["draft"].as_object_mut().unwrap().remove("file_ids");
            }
            _ => raw["draft"]["last_updated_ts"] = json!("11.1"),
        }
        let expected = raw.clone();
        let result = binding
            .read_remote_review_with(review, 1000, |_| std::future::ready(Ok(raw)))
            .await;
        assert!(result.requested && result.proof.is_none());
        assert_eq!(result.raw, Some(expected));
    }
}
#[tokio::test]
async fn rate_limit_blocks_reopened_binding_without_automatic_retry() {
    let (binding, review, _) = fixture();
    let calls = Cell::new(0);
    let result = binding
        .read_remote_review_with(review.clone(), 1000, |_| {
            calls.set(calls.get() + 1);
            std::future::ready(Err(Error::RateLimited {
                retry_after_secs: Some(60),
            }))
        })
        .await;
    assert_eq!(calls.get(), 1);
    assert_eq!(result.issue, Some(Issue::RateLimited));
    let until = result.cooldown_until_ms.unwrap();
    assert!(until >= 61000);
    let mut reopened = binding.clone();
    reopened.cooldown_until_ms = Arc::new(Mutex::new(0));
    let result = reopened
        .read_remote_review_with(review, 2000, |_| async { panic!("cooldown bypass") })
        .await;
    assert!(!result.requested);
    assert_eq!(result.cooldown_until_ms, Some(until));
}
