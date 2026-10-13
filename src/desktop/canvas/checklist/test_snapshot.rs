//! Synthetic canonical protobufs only; no private capture bytes or network.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use super_platinum_core::{
    canvas::{
        edit::CheckboxIntent,
        wire::{EditAck, EncodedField as F, LoadDataResponse, RawMessage},
    },
    config::WorkspaceSession,
    slack::api::CanvasControllerInit,
};
fn msg(fields: Vec<F>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn s(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn u(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
pub(in crate::canvas::checklist) fn snapshot(
    checked: bool,
    sequence: u64,
) -> Arc<CanvasCollabSnapshot> {
    build_snapshot(checked, sequence, false)
}
pub(in crate::canvas::checklist) fn local_snapshot(
    checked: bool,
    sequence: u64,
) -> Arc<CanvasCollabSnapshot> {
    build_snapshot(checked, sequence, true)
}
fn build_snapshot(checked: bool, sequence: u64, local: bool) -> Arc<CanvasCollabSnapshot> {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://synthetic.slack.com/".into(),
        token: "synthetic".into(),
    };
    let options = msg(vec![s(1, "quip-user"), s(33, "U1"), s(34, "T1")]);
    let init = CanvasControllerInit::parse(serde_json::json!({"ok":true,"quip_user_id":"quip-user","init_options":STANDARD.encode(options.raw())}),&workspace).unwrap();
    let file = serde_json::from_value(serde_json::json!({"id":"FCANVAS","filetype":"quip","quip_thread_id":"thread","editable":true})).unwrap();
    let thread = msg(vec![
        s(1, "thread"),
        u(2, 1000),
        s(10, "document"),
        u(22, 13),
        u(23, 13),
        u(119, 1),
    ]);
    let document = msg(vec![
        s(1, "document"),
        u(2, 1000),
        s(6, "thread"),
        u(12, 1),
        m(69, &msg(vec![s(1, "Fixture title")])),
    ]);
    let title = msg(vec![
        s(1, "title"),
        u(2, 1000),
        s(6, "thread"),
        s(7, "document"),
        u(9, 64),
        u(10, 48),
        m(12, &msg(vec![m(58, &msg(vec![s(1, "Fixture title")]))])),
    ]);
    let parent = msg(vec![
        s(1, "parent"),
        u(2, 1000),
        s(6, "thread"),
        s(7, "document"),
        u(10, 7),
    ]);
    let attrs = msg(vec![u(2, u64::from(checked))]);
    let parents = msg(vec![s(1, "parent"), u(4, 7)]);
    let crdt = msg(vec![
        s(1, "item"),
        u(2, 1000),
        u(3, 1000),
        u(4, 1_000_000),
        s(5, "synthetic-crdt"),
        s(6, "synthetic-head"),
        s(8, "base"),
    ]);
    let mut section = msg(vec![
        s(1, "item"),
        u(2, sequence),
        s(6, "thread"),
        s(7, "document"),
        m(13, &parents),
        m(16, &attrs),
        m(38, &crdt),
    ]);
    if local {
        section = section.replace_fields(38, &[]).unwrap();
        let content = msg(vec![m(
            1,
            &msg(vec![s(1, "Check keyboard navigation and narrow layouts")]),
        )]);
        section = section.replace_fields(12, &[m(12, &content)]).unwrap();
    }
    let payload = msg(vec![
        m(3, &thread),
        m(6, &document),
        m(7, &title),
        m(7, &parent),
        m(7, &section),
    ]);
    let index = msg(vec![
        s(1, "Section/document"),
        m(2, &msg(vec![s(1, "title")])),
        m(2, &msg(vec![s(1, "parent")])),
        m(2, &msg(vec![s(1, "item")])),
    ]);
    let response = msg(vec![u(1, 1), m(2, &payload), m(3, &index)]);
    let load = LoadDataResponse::decode(msg(vec![m(2, &response)]).raw()).unwrap();
    Arc::new(CanvasCollabSnapshot::from_responses(file, init, load).unwrap())
}
pub(in crate::canvas::checklist) fn prepare(
    shell: &mut ShellState,
    attempt: &Attempt,
) -> PreparedCheckboxEdit {
    let intent = CheckboxIntent::capture(
        &attempt.snapshot,
        &attempt.section,
        attempt.checked().unwrap(),
        10,
    )
    .unwrap();
    with_edit_session(shell, attempt, |session| {
        intent.prepare(&attempt.snapshot, session, 10).unwrap()
    })
    .unwrap()
}
pub(in crate::canvas::checklist) fn accepted(prepared: &PreparedCheckboxEdit) -> AcceptedCheckbox {
    let canonical = snapshot(prepared.desired(), prepared.base_sequence() + 1000);
    accepted_with_snapshot(prepared, &canonical, 1_000_000_000_000)
}
pub(in crate::canvas::checklist) fn accepted_with_snapshot(
    prepared: &PreparedCheckboxEdit,
    canonical: &CanvasCollabSnapshot,
    expiry: u64,
) -> AcceptedCheckbox {
    let section = canonical.section("item").unwrap();
    let crdt = prepared
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap()
        .replace_fields(3, &[u(3, section.sequence().unwrap().unwrap())])
        .unwrap()
        .replace_fields(4, &[u(4, expiry)])
        .unwrap();
    let acknowledged = section.raw().replace_fields(38, &[m(38, &crdt)]).unwrap();
    let data = msg(vec![s(15, "document"), m(1, &acknowledged)]);
    let ack = EditAck::decode(msg(vec![u(1, 1), m(2, &data)]).raw()).unwrap();
    prepared.validate_ack(&ack).unwrap()
}

pub(super) fn snapshot_for_file(
    file: &str,
    checked: bool,
    sequence: u64,
) -> Arc<CanvasCollabSnapshot> {
    let original = snapshot(checked, sequence);
    let mut metadata = original.file.clone();
    metadata.id = Some(file.into());
    Arc::new(
        CanvasCollabSnapshot::from_responses(
            metadata,
            original.init.clone(),
            original.load.clone(),
        )
        .unwrap(),
    )
}
