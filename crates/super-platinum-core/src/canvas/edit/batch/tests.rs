use super::*;
fn fixture() -> CanvasCollabSnapshot {
    super::super::tests::plain_without_crdt()
}
fn renamed(section: &Section, id: &str) -> Section {
    Section::from_raw(
        section
            .raw()
            .replace_fields(1, &[F::bytes(1, id.as_bytes()).unwrap()])
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn batch_keeps_target_order_and_independent_failures_while_sharing_title_and_parent() {
    let mut snapshot = fixture();
    snapshot
        .sections
        .push(renamed(&snapshot.sections[0], "other"));
    let mut results = CheckboxIntent::capture_batch(
        &snapshot,
        &[
            ("missing", true),
            ("item", true),
            ("item", false),
            ("other", true),
        ],
        100,
    )
    .into_iter();
    assert!(matches!(results.next().unwrap(), Err(Error::Changed)));
    let first = results.next().unwrap().unwrap();
    assert!(matches!(results.next().unwrap(), Err(Error::Changed)));
    let second = results.next().unwrap().unwrap();
    assert!(results.next().is_none());
    assert_eq!(first.section.id(), "item");
    assert_eq!(second.section.id(), "other");
    assert!(Arc::ptr_eq(&first.title, &second.title));
    assert!(Arc::ptr_eq(&first.parent, &second.parent));
    assert_eq!(first.title.form_title(), "Synthetic canvas title");
}
#[test]
fn large_batch_has_one_shared_title_and_parent_and_retains_fresh_preflight() {
    let mut snapshot = fixture();
    let ids: Vec<_> = (0..256).map(|i| format!("item-{i}")).collect();
    let original = snapshot.sections[0].clone();
    snapshot.sections.retain(|section| section.id() != "item");
    for id in &ids {
        snapshot.sections.push(renamed(&original, id));
    }
    let targets: Vec<_> = ids.iter().map(|id| (id.as_str(), true)).collect();
    let intents: Vec<_> = CheckboxIntent::capture_batch(&snapshot, &targets, 100)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(intents.len(), 256);
    assert_eq!(Arc::strong_count(&intents[0].title), 256);
    assert_eq!(Arc::strong_count(&intents[0].parent), 256);
    let title = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "title")
        .unwrap();
    *title = Section::from_raw(
        title
            .raw()
            .replace_fields(2, &[F::uint(2, 11_000).unwrap()])
            .unwrap(),
    )
    .unwrap();
    let mut session = EditSession::new();
    assert!(matches!(
        intents[0].prepare(&snapshot, &mut session, 100),
        Err(Error::Changed)
    ));
    assert_eq!(session.sequence, 0);
    assert!(session.local.is_empty());
}
#[test]
fn batch_retains_common_permission_and_title_guards_without_creating_work_for_empty_targets() {
    let mut snapshot = fixture();
    snapshot.thread = snapshot.thread.remove(23).unwrap();
    let results =
        CheckboxIntent::capture_batch(&snapshot, &[("item", true), ("missing", true)], 100);
    assert!(results.iter().all(|r| matches!(r, Err(Error::ReadOnly))));
    snapshot = fixture();
    snapshot.sections.retain(|s| s.id() != "title");
    assert!(
        CheckboxIntent::capture_batch(&snapshot, &[("item", true)], 100)
            .iter()
            .all(|r| matches!(r, Err(Error::Unsupported)))
    );
    assert!(CheckboxIntent::capture_batch(&snapshot, &[], 100).is_empty());
}
