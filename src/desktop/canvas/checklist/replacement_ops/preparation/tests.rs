use super::*;
use crate::canvas::text::replacement_fixture;
fn setup() -> (ShellState, Job) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Worker".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(
        &mut shell,
        &read,
        Ok(replacement_fixture::snapshot())
    ));
    let view = presentation(&shell).unwrap();
    let job = begin(&mut shell, view.visit, view.read, "paragraph", 100).unwrap();
    (shell, job)
}
#[test]
fn immediate_claim_blocks_duplicate_save_and_cannot_dispatch_until_worker_publication() {
    let (mut shell, job) = setup();
    let claimed = job.attempt().clone();
    assert!(current(&shell, &claimed));
    assert!(!dispatchable(&shell, &claimed));
    assert!(replacement(&claimed).is_none());
    assert!(
        begin(
            &mut shell,
            claimed.visit,
            claimed.read,
            &claimed.section,
            100
        )
        .is_err()
    );
    let ready = job.run().unwrap();
    let (published, intent) = ready.publish(&mut shell).unwrap();
    assert_eq!(published.serial, claimed.serial);
    assert!(Arc::ptr_eq(&published.snapshot, &claimed.snapshot));
    assert!(dispatchable(&shell, &published));
    assert_eq!(replacement(&published), Some(intent.replacement_section()));
    assert!(!current(&shell, &claimed));
    let prepared = intent.prepare(&claimed.snapshot, 100).unwrap();
    assert!(!mark_dispatched(&mut shell, &claimed, &prepared));
    assert!(mark_dispatched(&mut shell, &published, &prepared));
    assert!(!mark_dispatched(&mut shell, &published, &prepared));
}
fn change(shell: &mut ShellState, attempt: &Attempt, which: usize) {
    match which {
        0 => {
            cancel_preflight(shell, attempt);
        }
        1 => {
            begin_read(shell).unwrap();
        }
        2 => {
            open_visit(shell);
        }
        3 => {
            invalidate(shell, "T1", "Ffixture");
        }
        4 => shell.core.account_epoch += 1,
        5 => {
            shell.canvas_checklist.view.as_mut().unwrap().snapshot =
                Some(Arc::new((*attempt.snapshot).clone()))
        }
        6 => shell.canvas_checklist.operations[0].attempt.section = "different".into(),
        7 => shell.canvas_checklist.operations[0].dispatched = Some(("unexpected".into(), 1)),
        8 => shell.canvas_checklist.operations[0].stage = Stage::Uncertain,
        9 => shell.canvas_checklist.operations[0].attempt.revision += 1,
        _ => unreachable!(),
    }
}
#[test]
fn captured_intent_worker_refuses_cancel_retirement_or_changed_exact_attempt() {
    for which in 0..10 {
        let (mut shell, job) = setup();
        let attempt = job.attempt().clone();
        let ready = job.run().unwrap();
        change(&mut shell, &attempt, which);
        assert!(ready.publish(&mut shell).is_err(), "change {which}");
    }
}
#[test]
fn checkpoint_return_requires_same_live_preparation_before_journal_reservation() {
    for which in 0..10 {
        let (mut shell, job) = setup();
        let (attempt, intent) = job.run().unwrap().publish(&mut shell).unwrap();
        let prepared = intent.prepare(&attempt.snapshot, 100).unwrap();
        assert!(recovery::replacement_preparation_current(
            &shell, &attempt, &prepared
        ));
        let checkpoint = prepared.checkpoint().unwrap();
        assert!(checkpoint.is_replacement());
        change(&mut shell, &attempt, which);
        assert!(
            !recovery::replacement_preparation_current(&shell, &attempt, &prepared),
            "change {which}"
        );
        assert!(
            shell
                .canvas_checklist
                .operations
                .iter()
                .all(|op| op.durable.is_none())
        );
    }
}
