mod single_peer;
use super::*;
use crate::canvas::checklist::split_ops::readback::tests as split;
use super_platinum_core::canvas::{
    edit::text::TextIntent,
    wire::{EditAck, EncodedField as F, RawMessage, Section},
};
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}

fn saving(target: &str) -> (ShellState, Attempt, PreparedTextEdit) {
    let mut c = split::setup();
    assert!(split::prove(&mut c.shell, &c.read));
    assert!(confirm_readback(&mut c.shell, &c.attempt, &c.read));
    let desired = Arc::new(TextDraft::from_rtml("Changed paragraph").unwrap());
    let view = presentation(&c.shell).unwrap();
    let attempt = begin_text(&mut c.shell, view.visit, view.read, target, desired.clone()).unwrap();
    let intent = TextIntent::capture(&c.fresh, target, 100).unwrap();
    let prepared = intent
        .prepare(&c.fresh, &mut EditSession::new(), &desired, 100)
        .unwrap();
    assert!(mark_dispatched(&mut c.shell, &attempt, &prepared));
    (c.shell, attempt, prepared)
}

fn acknowledge(
    prepared: &PreparedTextEdit,
    source: &CanvasCollabSnapshot,
) -> (AcceptedText, AcceptedText, Arc<CanvasCollabSnapshot>) {
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    let original = source.section(prepared.section_id()).unwrap();
    let sequence = original.sequence().unwrap().unwrap() + 1000;
    let wire = update
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    let row = Section::from_raw(
        original
            .merge_partial(&update)
            .unwrap()
            .raw()
            .replace_fields(2, &[n(2, sequence)])
            .unwrap()
            .replace_fields(38, &[m(38, &wire)])
            .unwrap(),
    )
    .unwrap();
    let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, row.raw())]))]).raw()).unwrap();
    let accepted = prepared.validate_ack(&ack).unwrap();
    let replacement = prepared.validate_ack(&ack).unwrap();
    let mut fresh = source.clone();
    *fresh
        .sections
        .iter_mut()
        .find(|s| s.id() == row.id())
        .unwrap() = row.clone();
    (accepted, replacement, Arc::new(fresh))
}

struct Context {
    shell: ShellState,
    attempt: Attempt,
    read: Read,
    fresh: Arc<CanvasCollabSnapshot>,
    replacement: AcceptedText,
}
fn setup() -> Context {
    let (mut shell, attempt, prepared) = saving("paragraph");
    let (accepted, replacement, fresh) = acknowledge(&prepared, &attempt.snapshot);
    assert_eq!(accepted.associated_peer_count(), 2);
    assert!(finish_text(&mut shell, &attempt, accepted));
    assert!(matches!(
        shell.canvas_checklist.operations[0].accepted,
        Some(Accepted::TextSequence(_))
    ));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(fresh.clone())));
    Context {
        shell,
        attempt,
        read,
        fresh,
        replacement,
    }
}
fn prove(shell: &mut ShellState, read: &Read) -> bool {
    Job::capture(shell, read)
        .and_then(Job::run)
        .is_some_and(|proof| proof.publish(shell))
}
#[test]
fn text_sequence_requires_executed_proof_while_ordinary_text_keeps_existing_route() {
    let mut c = setup();
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &c.read));
    assert!(prove(&mut c.shell, &c.read));
    assert!(confirm_readback(&mut c.shell, &c.attempt, &c.read));
    assert!(c.shell.canvas_checklist.operations.is_empty());
    let (mut shell, attempt, prepared) = saving("neighbor");
    let (accepted, _, fresh) = acknowledge(&prepared, &attempt.snapshot);
    assert_eq!(accepted.associated_peer_count(), 0);
    assert!(finish_text(&mut shell, &attempt, accepted));
    assert!(matches!(
        shell.canvas_checklist.operations[0].accepted,
        Some(Accepted::Text(_))
    ));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(fresh)));
    assert!(Job::capture(&shell, &read).is_none());
    assert!(confirm_readback(&mut shell, &attempt, &read));
}
#[test]
fn held_text_sequence_proof_rejects_owner_read_operation_and_draft_replacement() {
    for change in 0..13 {
        let mut c = setup();
        let proof = Job::capture(&c.shell, &c.read).unwrap().run().unwrap();
        match change {
            0 => {
                begin_read(&mut c.shell).unwrap();
            }
            1 => {
                c.shell.canvas_checklist.view.as_mut().unwrap().snapshot =
                    Some(Arc::new((*c.fresh).clone()));
            }
            2 => {
                assert!(invalidate(&mut c.shell, "T1", "Ffixture"));
            }
            3 => {
                open_visit(&mut c.shell);
            }
            4 => {
                c.shell.core.account_epoch += 1;
            }
            5 => {
                c.shell.core.active_account = Some("other".into());
            }
            6 => {
                c.shell.canvas_checklist.operations[0].accepted =
                    Some(Accepted::TextSequence(AcceptedState::new(c.replacement)));
            }
            7 => {
                c.shell.canvas_checklist.operations[0].attempt.serial += 1;
            }
            8 => {
                c.shell.canvas_checklist.operations[0]
                    .dispatched
                    .as_mut()
                    .unwrap()
                    .1 += 1;
            }
            9 => {
                c.shell.canvas_checklist.operations[0].dispatched_after_read += 1;
            }
            10 => {
                c.shell.canvas_checklist.operations[0].stage = Stage::Uncertain;
            }
            11 => {
                c.shell.canvas_checklist.operations[0].attempt.section = "other".into();
            }
            12 => {
                let Mutation::Text(draft) =
                    &mut c.shell.canvas_checklist.operations[0].attempt.mutation
                else {
                    unreachable!()
                };
                *draft = Arc::new((**draft).clone());
            }
            _ => unreachable!(),
        }
        assert!(!proof.publish(&mut c.shell), "change {change}");
        assert!(
            !confirm_readback(&mut c.shell, &c.attempt, &c.read),
            "change {change}"
        );
        assert_eq!(c.shell.canvas_checklist.operations.len(), 1);
    }
}
#[test]
fn text_sequence_proof_cannot_reuse_new_read_or_accept_missing_associated_peer() {
    let mut c = setup();
    assert!(prove(&mut c.shell, &c.read));
    let newer = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &newer, Ok(c.fresh.clone())));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &newer));
    assert!(prove(&mut c.shell, &newer));
    let mut changed = (*c.fresh).clone();
    let peer = changed
        .sections
        .iter()
        .find(|s| !matches!(s.id(), "title" | "paragraph" | "neighbor"))
        .unwrap()
        .id()
        .to_owned();
    changed.sections.retain(|s| s.id() != peer);
    let latest = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &latest, Ok(Arc::new(changed))));
    assert!(!prove(&mut c.shell, &latest));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &latest));
}
#[test]
fn fixture_text_completion_requires_exact_saving_dispatch_and_draft() {
    let (mut shell, attempt, prepared) = saving("paragraph");
    let session = prepared.session_id();
    let sequence = prepared.sequence();
    assert!(fixture_completion_current(
        &shell, &attempt, session, sequence
    ));
    assert!(!fixture_completion_current(
        &shell,
        &attempt,
        "other-session",
        sequence
    ));
    assert!(!fixture_completion_current(
        &shell,
        &attempt,
        session,
        sequence + 1
    ));
    let original_account = shell.core.active_account.clone();
    shell.core.active_account = Some("other".into());
    assert!(!fixture_completion_current(
        &shell, &attempt, session, sequence
    ));
    shell.core.active_account = original_account;
    let original_draft = shell.canvas_checklist.operations[0]
        .attempt
        .mutation
        .clone();
    if let Mutation::Text(draft) = &mut shell.canvas_checklist.operations[0].attempt.mutation {
        *draft = Arc::new((**draft).clone());
    }
    assert!(!fixture_completion_current(
        &shell, &attempt, session, sequence
    ));
    shell.canvas_checklist.operations[0].attempt.mutation = original_draft;
    assert!(fixture_completion_current(
        &shell, &attempt, session, sequence
    ));
    shell.canvas_checklist.view.as_mut().unwrap().snapshot =
        Some(Arc::new((*attempt.snapshot).clone()));
    assert!(!fixture_completion_current(
        &shell, &attempt, session, sequence
    ));
    shell.canvas_checklist.view.as_mut().unwrap().snapshot = Some(attempt.snapshot.clone());
    shell.canvas_checklist.view.as_mut().unwrap().revision += 1;
    assert!(!fixture_completion_current(
        &shell, &attempt, session, sequence
    ));
}
