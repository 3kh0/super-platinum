//! Synthetic canonical/ACK vectors; no observed server heading ACK is claimed.
use super::*;

const ID: &str = "temp:C:xyz0123456789abcdef012345678";

fn encoded_tag(field: &F) -> u32 {
    RawMessage::decode(field.raw()).unwrap().fields()[0].tag()
}

fn heading(level: u64, base: Option<u64>) -> CanvasCollabSnapshot {
    let mut snapshot = fixture();
    let raw = snapshot.sections[0]
        .raw()
        .replace_fields(1, &[t(1, ID)])
        .unwrap()
        .replace_fields(10, &[n(10, level)])
        .unwrap();
    // Unknown stable siblings retain their original noncanonical encoding.
    let mut bytes = raw.raw().to_vec();
    bytes.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]);
    snapshot.sections[0] = Section::from_raw(RawMessage::decode(&bytes).unwrap()).unwrap();
    if let Some(base) = base {
        let section = &snapshot.sections[0];
        let wire = crdt::bootstrap(
            section,
            &canonical_draft(section).unwrap(),
            "0123456789ab",
            0,
        )
        .unwrap()
        .replace_fields(2, &[n(2, base)])
        .unwrap()
        .replace_fields(3, &[n(3, section.sequence().unwrap().unwrap())])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
        snapshot.sections[0] =
            Section::from_raw(section.raw().replace_fields(38, &[m(38, &wire)]).unwrap()).unwrap();
    }
    snapshot
}

fn prepare(snapshot: &CanvasCollabSnapshot, session: &mut EditSession) -> PreparedTextEdit {
    TextIntent::capture(snapshot, ID, 100)
        .unwrap()
        .prepare(snapshot, session, &changed(), 100)
        .unwrap()
}

fn ack_raw(prepared: &PreparedTextEdit) -> RawMessage {
    let update = prepared.document_data().message(1).unwrap().unwrap();
    let sequence = prepared.base_sequence() + 1000;
    let wire = update
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    prepared
        .canonical
        .merge_partial(&Section::from_raw(update).unwrap())
        .unwrap()
        .raw()
        .replace_fields(2, &[n(2, sequence)])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap()
}

fn ack(raw: &RawMessage) -> EditAck {
    EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, raw)]))]).raw()).unwrap()
}

fn readback(snapshot: &CanvasCollabSnapshot, accepted: &AcceptedText) -> CanvasCollabSnapshot {
    let mut fresh = snapshot.clone();
    // A full canonical read retains CRDT history. `association` is a comparison
    // key which deliberately removes tag38, not a canonical-load projection.
    let mut raw = accepted.baseline.raw().clone();
    for tag in [18, 103, 104] {
        raw = raw.remove(tag).unwrap();
    }
    fresh.sections[0] = Section::from_raw(raw).unwrap();
    fresh
}

#[test]
fn h1_h2_h3_use_content_only_projection_and_preserve_canonical_level() {
    for level in 1..=3 {
        for base in [None, Some(0), Some(9000)] {
            let snapshot = heading(level, base);
            let before = snapshot.sections.clone();
            let intent = TextIntent::capture(&snapshot, ID, 100).unwrap();
            assert_eq!(intent.heading_level(), Some(level as u8));
            let mut session = EditSession::new();
            let prepared = prepare(&snapshot, &mut session);
            assert_eq!(prepared.heading_level(), Some(level as u8));
            let update = prepared.document_data().message(1).unwrap().unwrap();
            assert_eq!(
                update
                    .fields()
                    .iter()
                    .map(|field| field.tag())
                    .collect::<Vec<_>>(),
                [1, 2, 9, 12, 13, 19, 20, 33, 38]
            );
            assert_eq!(update.uint(9).unwrap(), Some(0));
            assert!(update.message(13).unwrap().unwrap().fields().is_empty());
            for tag in [3, 8, 10, 11, 16, 29, 1001] {
                assert!(!update.has(tag));
            }
            let accepted = prepared.validate_ack(&ack(&ack_raw(&prepared))).unwrap();
            assert_eq!(accepted.heading_level(), Some(level as u8));
            assert!(
                accepted
                    .confirms_readback(&readback(&snapshot, &accepted))
                    .unwrap()
            );
            assert_eq!(snapshot.sections, before);
            assert!(canonical_draft(&accepted.baseline).unwrap() == changed());
        }
    }
    assert_eq!(
        TextIntent::capture(&fixture(), "item", 100)
            .unwrap()
            .heading_level(),
        None
    );
}

#[test]
fn heading_admission_keeps_existing_shape_guards() {
    for level in [4, 5, 6, 7, 255] {
        assert!(TextIntent::capture(&heading(level, None), ID, 100).is_err());
    }
    for field in [
        n(9, 1),
        n(3, 1),
        n(29, 1),
        n(35, 1),
        m(15, &msg(&[])),
        m(13, &msg(&[t(1, "parent")])),
    ] {
        let mut snapshot = heading(2, None);
        snapshot.sections[0] = Section::from_raw(
            snapshot.sections[0]
                .raw()
                .replace_fields(encoded_tag(&field), &[field])
                .unwrap(),
        )
        .unwrap();
        assert!(TextIntent::capture(&snapshot, ID, 100).is_err());
    }
    let mut snapshot = heading(1, None);
    snapshot.sections[0] = Section::from_raw(
        snapshot.sections[0]
            .raw()
            .replace_fields(
                12,
                &[m(
                    12,
                    &msg(&[m(1, &msg(&[t(1, "<control>nested</control>")]))]),
                )],
            )
            .unwrap(),
    )
    .unwrap();
    assert!(TextIntent::capture(&snapshot, ID, 100).is_err());
}

#[test]
fn heading_content_id_projection_conditionally_resends_without_restricting_ordinary_attributes() {
    for level in 1..=3 {
        for content_id in [None, Some(""), Some("content-id"), Some(" ")] {
            let mut snapshot = heading(level, None);
            let mut attrs = snapshot.sections[0].attributes().unwrap().unwrap();
            if let Some(value) = content_id {
                attrs = attrs.replace_fields(29, &[t(29, value)]).unwrap();
            }
            snapshot.sections[0] = Section::from_raw(
                snapshot.sections[0]
                    .raw()
                    .replace_fields(16, &[m(16, &attrs)])
                    .unwrap(),
            )
            .unwrap();
            let prepared = prepare(&snapshot, &mut EditSession::new());
            let expected = content_id
                .filter(|id| !id.is_empty())
                .map(|_| attrs.clone());
            assert_eq!(
                prepared
                    .document_data()
                    .message(1)
                    .unwrap()
                    .unwrap()
                    .message(16)
                    .unwrap(),
                expected
            );
            let raw = ack_raw(&prepared);
            let accepted = prepared.validate_ack(&ack(&raw)).unwrap();
            assert_eq!(accepted.baseline.attributes().unwrap(), Some(attrs.clone()));
            assert!(
                accepted
                    .confirms_readback(&readback(&snapshot, &accepted))
                    .unwrap()
            );
            let introduced = attrs
                .replace_fields(29, &[t(29, "new-content-id")])
                .unwrap();
            let bad = raw.replace_fields(16, &[m(16, &introduced)]).unwrap();
            assert!(prepared.validate_ack(&ack(&bad)).is_err());
            let mut later = readback(&snapshot, &accepted);
            later.sections[0] =
                Section::from_raw(bad.replace_fields(2, &[n(2, 12000)]).unwrap()).unwrap();
            assert!(!accepted.confirms_readback(&later).unwrap());
        }
        for field in [n(29, 1)] {
            let mut snapshot = heading(level, None);
            let attrs = snapshot.sections[0]
                .attributes()
                .unwrap()
                .unwrap()
                .replace_fields(29, &[field])
                .unwrap();
            snapshot.sections[0] = Section::from_raw(
                snapshot.sections[0]
                    .raw()
                    .replace_fields(16, &[m(16, &attrs)])
                    .unwrap(),
            )
            .unwrap();
            assert!(TextIntent::capture(&snapshot, ID, 100).is_err());
            // This new boundary is heading-only; preserve existing paragraphs.
            snapshot.sections[0] = Section::from_raw(
                snapshot.sections[0]
                    .raw()
                    .replace_fields(10, &[n(10, 0)])
                    .unwrap(),
            )
            .unwrap();
            assert!(TextIntent::capture(&snapshot, ID, 100).is_ok());
        }
    }
}

#[test]
fn heading_level_or_sibling_drift_before_dispatch_reserves_nothing() {
    let snapshot = heading(2, None);
    let intent = TextIntent::capture(&snapshot, ID, 100).unwrap();
    let mut session = EditSession::new();
    let actor = session.id.clone();
    for field in [n(10, 1), m(16, &msg(&[n(99, 2)])), t(8, "changed-position")] {
        let mut fresh = snapshot.clone();
        fresh.sections[0] = Section::from_raw(
            fresh.sections[0]
                .raw()
                .replace_fields(encoded_tag(&field), &[field])
                .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            intent.prepare(&fresh, &mut session, &changed(), 100),
            Err(Error::Changed)
        ));
        assert_eq!(session.sequence, 0);
        assert_eq!(session.id, actor);
    }
}

#[test]
fn partial_heading_ack_inherits_style_but_explicit_structure_change_refuses() {
    for base in [0, 9000] {
        for level in 1..=3 {
            let snapshot = heading(level, Some(base));
            let prepared = prepare(&snapshot, &mut EditSession::new());
            let raw = ack_raw(&prepared);
            let partial = raw
                .remove(10)
                .unwrap()
                .remove(16)
                .unwrap()
                .remove(8)
                .unwrap();
            let accepted = prepared.validate_ack(&ack(&partial)).unwrap();
            assert_eq!(accepted.section().style().unwrap(), None);
            assert_eq!(accepted.heading_level(), Some(level as u8));
            assert!(
                accepted
                    .confirms_readback(&readback(&snapshot, &accepted))
                    .unwrap()
            );
            for field in [
                n(10, 0),
                n(10, level % 3 + 1),
                n(9, 2),
                t(8, "moved"),
                m(16, &msg(&[n(99, 2)])),
                n(29, 1),
                n(11, 42),
                n(1001, 2),
                m(13, &msg(&[t(1, "parent")])),
            ] {
                let bad = raw.replace_fields(encoded_tag(&field), &[field]).unwrap();
                assert!(prepared.validate_ack(&ack(&bad)).is_err());
            }
            // ContentText.language is not a text-editable field.
            let content = raw.message(12).unwrap().unwrap();
            let text = content
                .message(1)
                .unwrap()
                .unwrap()
                .replace_fields(2, &[t(2, "fr")])
                .unwrap();
            let bad = raw
                .replace_fields(
                    12,
                    &[m(12, &content.replace_fields(1, &[m(1, &text)]).unwrap())],
                )
                .unwrap();
            assert!(prepared.validate_ack(&ack(&bad)).is_err());
        }
    }
}

#[test]
fn heading_readback_never_confirms_changed_level_or_structure_at_later_revision() {
    let snapshot = heading(3, Some(9000));
    let prepared = prepare(&snapshot, &mut EditSession::new());
    let accepted = prepared.validate_ack(&ack(&ack_raw(&prepared))).unwrap();
    for later in [false, true] {
        let mut fresh = readback(&snapshot, &accepted);
        if later {
            let raw = fresh.sections[0]
                .raw()
                .replace_fields(2, &[n(2, 12000)])
                .unwrap();
            let raw = raw
                .replace_fields(
                    12,
                    &[m(
                        12,
                        &content(
                            &fresh.sections[0],
                            &TextDraft::from_rtml("Later text").unwrap(),
                        )
                        .unwrap(),
                    )],
                )
                .unwrap();
            fresh.sections[0] = Section::from_raw(raw).unwrap();
        }
        assert!(accepted.confirms_readback(&fresh).unwrap());
        for field in [
            n(10, 0),
            n(10, 2),
            n(9, 1),
            t(8, "moved"),
            m(16, &msg(&[n(99, 2)])),
            n(29, 1),
            n(1001, 2),
            m(13, &msg(&[t(1, "parent")])),
        ] {
            let mut bad = fresh.clone();
            bad.sections[0] = Section::from_raw(
                bad.sections[0]
                    .raw()
                    .replace_fields(encoded_tag(&field), &[field])
                    .unwrap(),
            )
            .unwrap();
            assert!(!accepted.confirms_readback(&bad).unwrap_or(false));
        }
        let mut bad = fresh.clone();
        let content = bad.sections[0].raw().message(12).unwrap().unwrap();
        let text = content.message(1).unwrap().unwrap().remove(2).unwrap();
        bad.sections[0] = Section::from_raw(
            bad.sections[0]
                .raw()
                .replace_fields(
                    12,
                    &[m(12, &content.replace_fields(1, &[m(1, &text)]).unwrap())],
                )
                .unwrap(),
        )
        .unwrap();
        assert!(!accepted.confirms_readback(&bad).unwrap());
    }
}

#[test]
fn heading_base_zero_and_positive_histories_survive_two_text_saves() {
    for level in 1..=3 {
        for base in [0, 9000] {
            let snapshot = heading(level, Some(base));
            let mut session = EditSession::new();
            let prepared = prepare(&snapshot, &mut session);
            let accepted = prepared.validate_ack(&ack(&ack_raw(&prepared))).unwrap();
            session.remember_text_accepted(&accepted).unwrap();
            let fresh = readback(&snapshot, &accepted);
            assert!(fresh.sections[0].raw().has(38));
            if base == 0 {
                let mut missing_history = fresh.clone();
                missing_history.sections[0] =
                    Section::from_raw(fresh.sections[0].raw().remove(38).unwrap()).unwrap();
                assert!(!accepted.confirms_readback(&missing_history).unwrap());
                let missing = TextIntent::capture(&missing_history, ID, 200).unwrap();
                assert!(matches!(
                    missing.prepare(
                        &missing_history,
                        &mut session,
                        &TextDraft::from_rtml("Another edit").unwrap(),
                        200,
                    ),
                    Err(Error::Unsupported)
                ));
                assert_eq!(session.sequence, 1);
            }
            let intent = TextIntent::capture(&fresh, ID, 200).unwrap();
            let desired =
                TextDraft::from_rtml("<b>Heading \u{4e2d}\u{1f642}</b> <i>next</i>").unwrap();
            let second = intent.prepare(&fresh, &mut session, &desired, 200).unwrap();
            assert_eq!(second.heading_level(), Some(level as u8));
            assert_eq!(second.sequence(), 2);
            let second_ack = second.validate_ack(&ack(&ack_raw(&second))).unwrap();
            assert!(
                second_ack
                    .confirms_readback(&readback(&fresh, &second_ack))
                    .unwrap()
            );
            assert!(canonical_draft(&second_ack.baseline).unwrap() == desired);
            assert_eq!(
                second_ack
                    .section()
                    .crdt()
                    .unwrap()
                    .unwrap()
                    .uint(2)
                    .unwrap(),
                Some(base)
            );
        }
    }
}

mod content_id;
