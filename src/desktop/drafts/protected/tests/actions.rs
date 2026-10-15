use super::super::{actions::Action, editor::Editor, registry::Registry};
use super::*;

#[tokio::test]
async fn retired_native_visit_refuses_clear_without_changing_inventory() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let before = fixture.owner.review().await.unwrap();
    let visit = Visit::new();
    let lease = editor
        .prepare_review(visit.clone(), before.local())
        .await
        .unwrap();
    visit.retire();
    assert!(matches!(
        editor
            .apply_inner(lease, Action::Control(durable::Control::Clear), || Ok(None))
            .await,
        Err(durable::Error::Stale)
    ));
    assert!(fixture.owner.review().await.unwrap().same(&before));
}

#[tokio::test]
async fn no_op_input_retires_review_lease_even_when_visible_bytes_match() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let before = fixture.owner.review().await.unwrap();
    let lease = editor
        .prepare_review(Visit::new(), before.local())
        .await
        .unwrap();
    editor.submit(before.local()).unwrap();
    editor.flush().await.unwrap();
    assert!(matches!(
        editor
            .apply_inner(lease, Action::Control(durable::Control::Clear), || Ok(None))
            .await,
        Err(durable::Error::Stale)
    ));
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Original"
    );
}

#[tokio::test]
async fn clear_requires_native_barrier_and_subsequent_input_is_an_alternate() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let before = fixture.owner.review().await.unwrap();
    let lease = editor
        .prepare_review(Visit::new(), before.local())
        .await
        .unwrap();
    let applied = editor
        .clone()
        .apply_inner(lease, Action::Control(durable::Control::Clear), || Ok(None))
        .await
        .unwrap();
    let barrier = applied.barrier.unwrap();
    assert!(editor.snapshot().parked);
    assert!(fixture.owner.review().await.unwrap().local().empty());
    let ticket = editor.prepare_projection(barrier.id()).await.unwrap();
    assert_eq!(ticket.before_local().composer.text, "Original");
    assert!(ticket.after_local().empty());
    editor.submit(&local("Typed after clear receipt")).unwrap();
    assert!(matches!(
        editor.flush().await,
        Err(durable::Error::ReviewRequired)
    ));
    assert!(matches!(
        editor.prepare_projection(barrier.id()).await,
        Err(durable::Error::Stale)
    ));
    let current = fixture.owner.review().await.unwrap();
    assert!(current.local().empty());
    assert!(
        current
            .alternates()
            .iter()
            .any(|value| value.draft.composer.text == "Typed after clear receipt")
    );
}

#[tokio::test]
async fn registry_replacement_and_none_to_some_are_distinct_authority() {
    let fixture = Fixture::new().await;
    let first = Editor::open(fixture.owner.clone()).await.unwrap();
    let second = Editor::open(fixture.owner.clone()).await.unwrap();
    let registry = Registry::default();
    registry.select(ID.into(), 5);
    let key = &fixture.owner.scope().key;
    registry.replace(ID, 5, key, None, first.clone()).unwrap();
    assert!(matches!(
        registry.replace(ID, 5, key, None, second.clone()),
        Err(durable::Error::Stale)
    ));
    registry
        .replace(ID, 5, key, Some(&first), second.clone())
        .unwrap();
    assert!(!registry.current(ID, 5, key, &first));
    assert!(registry.current(ID, 5, key, &second));
    assert!(!registry.remove(ID, 5, key, &first));
    registry.select(ID.into(), 6);
    assert!(!registry.current(ID, 6, key, &second));
}

#[tokio::test]
async fn released_editor_routes_repeated_no_op_inputs_through_local_lease() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    editor.release().await.unwrap();
    let before = fixture.owner.local_inventory().await.unwrap();
    assert!(before.row().is_none());
    let first = editor.submit(&local("Original")).unwrap();
    let second = editor.submit(&local("Original")).unwrap();
    assert!(second > first);
    let snapshot = editor.flush().await.unwrap();
    assert!(snapshot.pending.is_none());
    let after = fixture.owner.local_inventory().await.unwrap();
    assert!(after.row().is_none());
    assert!(same_local(before.local(), after.local()));
    assert!(after.clock() > before.clock());
}

#[tokio::test]
async fn historical_managed_retry_cannot_publish_over_a_later_writer() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let expected = fixture.owner.review().await.unwrap();
    let writer = editor.state.lock().unwrap().writer.clone();
    let input = local("Accepted earlier");
    wait(
        fixture
            .owner
            .checkpoint(writer.clone(), 1, expected.clone(), input.clone(), None)
            .unwrap(),
    )
    .await
    .unwrap();
    let current = fixture.owner.review().await.unwrap();
    let other = wait(fixture.owner.register(current.clone()).unwrap())
        .await
        .unwrap();
    wait(
        fixture
            .owner
            .checkpoint(other, 1, current, local("Newer writer"), None)
            .unwrap(),
    )
    .await
    .unwrap();
    {
        let mut state = editor.state.lock().unwrap();
        state.serial = 1;
        state.pending = Some((1, input.clone()));
        state.retry = Some((
            super::super::editor::Command {
                writer,
                serial: 1,
                expected,
                local: input,
                direct: None,
            },
            false,
        ));
        state.error = Some(durable::Error::Storage);
    }
    editor.retry_storage().unwrap();
    assert!(matches!(
        editor.flush().await,
        Err(durable::Error::ReviewRequired)
    ));
    let snapshot = editor.snapshot();
    assert_eq!(snapshot.view.local().composer.text, "Newer writer");
    assert_eq!(snapshot.pending.unwrap().composer.text, "Accepted earlier");
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Newer writer"
    );
}

#[tokio::test]
async fn settlement_keeps_actual_visible_alternate_as_projection_predecessor() {
    use super_platinum_core::drafts::store::sync::LocalChoice;
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let before = fixture.owner.review().await.unwrap();
    let lease = editor
        .prepare_review(Visit::new(), before.local())
        .await
        .unwrap();
    editor
        .clone()
        .apply_inner(lease, Action::Control(durable::Control::Clear), || Ok(None))
        .await
        .unwrap();
    let visible = local("Visible alternate");
    editor.submit(&visible).unwrap();
    assert!(matches!(
        editor.flush().await,
        Err(durable::Error::ReviewRequired)
    ));
    let lease = editor
        .prepare_settlement(Visit::new(), &visible)
        .await
        .unwrap();
    let applied = editor
        .clone()
        .apply_inner(lease, Action::Settle(LocalChoice::Current), || Ok(None))
        .await
        .unwrap();
    let barrier = applied.barrier.unwrap();
    let ticket = editor.prepare_projection(barrier.id()).await.unwrap();
    assert!(same_local(ticket.before_local(), &visible));
    assert!(ticket.after_local().empty());
    assert!(editor.snapshot().parked);
    // A later serial from that immutable writer is a new preserved version,
    // even though this review retired its earlier alternate serial.
    editor.submit(&local("After settlement")).unwrap();
    assert!(matches!(
        editor.flush().await,
        Err(durable::Error::ReviewRequired)
    ));
    let fresh = fixture.owner.review().await.unwrap();
    assert!(fresh.local().empty());
    assert!(
        fresh
            .alternates()
            .iter()
            .any(|alternate| alternate.draft.composer.text == "After settlement")
    );
}
