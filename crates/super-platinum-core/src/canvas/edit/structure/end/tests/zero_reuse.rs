//! Acknowledged modern zero-base children are existing canonical sections.
use super::*;
mod recovery;

pub(super) fn source() -> (CanvasCollabSnapshot, String) {
    let mut s = fixture("Café &amp; 日本😀", true);
    // Make the parent final before creating a child through the already
    // accepted new-end path. No legacy ID is disguised by changing base2.
    for row in &mut s.sections {
        let pos = match row.id() {
            "item" => "aae",
            "other" => "aac",
            _ => continue,
        };
        *row = change(&change(row, t(8, pos)), t(21, pos));
    }
    let new = prepare(&s);
    assert!(!new.reuses_child());
    let id = new.child_section().to_owned();
    assert!(position::modern_id(&id));
    let mut rows = pair(&new, &s);
    let wire = rows[1].crdt().unwrap().unwrap();
    let head = wire.string(8).unwrap().unwrap();
    let wire = wire
        .replace_fields(11, &[m(11, &msg(&[t(1, head), n(2, 1000)]))])
        .unwrap();
    rows[1] = change(&rows[1], m(38, &wire));
    let accepted = new.validate_ack(&ack(&rows)).unwrap();
    let canonical = readback(&s, &rows);
    assert!(accepted.confirms_readback(&canonical).unwrap());
    assert_eq!(wire.uint(2).unwrap(), Some(0));
    assert_eq!(wire.uint(3).unwrap(), Some(1000));
    (canonical, id)
}

fn attempt(s: &CanvasCollabSnapshot, updates: &[&str]) -> PreparedEndEdit {
    let intent = EndIntent::capture(s, "item", 100).unwrap();
    assert!(intent.reuses_child());
    let witness = witness(&intent.seed().rtml().unwrap())
        .with_child_updates(&updates.iter().map(|s| draft(s)).collect::<Vec<_>>())
        .unwrap();
    intent.prepare(s, &witness, 100).unwrap()
}

fn processed(wire: &RawMessage) -> Vec<String> {
    wire.fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            RawMessage::decode(wire.encoded_field(f).unwrap())
                .unwrap()
                .string(6)
                .unwrap()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn replace(s: &CanvasCollabSnapshot, id: &str, field: F) -> CanvasCollabSnapshot {
    let mut changed = s.clone();
    let row = changed.sections.iter_mut().find(|r| r.id() == id).unwrap();
    *row = change(row, field);
    changed
}

#[test]
fn canonical_zero_reuse_preserves_identity_history_and_both_save_modes() {
    for updates in [
        &[][..],
        &["New", "New draftx", "New draft", "New paragraph"][..],
        &[
            "New",
            "New 日本😀 draftx",
            "New 日本😀 draft",
            "New 日本😀 paragraph",
        ][..],
        &["x", ""][..],
    ] {
        let (s, id) = source();
        let original = s.section(&id).unwrap();
        let seed = original.crdt().unwrap().unwrap();
        let p = attempt(&s, updates);
        assert_eq!(p.child_section(), id);
        let rows = p.data.messages(1).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].uint(2).unwrap(), original.sequence().unwrap());
        assert!(!rows[1].has(8) && !rows[1].has(21));
        assert_eq!(p.child_wire.uint(2).unwrap(), Some(0));
        assert_eq!(p.child_wire.uint(3).unwrap(), original.sequence().unwrap());
        assert_eq!(p.child_wire.string(8).unwrap(), seed.string(8).unwrap());
        assert!(p.child_wire.messages(11).unwrap() == seed.messages(11).unwrap());
        assert!(!p.child_wire.has(4) && !p.child_wire.has(10));
        let before = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
        let after = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
        assert!(
            before
                .get_changes(&[])
                .iter()
                .all(|c| after.get_change_by_hash(&c.hash()).is_some())
        );
        if updates.is_empty() {
            assert!(!rows[1].has(9) && !rows[1].has(12) && !p.child_wire.has(12));
            assert!(p.child_wire == validation::project_parent_session(&seed).unwrap());
        } else {
            assert!(rows[1].has(9) && rows[1].has(12) && p.child_wire.has(12));
            let mut expected = processed(&seed);
            expected.push(after.get_heads()[0].to_string());
            expected.extend(processed(&seed));
            assert_eq!(processed(&p.child_wire), expected);
            assert_eq!(
                after.get_changes(&before.get_heads()).len(),
                if updates.len() == 2 { 2 } else { 7 }
            );
        }
        let canonical = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        let fresh = readback(&s, &canonical);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        assert_eq!(fresh.sections.len(), s.sections.len());
        assert_eq!(
            fresh.section(&id).unwrap().raw().string(8).unwrap(),
            original.raw().string(8).unwrap()
        );
        let continued = save_text(&fresh, &id, "Continued 日本😀 &amp; text");
        assert_eq!(
            continued
                .section(&id)
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .uint(2)
                .unwrap(),
            Some(0)
        );
        assert_eq!(continued.section("item"), fresh.section("item"));
    }
}

#[test]
fn zero_reuse_source_guards_and_lease_remain_strict() {
    let (s, id) = source();
    let wire = s.section(&id).unwrap().crdt().unwrap().unwrap();
    for bad in [
        wire.replace_fields(3, &[n(3, 0)]).unwrap(),
        wire.remove(4).unwrap(),
        wire.replace_fields(4, &[n(4, 100)]).unwrap(),
        wire.remove(6).unwrap(),
        wire.replace_fields(6, &[t(6, &"0".repeat(64))]).unwrap(),
        wire.replace_fields(10, &[m(10, &msg(&[n(1, 0)]))]).unwrap(),
        wire.replace_fields(9, &[m(9, &msg(&[]))]).unwrap(),
    ] {
        assert!(EndIntent::capture(&replace(&s, &id, m(38, &bad)), "item", 100).is_err());
    }
    for field in [
        n(2, 0),
        n(35, 1),
        n(10, 1),
        m(12, &body(" ", false)),
        m(16, &msg(&[n(13, 1)])),
        t(1, "legacy-child"),
    ] {
        assert!(EndIntent::capture(&replace(&s, &id, field), "item", 100).is_err());
    }
    // An independently acknowledged zero child is still not an admitted parent.
    let text_child = save_text(&s, &id, "Nonempty child");
    assert!(EndIntent::capture(&text_child, &id, 100).is_err());
    let legacy = replace(
        &replace(&s, &id, t(1, "legacy-child")),
        "legacy-child",
        m(
            38,
            &wire.replace_fields(1, &[t(1, "legacy-child")]).unwrap(),
        ),
    );
    assert!(EndIntent::capture(&legacy, "item", 100).is_err());
    let p = attempt(&s, &["x", ""]);
    assert!(p.validate_snapshot(&s, 1_000_000).is_err());
    assert!(
        p.validate_snapshot(
            &replace(
                &s,
                &id,
                m(38, &wire.replace_fields(4, &[n(4, 999_999)]).unwrap())
            ),
            100
        )
        .is_err()
    );
    assert!(
        p.validate_snapshot(&replace(&s, &id, n(2, 2000)), 100)
            .is_err()
    );
    assert!(
        p.validate_snapshot(
            &replace(&s, "title", m(12, &body("Changed title", false))),
            100
        )
        .is_err()
    );
}

#[test]
fn zero_reuse_ack_and_readback_require_complete_source_bound_pair() {
    for updates in [&[][..], &["New", "New paragraph"][..], &["x", ""][..]] {
        let (s, id) = source();
        let p = attempt(&s, updates);
        let rows = pair(&p, &s);
        assert!(p.validate_ack(&ack(&rows[..1])).is_err());
        assert!(p.validate_ack(&ack(&rows[1..])).is_err());
        let sent = rows[1].crdt().unwrap().unwrap();
        for bad_wire in [
            sent.replace_fields(2, &[n(2, 1000)]).unwrap(),
            sent.replace_fields(3, &[n(3, 0)]).unwrap(),
            sent.remove(6).unwrap(),
            sent.replace_fields(8, &[t(8, &"0".repeat(64))]).unwrap(),
        ] {
            let bad = [rows[0].clone(), change(&rows[1], m(38, &bad_wire))];
            assert!(p.validate_ack(&ack(&bad)).is_err());
        }
        let bad = [
            rows[0].clone(),
            change(
                &rows[1],
                m(12, &body(&p.child_draft().rtml().unwrap(), true)),
            ),
        ];
        assert!(p.validate_ack(&ack(&bad)).is_err());
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let fresh = readback(&s, &rows);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        let bad = replace(
            &fresh,
            &id,
            m(38, &sent.replace_fields(2, &[n(2, 1000)]).unwrap()),
        );
        assert!(!accepted.confirms_readback(&bad).unwrap());
        let mut missing = fresh.clone();
        missing.sections.retain(|r| r.id() != id);
        assert!(!accepted.confirms_readback(&missing).unwrap());
    }
}
