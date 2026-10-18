use super::*;
use std::{cell::Cell, rc::Rc};

type Slot = Rc<Cell<Option<Signal<ShellState>>>>;

fn mount() -> (VirtualDom, Signal<ShellState>) {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(super::super::tests::shell);
            slot.set(Some(state));
            VNode::empty()
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap())
}

// Exercise the actual retained controller book and settlement functions with
// prepared requests; only the offline transport is replaced by the fixture.
fn attempt(s: &mut ShellState, mode: fixture::Mode) -> (u64, u64, u64) {
    let editor = super::super::tests::editor(s);
    let title = "Révision de projet".to_owned();
    let description = "Keep the proposed details.\nReview 中🙂 e\u{301} together.".to_owned();
    let plan = MutationPlan::new(
        &editor,
        editor.metadata_command(&title, &description, None).unwrap(),
    )
    .unwrap();
    let surface = s.app_home.about.management.surface.as_ref().unwrap();
    let visit = surface.id;
    let owner = surface.owner.clone();
    let policy = surface.policy.clone().unwrap();
    let doc = next();
    let revision = next();
    let id = next();
    s.app_home.about.management.documents.push(Document {
        activity: activity::State::default(),
        id: doc,
        account: s.core.active_account.clone(),
        team: owner.ctx.session.team_id.clone(),
        user: owner.ctx.session.user_id.clone(),
        editor,
        title: title.clone(),
        description: description.clone(),
        revision,
        observed: None,
        pending: Some(id),
        notice: None,
    });
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .document = Some(doc);
    let lease = read::Lease::begin(s, visit).unwrap();
    s.app_home.about.management.attempts.push(Attempt {
        id,
        document: doc,
        lease,
        revision,
        title,
        description,
        policy,
        replies: (0..plan.slots().len()).map(|_| None).collect(),
        plan,
        finished: false,
        resolved: false,
        release: None,
    });
    fixture::set_mode(s, mode);
    for index in 0..2 {
        assert!(exact(s, id));
        let a = s.app_home.about.management.attempts.last_mut().unwrap();
        a.plan.mark_sent(index).unwrap();
        let request = a.plan.slots()[index]
            .plan()
            .request(&owner.ctx.client, &owner.ctx.session);
        let reply = fixture::mutation(s, &request);
        settle(s, id, index, reply);
    }
    finish(s, id, "Responses retained.");
    let observed = super::super::tests::editor(s);
    document_mut(s, visit).unwrap().observed = Some(observed);
    observe(s, visit);
    (visit, doc, id)
}

#[test]
fn controller_requires_acknowledged_slots_and_matching_readback_for_completion() {
    for mode in [
        fixture::Mode::Success,
        fixture::Mode::TriggerRejected,
        fixture::Mode::ReplyLoss,
    ] {
        let (_dom, mut state) = mount();
        let (visit, _, id) = attempt(&mut state.write(), mode);
        let s = state.peek();
        let d = document(&s, visit).unwrap();
        let a = s.app_home.about.management.attempts.last().unwrap();
        assert!(a.finished);
        assert_eq!(d.title, "Révision de projet");
        assert!(d.description.contains("中🙂 e\u{301}"));
        assert_eq!(fixture::snapshot(&s)["writes"], 2);
        if mode == fixture::Mode::Success {
            assert!(d.pending.is_none());
            assert!(!d.dirty());
            assert!(a.resolved);
        } else {
            assert_eq!(d.pending, Some(id));
            assert!(!a.resolved);
            assert!(!editable(&s, visit));
            assert!(d.observed.is_some());
            drop(s);
            save(state, visit); // An unresolved attempt cannot dispatch twice.
            assert_eq!(fixture::snapshot(&state.peek())["writes"], 2);
            assert_eq!(state.peek().app_home.about.management.attempts.len(), 1);
        }
    }
}

#[test]
fn actual_shell_replacement_retains_unknown_matching_draft_until_explicit_resolution() {
    let (_dom, mut state) = mount();
    let (old_visit, doc, id) = attempt(&mut state.write(), fixture::Mode::ReplyLoss);
    let original = document(&state.peek(), old_visit).unwrap().clone();
    let replacement = fixture::replace_account(state).unwrap();
    assert_eq!(replacement["foreignDocumentHidden"], true);
    assert_eq!(replacement["retainedPending"], 1);
    assert!(!exact(&state.peek(), id));
    assert!(document(&state.peek(), old_visit).is_none());
    assert_eq!(fixture::snapshot(&state.peek())["writes"], 2);

    // Reentering the original account creates a fresh owner. Its readback can
    // expose the retained document, but cannot turn unknown receipts into ACKs.
    crate::bootstrap::install_loaded(state, super::super::tests::shell());
    let visit = {
        let mut s = state.write();
        let h = s.app_home.about.management.surface.as_mut().unwrap();
        h.document = Some(doc);
        let visit = h.id;
        observe(&mut s, visit);
        visit
    };
    {
        let s = state.peek();
        let d = document(&s, visit).unwrap();
        assert_eq!(d.pending, Some(id));
        assert_eq!(d.title, original.title);
        assert_eq!(d.description, original.description);
        assert!(!editable(&s, visit));
        assert!(
            s.app_home.about.management.attempts[0]
                .plan
                .slots()
                .iter()
                .all(|slot| matches!(slot.outcome(), Outcome::Unknown))
        );
    }
    use_current(state, visit);
    let s = state.peek();
    let d = document(&s, visit).unwrap();
    assert!(d.pending.is_none());
    assert!(!d.dirty());
    let a = &s.app_home.about.management.attempts[0];
    assert!(a.resolved);
    assert!(
        a.plan
            .slots()
            .iter()
            .all(|slot| matches!(slot.outcome(), Outcome::Unknown))
    );
}
