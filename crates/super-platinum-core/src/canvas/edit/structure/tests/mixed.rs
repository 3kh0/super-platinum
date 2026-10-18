//! Mixed untouched sections exercise the same consumed intent/ACK/journal path.
use super::*;

fn at(snapshot: &CanvasCollabSnapshot, id: &str, position: &str, kind: u64, style: u64) -> Section {
    let mut section = snapshot.section("item").unwrap().clone();
    for field in [
        t(1, id),
        t(8, position),
        t(21, position),
        n(9, kind),
        n(10, style),
    ] {
        section = changed(&section, field);
    }
    section
}
fn mixed() -> CanvasCollabSnapshot {
    let mut snapshot = fixture();
    let heading = at(&snapshot, "heading", "aab", 0, 1);
    let list = at(&snapshot, "list", "aad", 1, 5);
    let child = changed(
        &changed(
            &at(&snapshot, "list-child", "aadb", 0, 0),
            m(13, &msg(&[t(1, "list"), n(4, 5)])),
        ),
        t(21, "aad-aadb"),
    );
    let unknown = changed(
        &at(&snapshot, "unknown", "aadd", 987, 765),
        m(12, &msg(&[t(77, "opaque sibling")])),
    );
    snapshot.sections.extend([heading, list, child, unknown]);
    snapshot
}
fn replace(snapshot: &mut CanvasCollabSnapshot, id: &str, field: F) {
    let section = snapshot.sections.iter_mut().find(|s| s.id() == id).unwrap();
    *section = changed(section, field);
}
fn text() -> TextDraft {
    TextDraft::from_rtml("New <b>paragraph</b>").unwrap()
}

#[test]
fn mixed_insertion_uses_all_positioned_neighbors_not_filtered_paragraphs() {
    let snapshot = mixed();
    for (side, lower, upper) in [(Side::Before, "aab", "aac"), (Side::After, "aac", "aad")] {
        let prepared = StructureIntent::insert(&snapshot, "item", side, &text(), 100)
            .unwrap()
            .prepare(&snapshot, 100)
            .unwrap();
        let data = prepared.document_data();
        let sections = data.messages(1).unwrap();
        assert_eq!(sections.len(), 1, "no unrelated section reserialization");
        let update = &sections[0];
        let p = position::base(update.string(8).unwrap().unwrap()).unwrap();
        assert!(p > lower && p < upper);
        assert!(update.message(13).unwrap().unwrap().fields().is_empty());
        assert_eq!(update.uint(9).unwrap(), Some(0));
        assert_eq!(update.uint(10).unwrap(), Some(0));
        let canonical = canonical_ack(&prepared, &snapshot);
        let fresh = readback(&snapshot, &canonical);
        let accepted = prepared.validate_ack(&ack(&canonical)).unwrap();
        assert!(accepted.confirms_readback(&fresh).unwrap());
        for old in &snapshot.sections {
            assert_eq!(
                fresh.section(old.id()).unwrap().raw().raw(),
                old.raw().raw()
            );
        }
    }
}

#[test]
fn mixed_deletion_writes_only_paragraph_and_requires_exact_untouched_readback() {
    let snapshot = mixed();
    let prepared = StructureIntent::delete(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    assert_eq!(prepared.document_data().messages(1).unwrap().len(), 1);
    let canonical = canonical_ack(&prepared, &snapshot);
    let mut fresh = readback(&snapshot, &canonical);
    let accepted = prepared.validate_ack(&ack(&canonical)).unwrap();
    assert!(accepted.confirms_readback(&fresh).unwrap());
    for id in ["heading", "list", "list-child", "unknown"] {
        let mut changed = fresh.clone();
        replace(&mut changed, id, t(1002, "concurrent extension"));
        assert!(!accepted.confirms_readback(&changed).unwrap());
    }
    fresh.sections.retain(|s| s.id() != "item");
    assert!(
        accepted.confirms_readback(&fresh).unwrap(),
        "sealed tombstone may be absent from complete index"
    );
}

#[test]
fn mixed_preflight_detects_each_sibling_raw_revision_and_position_change() {
    let snapshot = mixed();
    for id in ["heading", "list", "list-child", "unknown"] {
        for field in [n(2, 99999), t(1002, "changed"), t(8, "zzz")] {
            let intent =
                StructureIntent::insert(&snapshot, "item", Side::After, &text(), 100).unwrap();
            let mut fresh = snapshot.clone();
            replace(&mut fresh, id, field);
            assert!(intent.prepare(&fresh, 100).is_err());
        }
    }
}

#[test]
fn mixed_nonparagraphs_and_parented_list_child_gain_no_target_authority() {
    let snapshot = mixed();
    for id in ["title", "heading", "list", "list-child", "unknown"] {
        assert!(StructureIntent::insert(&snapshot, id, Side::After, &text(), 100).is_err());
        assert!(StructureIntent::delete(&snapshot, id, 100).is_err());
    }
    let actions = paragraph_actions(
        &snapshot,
        &["item", "other", "heading", "list", "list-child", "unknown"],
        100,
    );
    assert!(actions[..2].iter().all(|a| {
        a.as_ref()
            .is_some_and(|a| a.insert_before && a.insert_after && a.delete)
    }));
    assert!(actions[2..].iter().all(Option::is_none));
}

#[test]
fn mixed_parent_topology_remains_bounded_and_never_flattened() {
    let snapshot = mixed();
    for (id, field) in [
        ("list-child", m(13, &msg(&[t(1, "missing"), n(4, 5)]))),
        (
            "list-child",
            m(13, &msg(&[t(1, "list"), t(1, "heading"), n(4, 5)])),
        ),
        ("list-child", m(13, &msg(&[t(1, "list"), n(4, 6)]))),
        (
            "list-child",
            m(13, &msg(&[t(1, "list"), n(4, 5), t(2, "layout")])),
        ),
        ("list-child", t(21, "aadb")),
        ("list", m(13, &msg(&[t(1, "unknown"), n(4, 765)]))),
        ("list", n(10, 765)),
        ("unknown", n(29, 1)),
        ("heading", t(21, "other-path")),
        ("unknown", t(8, "aac")),
        ("unknown", t(8, "aadd:temp")),
    ] {
        let mut fresh = snapshot.clone();
        replace(&mut fresh, id, field);
        assert!(
            Inventory::capture(&fresh).is_err(),
            "invalid placement for {id}"
        );
    }
}

#[test]
fn mixed_known_top_level_heading_witnesses_reachable_sibling_but_opaque_does_not() {
    for style in [1, 2, 3] {
        let mut snapshot = mixed();
        snapshot
            .sections
            .retain(|s| ["item", "title", "heading"].contains(&s.id()));
        replace(&mut snapshot, "heading", n(10, style));
        assert!(StructureIntent::delete(&snapshot, "item", 100).is_ok());
    }
    let mut snapshot = mixed();
    snapshot
        .sections
        .retain(|s| ["item", "title", "unknown"].contains(&s.id()));
    assert!(StructureIntent::delete(&snapshot, "item", 100).is_err());
    let mut snapshot = mixed();
    snapshot
        .sections
        .retain(|s| s.id() != "other" && s.id() != "heading");
    assert!(
        StructureIntent::delete(&snapshot, "item", 100).is_err(),
        "list child is not a top-level sibling witness"
    );
}

#[test]
fn mixed_locked_readonly_and_opaque_content_targets_still_refuse() {
    for field in [
        n(35, 1),
        m(15, &msg(&[n(2, 99999)])),
        m(16, &msg(&[t(29, "content-id")])),
        n(11, 1),
    ] {
        let mut snapshot = mixed();
        replace(&mut snapshot, "item", field);
        assert!(StructureIntent::insert(&snapshot, "item", Side::After, &text(), 100).is_err());
        assert!(StructureIntent::delete(&snapshot, "item", 100).is_err());
    }
}

#[test]
fn mixed_raw_checkpoint_restart_keeps_unknown_outcome_and_exact_review_fence() {
    use recovery::journal::{Begin, Journal, LocalDecision};
    let path = std::env::temp_dir().join(format!("canvas-mixed-{}.sqlite", uuid::Uuid::new_v4()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    let account = "00000000-0000-4000-8000-000000000991";
    let snapshot = mixed();
    let prepared = StructureIntent::insert(&snapshot, "item", Side::After, &text(), 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let checkpoint = prepared.checkpoint().unwrap();
    let encoded = serde_json::to_value(&checkpoint).unwrap();
    let raw: Vec<Vec<u8>> = serde_json::from_value(encoded["sections"].clone()).unwrap();
    assert_eq!(raw.len(), snapshot.sections.len());
    for bytes in raw {
        let section = Section::decode(&bytes).unwrap();
        assert_eq!(snapshot.section(section.id()).unwrap().raw().raw(), bytes);
    }
    let original = {
        let mut journal = Journal::open(&path, account).unwrap();
        match journal.begin(checkpoint.clone()).unwrap() {
            Begin::Started(r) => r,
            _ => panic!("new private journal"),
        }
    };
    let mut journal = Journal::open(&path, account).unwrap();
    let restored = journal.get(checkpoint.scope()).unwrap().unwrap();
    assert_eq!(restored.operation_id(), original.operation_id());
    assert!(restored.checkpoint() == &checkpoint);
    assert!(matches!(
        journal.begin(checkpoint.clone()).unwrap(),
        Begin::Blocked(_)
    ));
    let review = journal.local_review(&restored, &snapshot).unwrap();
    let mut stale = snapshot.clone();
    replace(&mut stale, "unknown", t(1002, "changed after review"));
    assert!(
        journal
            .remove_local_lock(
                &review,
                &stale,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                101
            )
            .is_err()
    );
    assert!(journal.get(checkpoint.scope()).unwrap().is_some());
    assert!(
        journal
            .remove_local_lock(
                &review,
                &snapshot,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                101
            )
            .unwrap()
    );
    assert!(journal.has_local_review(&restored).unwrap());
    drop(journal);
    let journal = Journal::open(&path, account).unwrap();
    assert!(journal.get(checkpoint.scope()).unwrap().is_none());
    assert!(journal.has_local_review(&restored).unwrap());
}
