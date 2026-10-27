use super::*;
use crate::canvas::text::replacement_fixture as fixture;
fn setup() -> (ShellState, Attempt, Read, Arc<CanvasCollabSnapshot>, String) {
    setup_mode(false)
}
fn setup_mode(durable: bool) -> (ShellState, Attempt, Read, Arc<CanvasCollabSnapshot>, String) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Pair proof".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let source = fixture::witnessed_snapshot(100);
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(source.clone())));
    let view = presentation(&shell).unwrap();
    let (attempt, intent) =
        begin_replacement(&mut shell, view.visit, view.read, "paragraph", 100).unwrap();
    let prepared = intent.prepare(&source, 100).unwrap();
    let id = prepared.replacement_section().to_owned();
    if durable {
        shell.canvas_checklist.operations[0].durable = Some(recovery::confirmed::tests::ticket(
            prepared.checkpoint().unwrap(),
        ));
    }
    assert!(mark_dispatched(&mut shell, &attempt, &prepared));
    let (accepted, fresh) = fixture::accepted(prepared, &source, 100);
    let mut current = (*fresh).clone();
    current.sections.push(accepted.deleted_section().clone());
    let current = Arc::new(current);
    assert!(finish_replacement(&mut shell, &attempt, accepted));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(current.clone())));
    (shell, attempt, read, current, id)
}

#[test]
fn sealed_retirement_removes_closed_operation_without_publishing_old_view() {
    for newer_visit in [false, true] {
        let (mut shell, attempt, read, _, _) = setup_mode(true);
        assert!(
            Job::capture(&shell, &read)
                .unwrap()
                .run()
                .unwrap()
                .publish(&mut shell)
        );
        assert!(!confirm_readback(&mut shell, &attempt, &read));
        let ticket = shell.canvas_checklist.operations[0]
            .durable
            .clone()
            .unwrap();
        shell.canvas_checklist.operations[0].durable_finishing = true;
        close_visit(&mut shell);
        if newer_visit {
            shell.shortcut_generation += 1;
            open_visit(&mut shell);
        }
        let before = shell
            .canvas_checklist
            .view
            .as_ref()
            .map(|v| (v.generation, v.read, v.revision, v.stale, v.loading));
        assert!(recovery::confirmed::finish(
            &mut shell, &attempt, &ticket, true
        ));
        assert!(shell.canvas_checklist.operations.is_empty());
        assert_eq!(
            before,
            shell.canvas_checklist.view.as_ref().map(|v| (
                v.generation,
                v.read,
                v.revision,
                v.stale,
                v.loading
            ))
        );
        assert!(
            shell
                .canvas_checklist
                .view
                .as_ref()
                .is_none_or(|v| v.snapshot.is_none() && v.settled_text.is_none())
        );
    }
}

#[test]
fn sealed_retirement_requires_exact_ticket_proof_and_success() {
    for change in 0..5 {
        let (mut shell, attempt, read, _, _) = setup_mode(true);
        assert!(
            Job::capture(&shell, &read)
                .unwrap()
                .run()
                .unwrap()
                .publish(&mut shell)
        );
        assert!(!confirm_readback(&mut shell, &attempt, &read));
        let ticket = shell.canvas_checklist.operations[0]
            .durable
            .clone()
            .unwrap();
        shell.canvas_checklist.operations[0].durable_finishing = true;
        match change {
            0 => shell.canvas_checklist.operations[0].durable = None,
            1 => {
                shell.canvas_checklist.operations[0].durable =
                    Some(recovery::confirmed::tests::foreign(ticket.clone()))
            }
            2 => shell.canvas_checklist.operations[0].accepted = None,
            3 => {
                if let Some(Accepted::Replacement(a)) =
                    shell.canvas_checklist.operations[0].accepted.as_mut()
                {
                    a.proof = None;
                }
            }
            _ => (),
        }
        close_visit(&mut shell);
        assert_eq!(
            recovery::confirmed::finish(&mut shell, &attempt, &ticket, change != 4),
            change == 4
        );
        assert_eq!(shell.canvas_checklist.operations.len(), 1);
        if change == 4 {
            assert!(!shell.canvas_checklist.operations[0].durable_finishing);
            assert!(
                shell.canvas_checklist.operations[0]
                    .durable
                    .as_ref()
                    .unwrap()
                    .same(&ticket)
            );
        } else {
            assert!(shell.canvas_checklist.operations[0].durable_finishing);
        }
    }
}
#[test]
fn only_executed_current_pair_worker_publishes_writable_provenance() {
    let (mut shell, attempt, read, raw, id) = setup();
    assert!(ReplacementIntent::main_canvas(&raw, &id, 100).is_err());
    assert!(!confirm_readback(&mut shell, &attempt, &read));
    let proof = Job::capture(&shell, &read).unwrap().run().unwrap();
    assert!(proof.publish(&mut shell));
    assert!(confirm_readback(&mut shell, &attempt, &read));
    let certified = presentation(&shell).unwrap().snapshot.unwrap();
    assert!(ReplacementIntent::main_canvas(&certified, &id, 100).is_ok());
    assert_eq!(certified.sections, raw.sections);
}
#[test]
fn held_pair_certificate_cannot_publish_after_source_read_owner_or_dispatch_change() {
    for change in 0..8 {
        let (mut shell, attempt, read, raw, _) = setup();
        let proof = Job::capture(&shell, &read).unwrap().run().unwrap();
        match change {
            0 => {
                begin_read(&mut shell).unwrap();
            }
            1 => {
                shell.canvas_checklist.view.as_mut().unwrap().snapshot =
                    Some(Arc::new((*raw).clone()))
            }
            2 => {
                invalidate(&mut shell, "T1", "Ffixture");
            }
            3 => {
                open_visit(&mut shell);
            }
            4 => shell.core.account_epoch += 1,
            5 => shell.core.active_account = Some("other".into()),
            6 => shell.canvas_checklist.operations[0].attempt.serial += 1,
            7 => {
                shell.canvas_checklist.operations[0]
                    .dispatched
                    .as_mut()
                    .unwrap()
                    .1 += 1
            }
            _ => unreachable!(),
        }
        assert!(!proof.publish(&mut shell), "change {change}");
        assert!(!confirm_readback(&mut shell, &attempt, &read));
    }
}

#[test]
fn durable_worker_refuses_removed_replaced_or_foreign_ticket() {
    for change in 0..3 {
        let (mut shell, attempt, read, _, _) = setup_mode(true);
        let proof = Job::capture(&shell, &read).unwrap().run().unwrap();
        let old = shell.canvas_checklist.operations[0]
            .durable
            .clone()
            .unwrap();
        shell.canvas_checklist.operations[0].durable = match change {
            0 => None,
            1 => Some(recovery::confirmed::tests::ticket(
                old.receipt().checkpoint().clone(),
            )),
            _ => Some(recovery::confirmed::tests::foreign(old)),
        };
        assert!(!proof.publish(&mut shell), "ticket change {change}");
        assert!(!confirm_readback(&mut shell, &attempt, &read));
    }
}
#[test]
fn durable_receipt_removal_is_accepted_only_after_explicit_matching_retirement() {
    let (mut shell, attempt, read, _, _) = setup_mode(true);
    let proof = Job::capture(&shell, &read).unwrap().run().unwrap();
    assert!(proof.confirmed.is_some());
    assert!(proof.publish(&mut shell));
    assert!(!confirm_readback(&mut shell, &attempt, &read));
    let ticket = shell.canvas_checklist.operations[0].durable.take().unwrap();
    assert!(!confirm_readback(&mut shell, &attempt, &read));
    shell.canvas_checklist.operations[0].durable = Some(ticket.clone());
    assert!(recovery::confirmed::mark_retired(
        &mut shell, &attempt, &ticket
    ));
    shell.canvas_checklist.operations[0].durable = None;
    assert!(confirm_readback(&mut shell, &attempt, &read));
}
