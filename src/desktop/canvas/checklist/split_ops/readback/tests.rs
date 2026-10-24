use super::*;
use crate::canvas::text::structure_fixture;
use super_platinum_core::canvas::{
    edit::{structure::recovery::Checkpoint, text::TextDraft},
    wire::{EditAck, EncodedField as F, RawMessage, Section},
};

fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn t(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}

pub(in crate::canvas::checklist) struct Context {
    pub shell: ShellState,
    pub attempt: Attempt,
    pub read: Read,
    pub fresh: Arc<CanvasCollabSnapshot>,
    pub checkpoint: Checkpoint,
    replacement: AcceptedSplit,
}

pub(in crate::canvas::checklist) fn setup() -> Context {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let mut source = (*structure_fixture::snapshot()).clone();
    let parent = source
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap();
    let body = msg(&[m(1, &msg(&[t(1, "First paragraph"), t(2, "en")]))]);
    *parent = Section::from_raw(parent.raw().replace_fields(12, &[m(12, &body)]).unwrap()).unwrap();
    let source = Arc::new(source);
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(source.clone())));
    let seed = TextDraft::from_rtml("First paragraph").unwrap();
    use super_platinum_core::canvas::edit::structure::SplitSequenceTarget;
    let left = TextDraft::from_rtml("First ").unwrap();
    let witness = SplitWitness::new(&seed, 6, &left, &TextDraft::from_rtml("paragraph").unwrap())
        .unwrap()
        .with_second_split(
            SplitSequenceTarget::Original,
            &left,
            3,
            &TextDraft::from_rtml("Fir").unwrap(),
            &TextDraft::from_rtml("st ").unwrap(),
        )
        .unwrap();
    let view = presentation(&shell).unwrap();
    let (attempt, intent) =
        begin_split(&mut shell, view.visit, view.read, "paragraph", witness, 100).unwrap();
    let prepared = intent
        .prepare(
            &source,
            crate::canvas::checklist::split_ops::witness(&attempt).unwrap(),
            100,
        )
        .unwrap();
    let checkpoint = prepared.checkpoint().unwrap();
    assert!(mark_dispatched(&mut shell, &attempt, &prepared));
    let rows = prepared
        .document_data()
        .messages(1)
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(index, update)| {
            let update = Section::from_raw(update).unwrap();
            let previous = source.section(update.id());
            let sequence = previous.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
            let mut raw = previous.map_or_else(
                || update.raw().clone(),
                |s| s.merge_partial(&update).unwrap().raw().clone(),
            );
            for field in [
                n(2, sequence),
                t(6, source.thread_id()),
                t(7, source.document_id()),
                m(
                    12,
                    &msg(&[m(
                        1,
                        &msg(&if index == 0 {
                            vec![
                                t(1, &prepared.member_drafts()[index].rtml().unwrap()),
                                t(2, "en"),
                            ]
                        } else {
                            vec![t(1, &prepared.member_drafts()[index].rtml().unwrap())]
                        }),
                    )]),
                ),
            ] {
                raw = raw
                    .merge_partial(&RawMessage::decode(field.raw()).unwrap())
                    .unwrap();
            }
            if previous.is_none() {
                let position = raw
                    .string(8)
                    .unwrap()
                    .unwrap()
                    .strip_suffix(":temp")
                    .unwrap()
                    .to_owned();
                raw = raw
                    .replace_fields(8, &[t(8, &position)])
                    .unwrap()
                    .replace_fields(21, &[t(21, &position)])
                    .unwrap();
            }
            let wire = raw
                .message(38)
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[n(3, sequence)])
                .unwrap()
                .replace_fields(4, &[n(4, 1_000_000)])
                .unwrap();
            Section::from_raw(raw.replace_fields(38, &[m(38, &wire)]).unwrap()).unwrap()
        })
        .collect::<Vec<_>>();
    let ack = EditAck::decode(
        msg(&[
            n(1, 1),
            m(
                2,
                &msg(&rows.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>()),
            ),
        ])
        .raw(),
    )
    .unwrap();
    let accepted = prepared.validate_ack(&ack).unwrap();
    let replacement = prepared.validate_ack(&ack).unwrap();
    let mut fresh = (*source).clone();
    for row in rows {
        if let Some(current) = fresh.sections.iter_mut().find(|s| s.id() == row.id()) {
            *current = row;
        } else {
            fresh.sections.push(row);
        }
    }
    let fresh = Arc::new(fresh);
    assert!(finish_split(&mut shell, &attempt, accepted));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(fresh.clone())));
    Context {
        shell,
        attempt,
        read,
        fresh,
        checkpoint,
        replacement,
    }
}

pub(in crate::canvas::checklist) fn prove(shell: &mut ShellState, read: &Read) -> bool {
    Job::capture(shell, read)
        .and_then(Job::run)
        .is_some_and(|proof| proof.publish(shell))
}

#[test]
fn split_sequence_unlock_requires_executed_core_proof_of_exact_read() {
    let mut c = setup();
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &c.read));
    assert!(prove(&mut c.shell, &c.read));
    assert!(confirm_readback(&mut c.shell, &c.attempt, &c.read));
    assert!(c.shell.canvas_checklist.operations.is_empty());
}

#[test]
fn held_split_sequence_proof_rejects_replaced_owner_read_snapshot_or_operation() {
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
                    Some(Accepted::SplitSequence(AcceptedState::new(c.replacement)));
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
                let Mutation::Split { second_child, .. } =
                    &mut c.shell.canvas_checklist.operations[0].attempt.mutation
                else {
                    unreachable!()
                };
                *second_child = Some("other-child".into());
            }
            12 => {
                let Mutation::Split { witness, .. } =
                    &mut c.shell.canvas_checklist.operations[0].attempt.mutation
                else {
                    unreachable!()
                };
                *witness = Arc::new((**witness).clone());
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
fn cached_split_sequence_proof_cannot_authorize_newer_read_or_changed_inventory() {
    let mut c = setup();
    assert!(prove(&mut c.shell, &c.read));
    let newer = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &newer, Ok(c.fresh.clone())));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &newer));
    assert!(prove(&mut c.shell, &newer));
    let mut changed = (*c.fresh).clone();
    changed.sections.retain(|s| s.id() != "neighbor");
    let latest = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &latest, Ok(Arc::new(changed))));
    assert!(!prove(&mut c.shell, &latest));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &latest));
}

#[test]
fn split_sequence_worker_refuses_missing_third_member() {
    let mut c = setup();
    let third = match c.shell.canvas_checklist.operations[0]
        .accepted
        .as_ref()
        .unwrap()
    {
        Accepted::SplitSequence(accepted) => accepted.value.second_child_id().unwrap().to_owned(),
        _ => unreachable!(),
    };
    let mut missing = (*c.fresh).clone();
    missing.sections.retain(|section| section.id() != third);
    let read = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &read, Ok(Arc::new(missing))));
    assert!(!prove(&mut c.shell, &read));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &read));
    assert_eq!(c.shell.canvas_checklist.operations.len(), 1);
}
