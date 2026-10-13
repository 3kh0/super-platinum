use super::*;

fn msg(fields: Vec<F>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn text(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn num(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn nested(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn fixture() -> CanvasCollabSnapshot {
    let attrs = msg(vec![num(1, 0), num(2, 0), text(99, "unknown attribute")]);
    let crdt = msg(vec![
        text(1, "item"),
        num(2, 10_000),
        num(3, 10_000),
        num(4, 1_000_000),
        F::bytes(5, b"opaque automerge fixture").unwrap(),
        text(6, "head"),
        text(8, "base"),
    ]);
    let item = Section::from_raw(msg(vec![
        text(1, "item"),
        num(2, 10_000),
        text(6, "ThTest"),
        text(7, "Dtest"),
        num(9, 0),
        num(10, 0),
        nested(12, &msg(vec![text(1, "untouched content")])),
        nested(
            13,
            &msg(vec![
                text(1, "parent"),
                text(2, "server container"),
                num(4, 7),
            ]),
        ),
        nested(16, &attrs),
        nested(38, &crdt),
    ]))
    .unwrap();
    let parent = Section::from_raw(msg(vec![
        text(1, "parent"),
        num(2, 10_000),
        text(6, "ThTest"),
        text(7, "Dtest"),
        num(10, 7),
    ]))
    .unwrap();
    let document = msg(vec![
        text(1, "Dtest"),
        num(2, 10_000),
        text(6, "ThTest"),
        num(12, 1),
    ]);
    let title = Section::from_raw(msg(vec![
        text(1, "title"),
        num(2, 10_000),
        text(6, "ThTest"),
        text(7, "Dtest"),
        num(9, 64),
        num(10, 48),
        nested(
            12,
            &msg(vec![nested(
                58,
                &msg(vec![text(1, "Synthetic canvas title")]),
            )]),
        ),
    ]))
    .unwrap();
    crate::slack::api::canvas_collab_test_snapshot(vec![item, parent, title], document)
}

#[test]
fn checkbox_partial_preserves_attributes_and_crdt_without_replacing_content() {
    let snapshot = fixture();
    let intent = CheckboxIntent::capture(&snapshot, "item", true, 100).unwrap();
    let mut session = EditSession::new();
    let prepared = intent.prepare(&snapshot, &mut session, 100).unwrap();
    let data = super::super::wire::DocumentData::from_raw(prepared.data.clone()).unwrap();
    let item = &data.sections()[0];
    assert_eq!(item.checked().unwrap(), Some(true));
    assert!(
        !item.raw().has(12),
        "checkbox edit must never replace content"
    );
    assert_eq!(
        item.attributes().unwrap().unwrap().string(99).unwrap(),
        Some("unknown attribute")
    );
    let parents = item.parents().unwrap().unwrap();
    assert_eq!(parents.string(1).unwrap(), Some("parent"));
    assert!(
        !parents.has(2),
        "server container identity is reconstructed"
    );
    let crdt = item.crdt().unwrap().unwrap();
    assert!(!crdt.has(4));
    assert!(!crdt.has(7));
    assert_eq!(
        crdt.bytes(5).unwrap(),
        Some(b"opaque automerge fixture".as_slice())
    );
    assert_eq!(prepared.sequence, 1);
    assert_eq!(
        item.raw().string(33).unwrap(),
        Some(format!("{}-0", prepared.session).as_str())
    );
    assert_eq!(item.raw().uint(20).unwrap(), Some(1));
    assert_eq!(item.raw().uint(39).unwrap(), Some(2));
    assert_eq!(prepared.form_title(), "Synthetic canvas title");
    assert!(!snapshot.document.has(69));
    assert_eq!(
        data.raw().bytes(14).unwrap(),
        Some(intent.title.rich_text().raw())
    );
}

#[test]
fn stale_preflight_never_reserves_a_save_sequence() {
    let snapshot = fixture();
    let intent = CheckboxIntent::capture(&snapshot, "item", true, 100).unwrap();
    let mut fresh = fixture();
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(2, &[num(2, 11_000)])
            .unwrap(),
    )
    .unwrap();
    let mut session = EditSession::new();
    assert!(matches!(
        intent.prepare(&fresh, &mut session, 100),
        Err(Error::Changed)
    ));
    assert_eq!(session.sequence, 0);
    assert!(session.local.is_empty());
    fresh = fixture();
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(
                12,
                &[nested(12, &msg(vec![text(1, "changed without sequence")]))],
            )
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        intent.prepare(&fresh, &mut session, 100),
        Err(Error::Changed)
    ));
    assert_eq!(session.sequence, 0);
}

#[test]
fn locks_expiry_and_unsupported_crdt_fail_before_a_request_exists() {
    let mut snapshot = fixture();
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 1_000_000),
        Err(Error::Expired)
    ));
    snapshot.document = snapshot
        .document
        .replace_fields(9, &[nested(9, &msg(vec![text(1, "locked")]))])
        .unwrap();
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 100),
        Err(Error::ReadOnly)
    ));
    snapshot = fixture();
    let crdt = snapshot.sections[0]
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(12, &[text(12, "pending local head")])
        .unwrap();
    snapshot.sections[0] = Section::from_raw(
        snapshot.sections[0]
            .raw()
            .replace_fields(38, &[nested(38, &crdt)])
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 100),
        Err(Error::Unsupported)
    ));
}

#[test]
fn reachable_edit_permission_is_required_even_with_editable_file_metadata() {
    let mut snapshot = fixture();
    snapshot.thread = snapshot.thread.remove(23).unwrap();
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 100),
        Err(Error::ReadOnly)
    ));
    snapshot.thread = snapshot.thread.replace_fields(23, &[num(23, 1)]).unwrap();
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 100),
        Err(Error::ReadOnly)
    ));
    snapshot.thread = snapshot.thread.replace_fields(23, &[num(23, 0)]).unwrap();
    assert!(CheckboxIntent::capture(&snapshot, "item", true, 100).is_ok());
    snapshot.thread = snapshot.thread.replace_fields(303, &[num(303, 1)]).unwrap();
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 100),
        Err(Error::ReadOnly)
    ));
}

#[test]
fn malformed_heads_and_missing_canonical_title_do_not_form_updates() {
    for head in [num(6, 1), text(6, ""), F::bytes(6, &[0xff]).unwrap()] {
        let mut snapshot = fixture();
        let crdt = snapshot.sections[0]
            .crdt()
            .unwrap()
            .unwrap()
            .replace_fields(6, &[head])
            .unwrap();
        snapshot.sections[0] = Section::from_raw(
            snapshot.sections[0]
                .raw()
                .replace_fields(38, &[nested(38, &crdt)])
                .unwrap(),
        )
        .unwrap();
        assert!(CheckboxIntent::capture(&snapshot, "item", true, 100).is_err());
    }
    let mut snapshot = fixture();
    snapshot.sections.retain(|section| section.id() != "title");
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 100),
        Err(Error::Unsupported)
    ));
}

fn acknowledgement(snapshot: &CanvasCollabSnapshot, checked: bool, sequence: u64) -> EditAck {
    let section = &snapshot.sections[0];
    let attrs = section
        .attributes()
        .unwrap()
        .unwrap()
        .replace_fields(2, &[num(2, u64::from(checked))])
        .unwrap();
    let raw = section
        .raw()
        .replace_fields(2, &[num(2, sequence)])
        .unwrap()
        .replace_fields(16, &[nested(16, &attrs)])
        .unwrap();
    let data = msg(vec![nested(1, &raw)]);
    EditAck::decode(msg(vec![num(1, 1), nested(2, &data)]).raw()).unwrap()
}

#[test]
fn only_matching_direct_acknowledgement_produces_acceptance() {
    let snapshot = fixture();
    let intent = CheckboxIntent::capture(&snapshot, "item", true, 100).unwrap();
    let prepared = intent
        .prepare(&snapshot, &mut EditSession::new(), 100)
        .unwrap();
    assert!(
        prepared
            .validate_ack(&acknowledgement(&snapshot, true, 12_000))
            .is_ok()
    );
    assert!(matches!(
        prepared.validate_ack(&acknowledgement(&snapshot, false, 12_000)),
        Err(Error::Unconfirmed)
    ));
    assert!(matches!(
        prepared.validate_ack(&acknowledgement(&snapshot, true, 10_000)),
        Err(Error::Unconfirmed)
    ));
    let mut ack = acknowledgement(&snapshot, true, 12_000);
    ack.is_new_edit_session = Some(true);
    assert!(matches!(
        prepared.validate_ack(&ack),
        Err(Error::Unconfirmed)
    ));
    ack = acknowledgement(&snapshot, true, 12_000);
    ack.changed = None;
    assert!(matches!(
        prepared.validate_ack(&ack),
        Err(Error::Unconfirmed)
    ));
}

pub(super) fn plain_without_crdt() -> CanvasCollabSnapshot {
    let mut snapshot = fixture();
    let content = msg(vec![nested(1, &msg(vec![text(1, "Ordinary checklist")]))]);
    snapshot.sections[0] = Section::from_raw(
        snapshot.sections[0]
            .raw()
            .remove(38)
            .unwrap()
            .replace_fields(12, &[nested(12, &content)])
            .unwrap(),
    )
    .unwrap();
    snapshot
}

#[test]
fn missing_crdt_bootstraps_real_session_without_server_expiry_or_content_replacement() {
    use automerge::{ROOT, ReadDoc};
    let snapshot = plain_without_crdt();
    let canonical = snapshot.sections[0].clone();
    let intent = CheckboxIntent::capture(&snapshot, "item", true, u64::MAX).unwrap();
    let mut session = EditSession::new();
    let prepared = intent.prepare(&snapshot, &mut session, u64::MAX).unwrap();
    let data = super::super::wire::DocumentData::from_raw(prepared.data.clone()).unwrap();
    let update = &data.sections()[0];
    let crdt = update.crdt().unwrap().unwrap();
    assert!(
        !crdt.has(4),
        "local state must never fabricate server expiry"
    );
    assert!(!crdt.has(7));
    assert!(
        !update.raw().has(12),
        "the save remains an attribute-only update"
    );
    let document = automerge::Automerge::load(crdt.bytes(5).unwrap().unwrap()).unwrap();
    let (_, text_object) = document.get(ROOT, "text").unwrap().unwrap();
    assert_eq!(
        document.text(text_object).unwrap(),
        "Ordinary checklist\u{feff}"
    );
    let heads = document.get_heads();
    assert_eq!(heads.len(), 1);
    assert_eq!(crdt.string(6).unwrap(), Some(heads[0].to_string().as_str()));
    assert_eq!(crdt.string(8).unwrap(), crdt.string(6).unwrap());
    assert_eq!(
        document.get_changes(&[])[0].actor_id().to_hex_string(),
        format!("{}00000000", prepared.session_id())
    );
    assert_eq!(crdt.uint(2).unwrap(), Some(10_000));
    assert_eq!(crdt.uint(3).unwrap(), Some(10_000));
    assert_eq!(session.sequence, 1);
    assert_eq!(session.local.get("item"), Some(&1));
    assert_eq!(canonical, snapshot.sections[0]);
}

#[test]
fn present_server_crdt_never_uses_local_bootstrap_to_bypass_expiration() {
    let snapshot = fixture();
    let intent = CheckboxIntent::capture(&snapshot, "item", true, 100).unwrap();
    let mut session = EditSession::new();
    assert!(matches!(
        intent.prepare(&snapshot, &mut session, 1_000_000),
        Err(Error::Expired)
    ));
    assert_eq!(session.sequence, 0);
    assert!(session.local.is_empty());
    let mut snapshot = plain_without_crdt();
    let without_expiry = fixture().sections[0]
        .crdt()
        .unwrap()
        .unwrap()
        .remove(4)
        .unwrap();
    snapshot.sections[0] = Section::from_raw(
        snapshot.sections[0]
            .raw()
            .replace_fields(38, &[nested(38, &without_expiry)])
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        CheckboxIntent::capture(&snapshot, "item", true, 100),
        Err(Error::Expired)
    ));
    assert!(matches!(
        intent.prepare(&snapshot, &mut session, 100),
        Err(Error::Expired)
    ));
    assert_eq!(session.sequence, 0);
    assert!(session.local.is_empty());
}

#[test]
fn bootstrap_failure_never_reserves_counters_and_multiple_ack_sections_are_unconfirmed() {
    let snapshot = plain_without_crdt();
    let intent = CheckboxIntent::capture(&snapshot, "item", true, 100).unwrap();
    let mut session = EditSession::new();
    session.id = "invalid-session".into();
    assert!(matches!(
        intent.prepare(&snapshot, &mut session, 100),
        Err(Error::Unsupported)
    ));
    assert_eq!(session.sequence, 0);
    assert!(session.local.is_empty());
    let prepared = intent
        .prepare(&snapshot, &mut EditSession::new(), 100)
        .unwrap();
    let mut ack = acknowledgement(&snapshot, true, 12_000);
    let one = ack.final_data.as_ref().unwrap().sections()[0].raw();
    let many = msg(vec![nested(1, one), nested(1, snapshot.sections[1].raw())]);
    ack.final_data = Some(super::super::wire::DocumentData::from_raw(many).unwrap());
    assert!(matches!(
        prepared.validate_ack(&ack),
        Err(Error::Unconfirmed)
    ));
}

#[test]
fn title_preflight_failure_preserves_counters_and_prepared_rechecks_canonical_inputs() {
    let snapshot = plain_without_crdt();
    let intent = CheckboxIntent::capture(&snapshot, "item", true, 100).unwrap();
    let mut fresh = plain_without_crdt();
    fresh.sections[2] = Section::from_raw(
        fresh.sections[2]
            .raw()
            .replace_fields(2, &[num(2, 11_000)])
            .unwrap(),
    )
    .unwrap();
    let mut session = EditSession::new();
    assert!(matches!(
        intent.prepare(&fresh, &mut session, 100),
        Err(Error::Changed)
    ));
    assert_eq!(session.sequence, 0);
    assert!(session.local.is_empty());
    fresh = plain_without_crdt();
    let changed = msg(vec![nested(58, &msg(vec![text(1, "Another title")]))]);
    fresh.sections[2] = Section::from_raw(
        fresh.sections[2]
            .raw()
            .replace_fields(12, &[nested(12, &changed)])
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        intent.prepare(&fresh, &mut session, 100),
        Err(Error::Changed)
    ));
    assert_eq!(session.sequence, 0);
    let prepared = intent.prepare(&snapshot, &mut session, 100).unwrap();
    assert!(prepared.validate_snapshot(&snapshot, 100).is_ok());
    assert!(matches!(
        prepared.validate_snapshot(&fresh, 100),
        Err(Error::Changed)
    ));
    fresh = plain_without_crdt();
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(2, &[num(2, 11_000)])
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        prepared.validate_snapshot(&fresh, 100),
        Err(Error::Changed)
    ));
    assert_eq!(prepared.workspace_id(), "Ttest");
    assert_eq!(prepared.user_id(), "Utest");
}
