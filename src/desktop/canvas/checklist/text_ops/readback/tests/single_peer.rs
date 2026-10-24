//! One associated row must use the same sealed worker custody as a sequence.
use super::*;
use crate::canvas::{checklist::session_worker, text::owner_delete_fixture};

fn preparing() -> (ShellState, Attempt, Arc<CanvasCollabSnapshot>, TextIntent) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let source = owner_delete_fixture::test_source();
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(source.clone())));
    let intent = TextIntent::capture(&source, "paragraph", 100).unwrap();
    assert_eq!(intent.associated_peer_count(), 1);
    let view = presentation(&shell).unwrap();
    let attempt = begin_text(
        &mut shell,
        view.visit,
        view.read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Owner independently edited").unwrap()),
    )
    .unwrap();
    (shell, attempt, source, intent)
}

fn awaiting_read() -> Context {
    let (mut shell, attempt, source, intent) = preparing();
    let mut lease = session_worker::acquire(&mut shell, &attempt).unwrap();
    assert!(session_worker::acquire(&mut shell, &attempt).is_err());
    let claim = lease.claim();
    let prepared = lease
        .prepare(&intent, &source, draft(&attempt).unwrap(), 100)
        .unwrap();
    assert!(claim.preparing(&shell, &attempt));
    assert!(mark_dispatched(&mut shell, &attempt, &prepared));
    let (accepted, replacement, fresh) = acknowledge(&prepared, &source);
    assert_eq!(accepted.associated_peer_count(), 1);
    assert!(
        source
            .sections
            .iter()
            .filter(|s| s.id() != "paragraph")
            .all(|s| fresh.section(s.id()) == Some(s))
    );
    let memo = lease.remember(accepted);
    assert!(lease.restore(&mut shell));
    assert!(finish_text_with_memo(&mut shell, &attempt, memo));
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

#[test]
fn one_peer_session_ack_requires_executed_readback_proof() {
    let mut c = awaiting_read();
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &c.read));
    assert!(prove(&mut c.shell, &c.read));
    assert!(confirm_readback(&mut c.shell, &c.attempt, &c.read));
    assert!(c.shell.canvas_checklist.operations.is_empty());
}

#[test]
fn held_one_peer_proof_refuses_replaced_authority_and_dispatch() {
    for change in 0..7 {
        let mut c = awaiting_read();
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
                c.shell.core.account_epoch += 1;
            }
            3 => {
                open_visit(&mut c.shell);
            }
            4 => {
                c.shell.canvas_checklist.operations[0].accepted =
                    Some(Accepted::TextSequence(AcceptedState::new(c.replacement)));
            }
            5 => {
                c.shell.canvas_checklist.operations[0]
                    .dispatched
                    .as_mut()
                    .unwrap()
                    .1 += 1;
            }
            6 => {
                let Mutation::Text(value) =
                    &mut c.shell.canvas_checklist.operations[0].attempt.mutation
                else {
                    unreachable!()
                };
                *value = Arc::new((**value).clone());
            }
            _ => unreachable!(),
        }
        assert!(!proof.publish(&mut c.shell), "changed authority {change}");
        assert!(!confirm_readback(&mut c.shell, &c.attempt, &c.read));
        assert_eq!(c.shell.canvas_checklist.operations.len(), 1);
    }
}

#[test]
fn one_peer_missing_child_cannot_unlock_and_consumed_actor_counter_never_rolls_back() {
    let mut c = awaiting_read();
    let peer = c
        .fresh
        .sections
        .iter()
        .find(|s| !matches!(s.id(), "title" | "paragraph" | "neighbor"))
        .unwrap()
        .id()
        .to_owned();
    let mut missing = (*c.fresh).clone();
    missing.sections.retain(|s| s.id() != peer);
    let read = begin_read(&mut c.shell).unwrap();
    assert!(finish_read(&mut c.shell, &read, Ok(Arc::new(missing))));
    assert!(!prove(&mut c.shell, &read));
    assert!(!confirm_readback(&mut c.shell, &c.attempt, &read));

    let (mut shell, attempt, source, intent) = preparing();
    let mut lease = session_worker::acquire(&mut shell, &attempt).unwrap();
    let first = lease
        .prepare(&intent, &source, draft(&attempt).unwrap(), 100)
        .unwrap();
    let actor = first.session_id().to_owned();
    let first_sequence = first.sequence();
    let claim = lease.claim();
    assert!(cancel_preflight(&mut shell, &attempt));
    assert!(!claim.preparing(&shell, &attempt));
    assert!(lease.restore(&mut shell));
    let view = presentation(&shell).unwrap();
    let next = begin_text(
        &mut shell,
        view.visit,
        view.read,
        "paragraph",
        Arc::new(TextDraft::from_rtml("Next owner edit").unwrap()),
    )
    .unwrap();
    let mut lease = session_worker::acquire(&mut shell, &next).unwrap();
    let second = lease
        .prepare(&intent, &source, draft(&next).unwrap(), 100)
        .unwrap();
    assert_eq!(second.session_id(), actor);
    assert_eq!(second.sequence(), first_sequence + 1);
    assert!(lease.restore(&mut shell));
}
