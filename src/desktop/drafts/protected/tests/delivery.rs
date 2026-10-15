//! Actual worker/Editor paths; only the server outcome is synthetic. Native
//! field clearing and HTTP execution still require a later disconnected gate.
use super::super::{actions::Action, editor::Editor};
use super::*;
use super_platinum_core::drafts::sync::{ConsumeIntent, ConsumeKind};

async fn verified(fixture: &Fixture) -> durable::Stream {
    let client = fixture.account.client().unwrap();
    let current = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), current))
        .await
        .unwrap();
    wait(client.verify_local(stream.clone(), fixture.owner.review().await.unwrap()))
        .await
        .unwrap();
    stream
}
async fn consume(editor: &Arc<Editor>, stream: &durable::Stream) -> durable::Review {
    let before = editor.owner.review().await.unwrap();
    let lease = editor
        .prepare_review(Visit::new(), before.local())
        .await
        .unwrap();
    editor
        .clone()
        .apply_inner(lease, Action::Consume(ConsumeKind::Send), || {
            Ok(Some(stream.clone()))
        })
        .await
        .unwrap()
        .after
}
async fn sent(fixture: &Fixture, stream: &durable::Stream) -> (durable::Review, ConsumeIntent) {
    let client = fixture.account.client().unwrap();
    let before = fixture.owner.review().await.unwrap();
    let intent = before.row().state().consumption().unwrap().clone();
    let ticket = wait(client.prepare(stream.clone(), before)).await.unwrap();
    let claimed = wait(client.claim_dispatch(ticket)).await.unwrap();
    let (operation, receipt) = claimed.into_parts();
    let review = wait(client.record(
        receipt,
        durable::Outcome::Sent {
            client_msg_id: operation.client_msg_id,
            draft_id: operation.base.map(|base| base.id),
            ts: "100.000001".into(),
        },
        0,
    ))
    .await
    .unwrap();
    (review, intent)
}

#[tokio::test]
async fn accepted_send_release_preserves_newer_input_through_local_handoff() {
    let fixture = Fixture::new().await;
    let stream = verified(&fixture).await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    consume(&editor, &stream).await;
    editor.submit(&local("Newer while sending")).unwrap();
    editor.flush().await.unwrap();
    let (accepted, intent) = sent(&fixture, &stream).await;
    editor.refresh_receipt().await.unwrap();
    editor.release_completion(accepted, intent).await.unwrap();
    editor.submit(&local("Typing after local handoff")).unwrap();
    let snapshot = editor.flush().await.unwrap();
    assert!(snapshot.view.inventory().row().is_none());
    let stored = fixture.owner.local_inventory().await.unwrap();
    assert!(stored.row().is_none());
    assert_eq!(stored.local().composer.text, "Typing after local handoff");
    assert!(snapshot.view.inventory().same(&Inventory::Local(stored)));
}

#[tokio::test]
async fn old_send_completion_cannot_release_a_newer_consumption() {
    let fixture = Fixture::new().await;
    let stream = verified(&fixture).await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    consume(&editor, &stream).await;
    let (accepted, intent) = sent(&fixture, &stream).await;
    editor.refresh_receipt().await.unwrap();
    editor.submit(&local("Second explicit action")).unwrap();
    editor.flush().await.unwrap();
    let successor = fixture.owner.review().await.unwrap();
    assert_eq!(successor.row().lineage().verified_epoch(), 0);
    assert_eq!(
        successor.row().lineage().basis(),
        durable::Basis::Unverified
    );
    assert_ne!(
        successor.row().state().client_msg_id(),
        intent.client_msg_id
    );
    assert!(successor.row().state().acknowledged().is_none());
    let client = fixture.account.client().unwrap();
    // Sent rotates to a new local-issued identity. The old receipt cannot grant
    // its successor permission to consume, even under the same live stream.
    assert!(matches!(
        wait(client.consume(stream.clone(), successor.clone(), ConsumeKind::Send)).await,
        Err(durable::Error::Gap)
    ));
    assert!(fixture.owner.review().await.unwrap().same(&successor));
    let reverified = wait(client.verify_local(stream.clone(), successor))
        .await
        .unwrap();
    assert_eq!(reverified.row().lineage().basis(), durable::Basis::Local);
    assert_eq!(reverified.row().lineage().verified_epoch(), stream.epoch());
    assert_eq!(reverified.local().composer.text, "Second explicit action");
    editor.refresh_receipt().await.unwrap();
    let second = consume(&editor, &stream).await;
    assert_ne!(second.row().state().consumption(), Some(&intent));
    assert!(matches!(
        editor.release_completion(accepted, intent).await,
        Err(durable::Error::Stale)
    ));
    assert!(fixture.owner.review().await.unwrap().same(&second));
}

#[tokio::test]
async fn session_receipt_publication_requires_unchanged_full_predecessor() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let loaded = fixture.account.inventory().await.unwrap();
    let mut session = Session::default();
    session.select(ID.into(), 7);
    let ticket = session.begin_load().unwrap();
    session.accept_load(ticket, &loaded).unwrap();
    let key = &fixture.owner.scope().key;
    let old = session.entry(ID, 7, key).unwrap().clone();
    editor.submit(&local("Newer published locally")).unwrap();
    let latest = editor.flush().await.unwrap().view;
    assert!(session.apply(ID, 7, &old, latest.clone()));
    assert!(!session.apply(ID, 7, &old, old.clone()));
    assert!(session.entry(ID, 7, key).unwrap().same(&latest));
}
