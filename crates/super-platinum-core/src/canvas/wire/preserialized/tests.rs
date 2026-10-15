use super::*;
use crate::canvas::wire::LoadDataResponse;

fn msg(fields: Vec<EncodedField>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn uint(tag: u32, n: u64) -> EncodedField {
    EncodedField::uint(tag, n).unwrap()
}
fn bytes(tag: u32, value: &[u8]) -> EncodedField {
    EncodedField::bytes(tag, value).unwrap()
}
fn child(tag: u32, raw: &RawMessage) -> EncodedField {
    EncodedField::message(tag, raw).unwrap()
}
fn row(id: &str) -> RawMessage {
    msg(vec![
        bytes(1, id.as_bytes()),
        uint(2, 7),
        bytes(6, b"THREAD"),
        bytes(7, b"DOC"),
    ])
}
fn history(id: &str) -> RawMessage {
    // Synthetic opaque wire bytes: this does not claim a valid Automerge history.
    msg(vec![
        bytes(1, id.as_bytes()),
        uint(2, 0),
        uint(3, 7),
        bytes(5, b"opaque"),
        bytes(6, b"head"),
        bytes(90, b"unknown"),
    ])
}
fn record(kind: u64, rows: &[RawMessage]) -> RawMessage {
    let mut fields = vec![uint(1, kind)];
    fields.extend(rows.iter().map(|row| child(2, row)));
    msg(fields)
}
fn wrapper(
    payload: Option<&RawMessage>,
    records: &[RawMessage],
    sessions: &[RawMessage],
) -> RawMessage {
    let mut response = vec![uint(1, 1), bytes(99, b"response unknown")];
    if let Some(payload) = payload {
        response.push(child(2, payload));
    }
    let mut items: Vec<_> = records.iter().map(|record| child(1, record)).collect();
    items.extend(sessions.iter().map(|session| child(2, session)));
    msg(vec![
        bytes(88, b"wrapper unknown"),
        child(2, &msg(response)),
        child(3, &msg(items)),
    ])
}
fn decode(raw: &RawMessage) -> LoadDataResponse {
    LoadDataResponse::decode(raw.raw()).unwrap()
}
fn bounded(raw: &RawMessage, limits: Limits) -> Result<Option<RawMessage>, Error> {
    let load = decode(raw);
    project_bounded(
        load.raw(),
        load.response(),
        load.preserialized().unwrap(),
        limits,
    )
}

#[test]
fn joins_sidecar_and_appends_without_reordering_any_original_bytes() {
    let mut payload_bytes = vec![0xa0, 0x06, 0x81, 0x00]; // unknown100 nonminimal value
    payload_bytes.extend_from_slice(child(7, &row("ordinary")).raw());
    payload_bytes.extend_from_slice(bytes(101, b"middle").raw());
    payload_bytes.extend_from_slice(uint(100, 2).raw());
    let payload = RawMessage::decode(&payload_bytes).unwrap();
    let mut section_bytes = row("joined").raw().to_vec();
    section_bytes.extend_from_slice(&[0xa0, 0x06, 0x81, 0x00]);
    section_bytes.extend_from_slice(bytes(101, b"first").raw());
    section_bytes.extend_from_slice(bytes(101, b"second").raw());
    let section = RawMessage::decode(&section_bytes).unwrap();
    let mut history_bytes = history("joined").raw().to_vec();
    history_bytes.extend_from_slice(&[0xa0, 0x06, 0x81, 0x00]);
    history_bytes.extend_from_slice(bytes(101, b"middle").raw());
    history_bytes.extend_from_slice(uint(100, 2).raw());
    let session = RawMessage::decode(&history_bytes).unwrap();
    let raw = wrapper(
        Some(&payload),
        &[
            record(2, &[section.clone(), row("missing")]),
            record(2, &[row("last")]),
        ],
        &[session.clone()],
    );
    let load = decode(&raw);
    let projected = load.payload().unwrap().unwrap();
    assert!(projected.raw().starts_with(payload.raw()));
    let rows = load.sections().unwrap();
    assert_eq!(
        rows.iter().map(Section::id).collect::<Vec<_>>(),
        ["ordinary", "joined", "missing", "last"]
    );
    assert!(rows[1].raw().raw().starts_with(section.raw()));
    assert_eq!(rows[1].crdt().unwrap().unwrap().raw(), session.raw());
    assert!(rows[0].crdt().unwrap().is_none());
    assert!(rows[2].crdt().unwrap().is_none());
    assert_eq!(load.raw().raw(), raw.raw());
    assert_eq!(load.response().unwrap(), &raw.message(2).unwrap().unwrap());
    assert_eq!(
        load.preserialized().unwrap(),
        &raw.message(3).unwrap().unwrap()
    );
    assert_eq!(
        projected.messages(7).unwrap(),
        rows.iter().map(|row| row.raw().clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        decode(&RawMessage::decode(load.raw().raw()).unwrap()).payload(),
        Ok(Some(projected))
    );
}
#[test]
fn creates_payload_only_when_preserialized_sections_exist() {
    let raw = wrapper(None, &[record(2, &[row("A")])], &[]);
    assert_eq!(decode(&raw).sections().unwrap()[0].id(), "A");
    let empty = wrapper(None, &[record(2, &[])], &[]);
    assert_eq!(decode(&empty).payload(), Ok(None));
    let missing_response = msg(vec![child(
        3,
        &msg(vec![child(1, &record(2, &[row("A")]))]),
    )]);
    assert_eq!(
        decode(&missing_response).payload(),
        Err(Error::Preserialized)
    );
}
#[test]
fn identical_inline_history_is_retained_but_conflicts_and_wrong_identity_refuse() {
    let session = history("A");
    let inline = row("A").replace_fields(38, &[child(38, &session)]).unwrap();
    let raw = wrapper(None, &[record(2, &[inline.clone()])], &[session.clone()]);
    assert_eq!(decode(&raw).sections().unwrap()[0].raw(), &inline);
    let changed = session
        .replace_fields(5, &[bytes(5, b"different")])
        .unwrap();
    assert_eq!(
        decode(&wrapper(None, &[record(2, &[inline])], &[changed])).payload(),
        Err(Error::Preserialized)
    );
    let wrong = row("A")
        .replace_fields(38, &[child(38, &history("B"))])
        .unwrap();
    assert_eq!(
        decode(&wrapper(None, &[record(2, &[wrong])], &[])).payload(),
        Err(Error::IdentityChanged)
    );
}
#[test]
fn sidecars_never_fill_ordinary_inline_sections_and_orphans_refuse() {
    let payload = msg(vec![child(7, &row("A"))]);
    for sessions in [vec![history("A")], vec![history("orphan")]] {
        assert_eq!(
            decode(&wrapper(
                Some(&payload),
                &[record(2, &[row("B")])],
                &sessions
            ))
            .payload(),
            Err(Error::Preserialized)
        );
    }
}
#[test]
fn duplicate_section_and_session_identities_refuse() {
    let a = row("A");
    let payload = msg(vec![child(7, &a)]);
    for raw in [
        wrapper(Some(&payload), &[record(2, &[a.clone()])], &[]),
        wrapper(None, &[record(2, &[a.clone(), a.clone()])], &[]),
        wrapper(
            None,
            &[record(2, &[a.clone()]), record(2, &[a.clone()])],
            &[],
        ),
        wrapper(None, &[record(2, &[a])], &[history("A"), history("A")]),
    ] {
        assert_eq!(decode(&raw).payload(), Err(Error::Duplicate));
    }
}
#[test]
fn unsupported_or_unknown_dispatch_is_never_silently_discarded() {
    for kind in [0, 1, 79, 83, 95, 999] {
        assert_eq!(
            decode(&wrapper(None, &[record(kind, &[row("A")])], &[])).payload(),
            Err(Error::Preserialized)
        );
    }
    let extra = record(2, &[row("A")])
        .replace_fields(90, &[bytes(90, b"future")])
        .unwrap();
    assert_eq!(
        decode(&wrapper(None, &[extra], &[])).payload(),
        Err(Error::Preserialized)
    );
    let raw = wrapper(None, &[record(2, &[row("A")])], &[]);
    let items = raw
        .message(3)
        .unwrap()
        .unwrap()
        .replace_fields(90, &[uint(90, 1)])
        .unwrap();
    let raw = raw.replace_fields(3, &[child(3, &items)]).unwrap();
    assert_eq!(decode(&raw).payload(), Err(Error::Preserialized));
}
#[test]
fn malformed_records_sections_and_history_fail_before_projection() {
    for bad in [
        msg(vec![child(2, &row("A"))]),
        msg(vec![uint(1, 2), uint(1, 2), child(2, &row("A"))]),
        msg(vec![bytes(1, b"2"), child(2, &row("A"))]),
        msg(vec![uint(1, 2), uint(2, 3)]),
        msg(vec![uint(1, 2), bytes(2, &[0x0a, 0xff])]),
        record(2, &[msg(vec![bytes(1, b"")])]),
    ] {
        assert!(decode(&wrapper(None, &[bad], &[])).payload().is_err());
    }
    for bad in [
        msg(vec![]),
        msg(vec![bytes(1, b"A"), bytes(1, b"A")]),
        msg(vec![bytes(1, b"A"), uint(5, 1)]),
        msg(vec![bytes(1, b"A"), uint(6, 1)]),
        msg(vec![bytes(1, b"A"), bytes(6, &[0xff])]),
        msg(vec![bytes(1, b"A"), uint(7, 2)]),
        msg(vec![bytes(1, b"A"), bytes(9, &[0])]),
    ] {
        assert!(
            decode(&wrapper(None, &[record(2, &[row("A")])], &[bad]))
                .payload()
                .is_err()
        );
    }
}
#[test]
fn aggregate_section_limit_counts_inline_and_every_record() {
    let payload = msg(vec![child(7, &row("ordinary"))]);
    let raw = wrapper(
        Some(&payload),
        &[record(2, &[row("A")]), record(2, &[row("B")])],
        &[],
    );
    assert_eq!(
        bounded(
            &raw,
            Limits {
                sections: 2,
                ..LIMITS
            }
        ),
        Err(Error::TooManyFields)
    );
    assert!(
        bounded(
            &raw,
            Limits {
                sections: 3,
                ..LIMITS
            }
        )
        .is_ok()
    );
}
#[test]
fn aggregate_field_limit_crosses_records_and_sidecar_expansion() {
    let raw = wrapper(
        None,
        &[record(2, &[row("A")]), record(2, &[row("B")])],
        &[history("A"), history("B")],
    );
    // Every individual message is below this bound; the complete join is not.
    assert_eq!(
        bounded(
            &raw,
            Limits {
                fields: 25,
                ..LIMITS
            }
        ),
        Err(Error::TooManyFields)
    );
    let one = wrapper(None, &[record(2, &[row("A")])], &[history("A")]);
    // The retained parsed layers fit 21 fields; adding sidecar fields does not.
    assert_eq!(
        bounded(
            &one,
            Limits {
                fields: 21,
                ..LIMITS
            }
        ),
        Err(Error::TooManyFields)
    );
    assert!(bounded(&raw, LIMITS).is_ok());
}
#[test]
fn retained_input_plus_history_expansion_has_a_separate_byte_budget() {
    let session = history("A");
    let raw = wrapper(None, &[record(2, &[row("A")])], &[session.clone()]);
    let expanded = raw.raw().len() + child(38, &session).raw().len();
    assert_eq!(
        bounded(
            &raw,
            Limits {
                bytes: expanded - 1,
                ..LIMITS
            }
        ),
        Err(Error::TooLarge)
    );
    assert!(
        bounded(
            &raw,
            Limits {
                bytes: expanded,
                ..LIMITS
            }
        )
        .is_ok()
    );
}
