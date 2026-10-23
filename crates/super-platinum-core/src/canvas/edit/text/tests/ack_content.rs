use super::*;

const BODY: &str = "Changed <b>paragraph</b>";

fn source(language: Option<&str>) -> CanvasCollabSnapshot {
    let mut snapshot = fixture();
    let original = &snapshot.sections[0];
    let content = original.raw().message(12).unwrap().unwrap();
    let text = content.message(1).unwrap().unwrap();
    let fields = language
        .map(|value| t(2, value))
        .into_iter()
        .collect::<Vec<_>>();
    let text = text.replace_fields(2, &fields).unwrap();
    snapshot.sections[0] = Section::from_raw(
        original
            .raw()
            .replace_fields(12, &[m(12, &msg(&[m(1, &text)]))])
            .unwrap(),
    )
    .unwrap();
    snapshot
}

fn prepare(snapshot: &CanvasCollabSnapshot) -> PreparedTextEdit {
    TextIntent::capture(snapshot, "item", 100)
        .unwrap()
        .prepare(
            snapshot,
            &mut EditSession::new(),
            &TextDraft::from_rtml(BODY).unwrap(),
            100,
        )
        .unwrap()
}

// Independently authored CONTENT12; never copy it from the prepared request.
// The actual outgoing sealed CRDT is retained to exercise real ACK validation.
fn ack(prepared: &PreparedTextEdit, content: RawMessage) -> EditAck {
    let wire = prepared
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, 11000)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    let section = prepared
        .canonical
        .raw()
        .replace_fields(2, &[n(2, 11000)])
        .unwrap()
        .replace_fields(12, &[m(12, &content)])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, &section)]))]).raw()).unwrap()
}

fn content_text(fields: &[F]) -> RawMessage {
    msg(&[m(1, &msg(fields))])
}

#[test]
fn independently_ordered_text_and_language_ack_confirms_and_reads_back() {
    for language in [None, Some(""), Some("en")] {
        let snapshot = source(language);
        let original = snapshot.sections.clone();
        let prepared = prepare(&snapshot);
        let outgoing = prepared
            .document_data()
            .message(1)
            .unwrap()
            .unwrap()
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap();
        if language.is_some() {
            assert_eq!(
                outgoing
                    .fields()
                    .iter()
                    .map(|f| f.tag())
                    .collect::<Vec<_>>(),
                [2, 1]
            );
        }
        // An ordinary protobuf serializer emits text1 before language2.
        let mut fields = vec![t(1, BODY)];
        if let Some(language) = language {
            fields.push(t(2, language));
        }
        let incoming = content_text(&fields);
        if language.is_some() {
            assert_ne!(incoming.message(1).unwrap().unwrap(), outgoing);
        }
        let accepted = prepared
            .validate_ack(&ack(&prepared, incoming.clone()))
            .unwrap();
        assert_eq!(
            accepted.section().raw().message(12).unwrap().unwrap(),
            incoming
        );
        let readback = canonical(&snapshot, &accepted);
        assert!(accepted.confirms_readback(&readback).unwrap());
        assert_eq!(
            readback.sections[0].raw().message(12).unwrap().unwrap(),
            incoming
        );
        assert_eq!(snapshot.sections, original);
    }
}

#[test]
fn ack_requires_exact_language_presence_and_value() {
    for (original, received) in [
        (Some("en"), None),
        (Some("en"), Some("")),
        (Some("en"), Some("fr")),
        (None, Some("")),
        (Some(""), None),
    ] {
        let prepared = prepare(&source(original));
        let mut fields = vec![t(1, BODY)];
        if let Some(language) = received {
            fields.push(t(2, language));
        }
        assert!(
            prepared
                .validate_ack(&ack(&prepared, content_text(&fields)))
                .is_err()
        );
    }
}

#[test]
fn reordered_ack_still_refuses_ambiguous_fields_unknown_siblings_and_changed_wire_body() {
    let prepared = prepare(&source(Some("en")));
    for fields in [
        vec![t(1, BODY), t(1, BODY), t(2, "en")],
        vec![t(1, BODY), t(2, "en"), t(2, "en")],
        vec![n(1, 1), t(2, "en")],
        vec![t(1, BODY), n(2, 1)],
        vec![F::bytes(1, &[0xff]).unwrap(), t(2, "en")],
        vec![t(1, BODY), F::bytes(2, &[0xff]).unwrap()],
        vec![t(1, BODY), t(2, "en"), t(99, "unknown")],
        vec![t(1, "Wrong body"), t(2, "en")],
        vec![t(2, "en")],
        // Same normalized draft, different RTML: must not erase it via content().
        vec![t(1, "Changed <b>para</b><b>graph</b>"), t(2, "en")],
    ] {
        assert!(
            prepared
                .validate_ack(&ack(&prepared, content_text(&fields)))
                .is_err()
        );
    }
    let text = msg(&[t(1, BODY), t(2, "en")]);
    for content in [
        msg(&[m(1, &text), m(1, &text)]),
        msg(&[m(1, &text), t(99, "unknown")]),
        msg(&[n(1, 1)]),
    ] {
        assert!(prepared.validate_ack(&ack(&prepared, content)).is_err());
    }
}
