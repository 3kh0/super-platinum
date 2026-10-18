//! Remote/Conflict proofs are produced by actual isolated worker transitions.
//! Synthetic positive mutation outcome seeds storage only; no request is sent.
use super::super::super::super::account::wait;
use super::*;
use super_platinum_core::drafts::sync::{DraftObservation, ObservationKind, RemoteDraft, Workflow};
async fn remote_fixture(
    conflict: bool,
) -> (
    durable::test_support::Isolated,
    Arc<Account>,
    Owner,
    durable::Review,
) {
    let path = std::env::temp_dir().join(format!(
        "managed-remote-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let id = "00000000-0000-4000-8000-000000000681";
    let store = durable::test_support::Isolated::create(id.into(), path).unwrap();
    let account = Account::isolated(id.into(), store.client());
    account.initialize().await.unwrap();
    let owner = account.owner(key()).unwrap();
    let client = account.client().unwrap();
    let inventory = owner.local_inventory().await.unwrap();
    let initial = wait(client.enroll_local(owner.scope().clone(), inventory))
        .await
        .unwrap();
    let writer = wait(client.register_writer(initial.clone())).await.unwrap();
    let serial = writer.initial_serial() + 1;
    wait(client.checkpoint(writer, serial, initial, local("initial"), None))
        .await
        .unwrap();
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    let mut stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    let before = owner.review().await.unwrap();
    // Seed a locally-created remote through the existing worker protocol.
    // The implementation under test never calls verify_local.
    let verified = wait(client.verify_local(stream.clone(), before))
        .await
        .unwrap();
    let ticket = wait(client.prepare(stream.clone(), verified))
        .await
        .unwrap();
    let claimed = wait(client.claim_dispatch(ticket)).await.unwrap();
    let (op, receipt) = claimed.into_parts();
    let mut remote = RemoteDraft {
        scope: op.scope.clone(),
        id: "D1".into(),
        client_msg_id: op.client_msg_id.clone(),
        last_updated_ts: "10.000001".into(),
        snapshot: op.snapshot.unwrap(),
    };
    let mut review = wait(client.record(receipt, durable::Outcome::Active(remote.clone()), 0))
        .await
        .unwrap();
    // An ACK changes remote identity and deliberately invalidates Local basis.
    // Establish Remote only through the real positive-baseline protocol.
    assert_eq!(review.row().lineage().basis(), durable::Basis::Unverified);
    assert_eq!(review.row().lineage().verified_epoch(), 0);
    assert!(
        wait(client.prepare(stream.clone(), review.clone()))
            .await
            .is_err()
    );
    wait(client.mark_gap(stream, durable::GapReason::Disconnect))
        .await
        .unwrap();
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    review = owner.review().await.unwrap();
    let ticket = wait(client.reserve_baseline(stream.clone(), review, 1000))
        .await
        .unwrap();
    let durable::BaselineClaim::Ready { completion, .. } =
        wait(client.claim_baseline(ticket, 1000)).await.unwrap()
    else {
        panic!("fresh isolated baseline expected");
    };
    remote.last_updated_ts = "11.000001".into();
    let raw = serde_json::json!({"ok":true,"draft":{
        "id":remote.id,"client_msg_id":remote.client_msg_id,"last_updated_ts":remote.last_updated_ts,
        "team_id":"T1","user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,
        "blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,"destinations":remote.snapshot.destinations
    }});
    let durable::BaselineResult::Retained(evidence) = wait(client.retain_baseline(
        completion,
        durable::BaselineResponse::Positive {
            raw,
            user: "U1".into(),
        },
        0,
    ))
    .await
    .unwrap() else {
        panic!("positive isolated baseline expected");
    };
    review = wait(client.finish_baseline(
        evidence,
        super_platinum_core::drafts::store::sync::RemoteChoice::Remote,
    ))
    .await
    .unwrap();
    assert_eq!(review.row().lineage().basis(), durable::Basis::Remote);
    assert_eq!(review.row().lineage().verified_epoch(), stream.epoch());
    assert_eq!(review.row().state().acknowledged(), Some(&remote));
    if conflict {
        let writer = wait(client.register_writer(review.clone())).await.unwrap();
        let serial = writer.initial_serial() + 1;
        wait(client.checkpoint(writer, serial, review, local("local branch"), None))
            .await
            .unwrap();
        review = owner.review().await.unwrap();
        remote.last_updated_ts = "12.000001".into();
        remote.snapshot.blocks = vec![
            serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"remote branch"}]}]}),
        ];
        let event = DraftObservation {
            scope: remote.scope.clone(),
            draft_id: remote.id.clone(),
            client_msg_id: remote.client_msg_id.clone(),
            event_ts: "12.000002".into(),
            kind: ObservationKind::Active(remote),
        };
        let ticket = wait(client.reserve_observation(stream, review, event))
            .await
            .unwrap();
        wait(client.finish_observation(ticket)).await.unwrap();
        review = owner.review().await.unwrap();
        assert!(matches!(
            review.row().state().workflow(),
            Workflow::Conflict { .. }
        ));
    }
    assert_eq!(review.row().lineage().basis(), durable::Basis::Remote);
    (store, account, owner, review)
}
#[tokio::test]
async fn remote_and_conflicted_open_install_rebind_keep_exact_raw_review_without_verification() {
    for conflict in [false, true] {
        let (_store, account, owner, expected) = remote_fixture(conflict).await;
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some(account.id().into());
        shell.core.account_epoch = 9;
        let mut host = Host::default();
        let selected = host.select(&shell, account.clone()).unwrap();
        let read = host.begin_load(&shell).unwrap();
        read.ready().await;
        host.finish_load(&shell, &read).unwrap();
        let opened = host
            .begin_managed(&shell, &selected, expected.clone())
            .unwrap();
        opened.ready().await;
        let editor = host.managed(&opened).unwrap();
        let installed = host
            .install_managed_without_native_for_test(&shell, &selected, &opened)
            .await
            .unwrap();
        assert!(owner.review().await.unwrap().same(&expected));
        let mut replacement = ShellState::fixture(crate::media::MediaRegistry::default());
        replacement.core.active_account = Some(account.id().into());
        replacement.core.account_epoch = 9;
        let selected = host.select(&replacement, account.clone()).unwrap();
        let read = host.begin_load(&replacement).unwrap();
        read.ready().await;
        host.finish_load(&replacement, &read).unwrap();
        let rebound = host
            .rebind_without_native_for_managed_test(&replacement, &selected, &installed)
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&editor, rebound.editor()));
        assert_eq!(account.retained_editors().unwrap().len(), 1);
        assert!(owner.review().await.unwrap().same(&expected));
        assert_eq!(
            rebound.expected().row().lineage().basis(),
            durable::Basis::Remote
        );
    }
}
