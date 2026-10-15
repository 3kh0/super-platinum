//! Isolated test setup copied from the existing positive-baseline fixture.
use super::*;
fn local_fixture(path: impl AsRef<std::path::Path>, thread: Option<String>) -> Fixture {
    let mut store = DraftStore::open(path).unwrap();
    let scope = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread,
        },
    };
    storage::activate(&store, &scope.account_id).unwrap();
    adapter_schema::install(&store, &scope.account_id).unwrap();
    store
        .save([(scope.key.clone(), local("/example argument"))])
        .unwrap();
    let inventory = views::local(&store.0, &scope).unwrap();
    let review = operations::enroll(&store, &scope, &inventory).unwrap();
    let state = operations::stream_state(&store, &scope.account_id, &scope.key.team).unwrap();
    let stream =
        operations::open_stream(&store, &scope.account_id, &scope.key.team, &state).unwrap();
    let review = operations::verify(&store, &stream, &review).unwrap();
    let writer = writers::register(&store, &review).unwrap();
    let WriteResult::Applied(review) =
        writers::checkpoint(&store, &writer, 1, Some(&review), review.local(), None).unwrap()
    else {
        panic!("canonical writer expected")
    };
    slash_journal::install(&mut store.0, &scope.account_id).unwrap();
    install(&store, &scope.account_id).unwrap();
    let (prior, next) = records(&review);
    slash_journal::compare(&mut store.0, &scope.account_id, None, &prior).unwrap();
    Fixture {
        store,
        scope,
        stream,
        review,
        writer,
        prior,
        next,
    }
}
pub(super) fn remote_at(path: impl AsRef<std::path::Path>, thread: Option<String>) -> Fixture {
    let mut f = local_fixture(path, thread);
    let ticket = operations::prepare(&f.store, &f.stream, &f.review).unwrap();
    let (op, receipt) = operations::claim(&f.store, ticket).unwrap().into_parts();
    let remote = crate::drafts::sync::RemoteDraft {
        scope: f.scope.clone(),
        id: "D1".into(),
        client_msg_id: op.client_msg_id.clone(),
        last_updated_ts: "10.000001".into(),
        snapshot: op.snapshot.clone().unwrap(),
    };
    let review =
        operations::record(&f.store, &receipt, &Outcome::Active(remote.clone()), 0).unwrap();
    // A newly acknowledged identity makes successor invalidate the prior basis.
    // Only a positive baseline can verify that remote identity for mutation.
    assert_eq!(review.row().lineage().basis(), Basis::Unverified);
    assert_eq!(review.row().lineage().verified_epoch(), 0);
    let error = facade::admitted(&f.store.0, &f.stream.0, &review.inner).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("new mutation requires current verified basis")
    );
    f.writer = writers::register(&f.store, &review).unwrap();
    let WriteResult::Applied(unverified) =
        writers::checkpoint(&f.store, &f.writer, 1, Some(&review), review.local(), None).unwrap()
    else {
        panic!("pre-baseline writer");
    };
    f.review = unverified;
    let (prior, next) = records(&f.review);
    f.prior = prior;
    f.next = next;
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
    assert!(matches!(f.claim(), Err(Error::Storage)));
    f.unchanged();

    // Use the existing gap/new-stream positive baseline protocol. No manual
    // basis edits, fabricated command receipt or weakened admission guard.
    operations::mark_gap(&f.store, &f.stream, GapReason::Disconnect).unwrap();
    let stream_state =
        operations::stream_state(&f.store, &f.scope.account_id, &f.scope.key.team).unwrap();
    f.stream = operations::open_stream(
        &f.store,
        &f.scope.account_id,
        &f.scope.key.team,
        &stream_state,
    )
    .unwrap();
    let review = current(&f);
    let ticket = baseline::reserve(&f.store, &f.stream, &review, 1000).unwrap();
    let BaselineClaim::Ready { completion, .. } = baseline::claim(&f.store, ticket, 1000).unwrap()
    else {
        panic!("fresh fixture baseline");
    };
    let raw = json!({"ok":true,"draft":{"id":remote.id,"client_msg_id":remote.client_msg_id,
        "last_updated_ts":"11.000001","team_id":"T1","user_id":"U1",
        "is_deleted":false,"is_sent":false,"date_scheduled":0,
        "blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,
        "destinations":remote.snapshot.destinations}});
    let BaselineResult::Retained(evidence) = baseline::retain(
        &f.store,
        &completion,
        BaselineResponse::Positive {
            raw,
            user: "U1".into(),
        },
        0,
    )
    .unwrap() else {
        panic!("positive remote baseline");
    };
    let remote = evidence.remote().clone();
    let review = baseline::finish(
        &f.store,
        &evidence,
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
    assert_eq!(review.row().lineage().basis(), Basis::Remote);
    assert_eq!(review.row().state().acknowledged(), Some(&remote));
    f.writer = writers::register(&f.store, &review).unwrap();
    let WriteResult::Applied(review) =
        writers::checkpoint(&f.store, &f.writer, 1, Some(&review), review.local(), None).unwrap()
    else {
        panic!("new receipt")
    };
    f.review = review;
    let (prior, next) = records(&f.review);
    f.prior = prior;
    f.next = next;
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
    f
}
