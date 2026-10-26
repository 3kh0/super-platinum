use super::*;
use crate::canvas::checklist::end_ops::readback::tests::{Context, prove};

fn setup_end() -> (Context, Ticket, Store) {
    let mut context = crate::canvas::checklist::end_ops::readback::tests::setup();
    let store = Store::new();
    let owner = Owner {
        account: "d1b586dc-c6d3-4615-8316-a60de869a5e8".into(),
        scope: context.checkpoint.scope().clone(),
        path: Some(store.0.clone()),
    };
    let mut journal = Journal::open(&store.0, &owner.account).unwrap();
    let Begin::Started(receipt) = journal.begin(context.checkpoint.clone()).unwrap() else {
        panic!("duplicate isolated End receipt")
    };
    let ticket = Ticket {
        owner,
        receipt: Arc::new(receipt),
    };
    context.shell.canvas_checklist.operations[0].durable = Some(ticket.clone());
    (context, ticket, store)
}

fn remove(ticket: &Ticket) {
    let mut journal =
        Journal::open(ticket.owner.path.as_ref().unwrap(), &ticket.owner.account).unwrap();
    assert!(journal.complete(&ticket.receipt).unwrap());
}

#[test]
fn end_worker_proof_waits_for_successful_durable_retirement() {
    let (mut c, ticket, _store) = setup_end();
    assert!(prove(&mut c.shell, &c.read));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &c.read));
    assert_eq!(
        c.shell.canvas_checklist.operations[0].durable_verified,
        Some(c.read.serial)
    );
    finish_retirement(&mut c.shell, &c.attempt, false);
    assert!(c.shell.canvas_checklist.operations[0].durable.is_some());
    assert_eq!(
        c.shell.canvas_checklist.view.as_ref().unwrap().settled_text,
        None
    );
    remove(&ticket);
    finish_retirement(&mut c.shell, &c.attempt, true);
    assert!(c.shell.canvas_checklist.operations.is_empty());
    assert_eq!(
        c.shell.canvas_checklist.view.as_ref().unwrap().settled_text,
        Some(c.attempt.serial)
    );
}

#[test]
fn end_retirement_cannot_reuse_proof_for_an_unverified_newer_read() {
    let (mut c, ticket, _store) = setup_end();
    assert!(prove(&mut c.shell, &c.read));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &c.read));
    let newer = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &newer, Ok(c.fresh.clone())));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &newer));
    remove(&ticket);
    finish_retirement(&mut c.shell, &c.attempt, true);
    assert_eq!(c.shell.canvas_checklist.operations.len(), 1);
    assert_eq!(
        c.shell.canvas_checklist.view.as_ref().unwrap().settled_text,
        None
    );
    assert!(prove(&mut c.shell, &newer));
    assert!(confirm_readback(&mut c.shell, &c.attempt, &newer));
}

#[test]
fn end_retirement_reuses_the_latest_independently_verified_read() {
    let (mut c, ticket, _store) = setup_end();
    assert!(prove(&mut c.shell, &c.read));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &c.read));
    let newer = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &newer, Ok(c.fresh.clone())));
    assert!(prove(&mut c.shell, &newer));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &newer));
    assert_eq!(
        c.shell.canvas_checklist.operations[0].durable_verified,
        Some(newer.serial)
    );
    remove(&ticket);
    finish_retirement(&mut c.shell, &c.attempt, true);
    assert!(c.shell.canvas_checklist.operations.is_empty());
}
