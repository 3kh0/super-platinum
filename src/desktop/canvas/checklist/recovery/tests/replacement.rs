use super::*;
use crate::canvas::text::replacement_fixture;

#[test]
fn paired_readback_stays_locked_until_exact_durable_retirement_and_rechecks_latest_read() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Pair".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    open_visit(&mut shell);
    let snapshot = replacement_fixture::snapshot();
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(snapshot.clone())));
    let view = presentation(&shell).unwrap();
    let (attempt, intent) =
        replacement_ops::begin_replacement(&mut shell, view.visit, view.read, "paragraph", 100)
            .unwrap();
    let prepared = intent.prepare(&snapshot, 100).unwrap();
    let store = Store::new();
    let owner = Owner {
        account: "00000000-0000-4000-8000-000000009997".into(),
        scope: prepared.checkpoint().unwrap().scope().clone(),
        path: Some(store.0.clone()),
    };
    let mut journal = Journal::open(&store.0, &owner.account).unwrap();
    let Begin::Started(receipt) = journal.begin(prepared.checkpoint().unwrap()).unwrap() else {
        panic!("fresh")
    };
    shell.canvas_checklist.operations[0].durable = Some(Ticket {
        owner,
        receipt: receipt.clone(),
    });
    assert!(replacement_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (accepted, current) = replacement_fixture::accepted(prepared, &snapshot, 100);
    assert!(replacement_ops::finish_replacement(
        &mut shell, &attempt, accepted
    ));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(current.clone())));
    assert!(!confirm_readback(&mut shell, &attempt, &read));
    finish_retirement(&mut shell, &attempt, false);
    assert!(shell.canvas_checklist.operations[0].durable.is_some());
    assert!(journal.get(receipt.checkpoint().scope()).unwrap().as_ref() == Some(&receipt));
    let newer = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &newer, Ok(current)));
    assert!(!confirm_readback(&mut shell, &attempt, &newer));
    assert!(journal.complete(&receipt).unwrap());
    finish_retirement(&mut shell, &attempt, true);
    assert!(text_ops::settled(&shell, &attempt));
    assert!(shell.canvas_checklist.operations.is_empty());
    assert!(journal.get(receipt.checkpoint().scope()).unwrap().is_none());
    drop(journal);
}
