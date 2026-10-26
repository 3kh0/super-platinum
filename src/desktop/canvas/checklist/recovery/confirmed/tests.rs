use super::*;
use crate::canvas::text::replacement_fixture;
use super_platinum_core::canvas::edit::structure::recovery::Checkpoint;

pub(in crate::canvas::checklist) fn ticket(checkpoint: Checkpoint) -> Ticket {
    let store = super::super::tests::Store::new();
    let owner = Owner {
        account: "00000000-0000-4000-8000-000000009997".into(),
        scope: checkpoint.scope().clone(),
        path: Some(store.0.clone()),
    };
    let mut journal = Journal::open(&store.0, &owner.account).unwrap();
    let Begin::Started(receipt) = journal.begin(checkpoint).unwrap() else {
        panic!("fresh")
    };
    Ticket {
        owner,
        receipt: Arc::new(receipt),
    }
}
pub(in crate::canvas::checklist) fn foreign(mut ticket: Ticket) -> Ticket {
    ticket.owner.account = "00000000-0000-4000-8000-000000009998".into();
    ticket
}
fn setup(previous: bool) -> (ShellState, Read, RestoreRead) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Restore".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    if previous {
        let read = begin_read(&mut shell).unwrap();
        assert!(finish_read(
            &mut shell,
            &read,
            Ok(replacement_fixture::snapshot())
        ));
    }
    let read = begin_read(&mut shell).unwrap();
    let owner = Owner::capture(&shell, &read.file).unwrap();
    shell.canvas_checklist.recovery = State {
        owner: Some(owner),
        loaded: true,
        ..Default::default()
    };
    let captured = RestoreRead::capture(&shell, &read).unwrap();
    (shell, read, captured)
}
#[test]
fn restore_fence_accepts_both_absent_and_exact_prior_raw_snapshot() {
    for previous in [false, true] {
        let (shell, _, captured) = setup(previous);
        assert_eq!(captured.previous.is_some(), previous);
        assert!(captured.current(&shell));
    }
}
#[test]
fn held_restore_refuses_owner_generation_read_revision_and_snapshot_replacement() {
    for change in 0..7 {
        let (mut shell, _, captured) = setup(true);
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("other".into()),
            2 => shell.canvas_checklist.recovery.generation += 1,
            3 => shell.canvas_checklist.view.as_mut().unwrap().revision += 1,
            4 => {
                shell.canvas_checklist.view.as_mut().unwrap().snapshot =
                    Some(replacement_fixture::snapshot())
            }
            5 => {
                open_visit(&mut shell);
            }
            _ => shell.canvas_checklist.recovery.receipt = None,
        }
        if change == 6 {
            // A new reservation changes generation even for the same file.
            shell.canvas_checklist.recovery.generation += 1;
        }
        assert!(!captured.current(&shell), "change {change}");
    }
}
