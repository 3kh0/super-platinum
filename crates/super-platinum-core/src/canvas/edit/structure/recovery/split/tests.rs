use super::*;
use crate::canvas::edit::structure::{ReplacementIntent, SplitIntent, SplitWitness};
use journal::{Begin, Journal};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000783";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-split-recovery-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, ACCOUNT).unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn t(tag: u32, text: &str) -> F {
    F::bytes(tag, text.as_bytes()).unwrap()
}
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, message: &RawMessage) -> F {
    F::message(tag, message).unwrap()
}
fn fixture() -> CanvasCollabSnapshot {
    let mut s = crate::canvas::edit::structure::tests::fixture();
    let item = s.sections.iter_mut().find(|s| s.id() == "item").unwrap();
    let content = text::content(
        item,
        &TextDraft::from_rtml("Keep thisSecond paragraph").unwrap(),
    )
    .unwrap();
    *item = Section::from_raw(item.raw().replace_fields(12, &[m(12, &content)]).unwrap()).unwrap();
    s
}
fn checkpoint(snapshot: &CanvasCollabSnapshot) -> Checkpoint {
    let before = TextDraft::from_rtml("Keep thisSecond paragraph").unwrap();
    let witness = SplitWitness::new(
        &before,
        9,
        &TextDraft::from_rtml("Keep this").unwrap(),
        &TextDraft::from_rtml("Second paragraph").unwrap(),
    )
    .unwrap();
    SplitIntent::capture(snapshot, "item", 100)
        .unwrap()
        .prepare(snapshot, &witness, 100)
        .unwrap()
        .checkpoint()
        .unwrap()
}
fn started(j: &mut Journal, c: Checkpoint) -> journal::Receipt {
    match j.begin(c).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("unexpected old lock"),
    }
}

#[test]
fn v3_roundtrip_preserves_whole_pair_and_raw_sources_without_replay_authority() {
    let snapshot = fixture();
    let c = checkpoint(&snapshot);
    assert!(c.is_split());
    assert!(!c.is_delete() && !c.is_replacement());
    assert_eq!(c.version, 3);
    assert_eq!(c.draft_rtml(), "Keep thisSecond paragraph");
    assert_eq!(
        RawMessage::decode(&c.request)
            .unwrap()
            .messages(1)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(c.sections.len(), snapshot.sections.len());
    for bytes in &c.sections {
        let section = Section::decode(bytes).unwrap();
        assert_eq!(snapshot.section(section.id()).unwrap().raw().raw(), bytes);
    }
    assert!(
        c.sections
            .iter()
            .any(|s| s.windows(4).any(|b| b == [0xc8, 0x3e, 0x81, 0x00]))
    );
    let stored = serde_json::to_string(&c).unwrap();
    let decoded: Checkpoint = serde_json::from_str(&stored).unwrap();
    assert!(decoded == c);
    let store = Store::new();
    let receipt = started(&mut store.open(), decoded);
    let mut j = store.open();
    let restored = j.get(c.scope()).unwrap().unwrap();
    assert!(restored == receipt);
    assert!(matches!(j.begin(c.clone()).unwrap(), Begin::Blocked(_)));
    // The dedicated pair review is read-only until the explicit local decision.
    let review = j.local_review(&restored, &snapshot).unwrap();
    assert!(review.is_split());
    assert_eq!(
        review.split().unwrap().original_text(),
        "Keep thisSecond paragraph"
    );
    assert_eq!(j.local_review_count(c.scope()).unwrap(), 0);
    assert!(j.get(c.scope()).unwrap().is_some());
}

#[test]
fn v1_v2_and_v3_remain_distinct_and_roundtrip_in_the_same_store() {
    let snapshot = fixture();
    let v1 = StructureIntent::insert(
        &snapshot,
        "item",
        Side::After,
        &TextDraft::from_rtml("New").unwrap(),
        100,
    )
    .unwrap()
    .prepare(&snapshot, 100)
    .unwrap()
    .checkpoint()
    .unwrap();
    let mut sole = snapshot.clone();
    sole.sections.retain(|s| s.id() != "other");
    let v2 = ReplacementIntent::main_canvas(&sole, "item", 100)
        .unwrap()
        .prepare(&sole, 100)
        .unwrap()
        .checkpoint()
        .unwrap();
    let v3 = checkpoint(&snapshot);
    let store = Store::new();
    for (expected, c) in [(1, v1), (2, v2), (3, v3)] {
        let original = serde_json::to_string(&c).unwrap();
        assert_eq!(c.version, expected);
        let decoded: Checkpoint = serde_json::from_str(&original).unwrap();
        assert_eq!(serde_json::to_string(&decoded).unwrap(), original);
        let receipt = started(&mut store.open(), decoded);
        let mut j = store.open();
        let restored = j.get(c.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        // These attempts were never dispatched; exact non-dispatch completion
        // is existing journal authority, not a restored server outcome.
        assert!(j.complete(&restored).unwrap());
    }
}

#[test]
fn changed_split_metadata_and_source_inventory_refuse_reservation() {
    let original = checkpoint(&fixture());
    let store = Store::new();
    let mut j = store.open();
    for kind in 0..11 {
        let mut c = original.clone();
        match kind {
            0 => c.version = 2,
            1 => c.sequence = 2,
            2 => c.draft = "Keep thisDifferent paragraph".into(),
            3 => {
                if let Action::Split { offset_utf16, .. } = &mut c.action {
                    *offset_utf16 = 8;
                }
            }
            4 => {
                if let Action::Split { parent_draft, .. } = &mut c.action {
                    *parent_draft = "Other".into();
                }
            }
            5 => {
                if let Action::Split { child_draft, .. } = &mut c.action {
                    *child_draft = "".into();
                }
            }
            6 => {
                if let Action::Split { child, .. } = &mut c.action {
                    *child = c.target.clone();
                }
            }
            7 => {
                if let Action::Split { position, .. } = &mut c.action {
                    *position = "zzz:temp".into();
                }
            }
            8 => c.sections.push(c.sections[0].clone()),
            9 => c
                .sections
                .retain(|bytes| Section::decode(bytes).unwrap().id() != "title"),
            _ => {
                let index = c
                    .sections
                    .iter()
                    .position(|b| Section::decode(b).unwrap().id() == "item")
                    .unwrap();
                let section = Section::decode(&c.sections[index]).unwrap();
                c.sections[index] = section
                    .raw()
                    .replace_fields(2, &[n(2, 12000)])
                    .unwrap()
                    .raw()
                    .to_vec();
            }
        }
        assert!(j.begin(c).is_err(), "corruption {kind}");
        assert!(j.get(original.scope()).unwrap().is_none());
    }
}

fn corrupt_request(c: &Checkpoint, kind: usize) -> Checkpoint {
    let mut changed = c.clone();
    let data = RawMessage::decode(&c.request).unwrap();
    let mut pair = data.messages(1).unwrap();
    match kind {
        0 => {
            pair.pop();
        }
        1 => pair.reverse(),
        2 => pair[1] = pair[0].clone(),
        3 => pair[1] = pair[1].replace_fields(33, &[t(33, "foreign-0")]).unwrap(),
        4 => pair[1] = pair[1].remove(38).unwrap(),
        5 => {
            pair[0] = pair[0]
                .replace_fields(
                    12,
                    &[m(12, &raw(&[m(1, &raw(&[t(1, "Keep this"), t(2, "fr")]))]))],
                )
                .unwrap()
        }
        6 => {
            let wire = pair[0].message(38).unwrap().unwrap();
            let info = wire
                .message(10)
                .unwrap()
                .unwrap()
                .replace_fields(5, &[t(5, "[]")])
                .unwrap();
            pair[0] = pair[0]
                .replace_fields(
                    38,
                    &[m(38, &wire.replace_fields(10, &[m(10, &info)]).unwrap())],
                )
                .unwrap();
        }
        7 => {
            let wire = pair[0].message(38).unwrap().unwrap().remove(10).unwrap();
            pair[0] = pair[0].replace_fields(38, &[m(38, &wire)]).unwrap();
        }
        8 => {
            let wire = pair[0].message(38).unwrap().unwrap();
            let info = wire
                .message(10)
                .unwrap()
                .unwrap()
                .replace_fields(4, &[n(4, 8)])
                .unwrap();
            pair[0] = pair[0]
                .replace_fields(
                    38,
                    &[m(38, &wire.replace_fields(10, &[m(10, &info)]).unwrap())],
                )
                .unwrap();
        }
        9 => {
            pair[0] = pair[0]
                .replace_fields(38, &[m(38, &pair[1].message(38).unwrap().unwrap())])
                .unwrap()
        }
        _ => pair[1] = pair[1].replace_fields(8, &[t(8, "zzz:temp")]).unwrap(),
    }
    changed.request = data
        .replace_fields(1, &pair.iter().map(|s| m(1, s)).collect::<Vec<_>>())
        .unwrap()
        .raw()
        .to_vec();
    changed
}

#[test]
fn partial_pair_trace_body_position_and_lineage_corruption_never_reserve() {
    let c = checkpoint(&fixture());
    let store = Store::new();
    let mut j = store.open();
    for kind in 0..11 {
        assert!(
            j.begin(corrupt_request(&c, kind)).is_err(),
            "request corruption {kind}"
        );
        assert!(j.get(c.scope()).unwrap().is_none());
    }
    let mut changed = c.clone();
    changed.request = RawMessage::decode(&c.request)
        .unwrap()
        .replace_fields(14, &[m(14, &raw(&[t(1, "wrong title")]))])
        .unwrap()
        .raw()
        .to_vec();
    assert!(j.begin(changed).is_err());
}

#[test]
fn corrupted_persisted_split_stays_blocking_and_failed_retirement_keeps_exact_pair() {
    let store = Store::new();
    let c = checkpoint(&fixture());
    let receipt = started(&mut store.open(), c.clone());
    let conn = rusqlite::Connection::open(&store.0).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_remove BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'test disk failure'); END;").unwrap();
    assert!(store.open().complete(&receipt).is_err());
    assert!(store.open().get(c.scope()).unwrap().unwrap() == receipt);
    let corrupt = serde_json::to_string(&corrupt_request(&c, 0)).unwrap();
    conn.execute("UPDATE receipts SET body=?1", [corrupt])
        .unwrap();
    let mut reopened = store.open();
    assert!(reopened.get(c.scope()).is_err());
    assert!(reopened.begin(c).is_err());
}
