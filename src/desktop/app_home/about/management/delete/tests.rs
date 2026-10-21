use super::*;
pub(super) fn prepared() -> (ShellState, u64, u64) {
    let mut s = super::super::tests::shell();
    let editor = super::super::tests::editor(&mut s);
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let visit = h.id;
    let doc = next();
    s.app_home.about.management.documents.push(Document {
        activity: activity::State::default(),
        id: doc,
        account: s.core.active_account.clone(),
        team: h.owner.ctx.session.team_id.clone(),
        user: h.owner.ctx.session.user_id.clone(),
        editor,
        title: "  Delete draft \u{4e2d} e\u{301}  ".into(),
        description: "Original opaque text\nSecond line \u{1f98a}".into(),
        revision: next(),
        observed: None,
        pending: None,
        notice: None,
    });
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .document = Some(doc);
    let original = Original::capture(&s, visit).unwrap();
    let id = original.id;
    let plan = MutationPlan::new(&original.editor, protocol::Command::Delete).unwrap();
    let lease = read::Lease::begin(&mut s, visit).unwrap();
    document_mut(&mut s, visit).unwrap().pending = Some(id);
    s.app_home
        .about
        .management
        .deletions
        .attempts
        .push(Attempt {
            original,
            lease,
            replies: (0..plan.slots().len()).map(|_| None).collect(),
            plan,
            finished: false,
            terminal: false,
            no_send: false,
            release: None,
        });
    (s, visit, id)
}
fn sent(s: &mut ShellState, index: usize) {
    s.app_home.about.management.deletions.attempts[0]
        .plan
        .mark_sent(index)
        .unwrap();
}
#[test]
fn only_complete_raw_ack_and_current_owner_make_terminal_without_erasing_draft() {
    for current in [false, true] {
        let (mut s, visit, id) = prepared();
        let before = document(&s, visit).unwrap().clone();
        for index in 0..2 {
            sent(&mut s, index);
            run::settle(
                &mut s,
                id,
                index,
                Ok(json!({"ok":true,"opaque":{"slot":index}})),
            );
        }
        run::finish(&mut s, id, current, "Retained.");
        assert_eq!(terminal(&s, visit), current);
        let d = document(&s, visit).unwrap();
        assert_eq!(d.title, before.title);
        assert_eq!(d.description, before.description);
        assert!(d.editor.same_revision(&before.editor));
        assert_eq!(d.pending, Some(id));
        assert!(!editable(&s, visit));
        assert_eq!(
            snapshot(&s, visit)["attempts"][0]["slots"][1]["raw"]["opaque"]["slot"],
            1
        );
        write::observe(&mut s, visit);
        assert_eq!(document(&s, visit).unwrap().pending, Some(id));
    }
}
#[test]
fn failed_first_slot_and_ambiguous_ack_never_authorize_second_or_replace_receipt() {
    for raw in [
        json!({"ok":false,"error":"denied"}),
        json!({"ok":true,"error":"contradictory"}),
        json!([{"ok":true}]),
        json!({"ok":"true"}),
    ] {
        let (mut s, visit, id) = prepared();
        sent(&mut s, 0);
        run::settle(&mut s, id, 0, Ok(raw.clone()));
        assert!(
            s.app_home.about.management.deletions.attempts[0]
                .plan
                .ready()
                .is_empty()
        );
        run::settle(&mut s, id, 0, Ok(json!({"ok":true})));
        run::finish(&mut s, id, true, "Retained.");
        assert!(!terminal(&s, visit));
        assert!(owns(&s, visit));
        assert_eq!(snapshot(&s, visit)["attempts"][0]["slots"][0]["raw"], raw);
    }
}
#[test]
fn partial_lost_ack_retains_two_independent_slots_and_cannot_be_resolved_by_metadata_use_current() {
    let (mut s, visit, id) = prepared();
    let original = document(&s, visit).unwrap().clone();
    sent(&mut s, 0);
    run::settle(&mut s, id, 0, Ok(json!({"ok":true})));
    sent(&mut s, 1);
    run::settle(
        &mut s,
        id,
        1,
        Err(Error::Transport("lost fixture ACK".into())),
    );
    run::finish(&mut s, id, true, "Retained.");
    document_mut(&mut s, visit).unwrap().observed = Some(original.editor.clone());
    write::observe(&mut s, visit);
    assert!(owns(&s, visit));
    assert!(!terminal(&s, visit));
    assert!(!editable(&s, visit));
    assert_eq!(
        snapshot(&s, visit)["attempts"][0]["slots"][0]["outcome"],
        "acknowledged"
    );
    assert_eq!(
        snapshot(&s, visit)["attempts"][0]["slots"][1]["outcome"],
        "unknown"
    );
    assert_eq!(document(&s, visit).unwrap().title, original.title);
}
#[test]
fn no_send_can_release_pending_but_retired_sent_attempt_cannot_send_remaining_slot() {
    let (mut s, visit, id) = prepared();
    let before = document(&s, visit).unwrap().clone();
    run::finish(&mut s, id, false, "No deletion sent.");
    assert!(!owns(&s, visit));
    assert!(editable(&s, visit));
    assert_eq!(document(&s, visit).unwrap().title, before.title);
    let (mut s, visit, id) = prepared();
    sent(&mut s, 0);
    run::settle(&mut s, id, 0, Ok(json!({"ok":true})));
    s.app_home.about.management.surface.as_mut().unwrap().id = next();
    assert!(!exact(&s, id));
    run::finish(&mut s, id, false, "Retired.");
    assert_eq!(
        snapshot(&s, visit)["attempts"][0]["slots"][1]["outcome"],
        "pending"
    );
    assert!(!s.app_home.about.management.deletions.attempts[0].terminal);
}
#[test]
fn original_document_value_revision_owner_and_policy_are_exact() {
    let (s, visit, id) = prepared();
    assert!(exact(&s, id));
    let original = s.app_home.about.management.deletions.attempts[0]
        .original
        .clone();
    let mut s = s;
    document_mut(&mut s, visit).unwrap().description.push('x');
    assert!(!original.current(&s, Some(id)));
    document_mut(&mut s, visit).unwrap().description = original.description.clone();
    assert!(exact(&s, id));
    s.core.account_epoch += 1;
    assert!(!exact(&s, id));
}

#[test]
fn metadata_use_current_cannot_unseal_a_delete_attempt() {
    use std::{cell::Cell, rc::Rc};
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| prepared().0);
            slot.set(Some(state));
            VNode::empty()
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    let mut state = slot.get().unwrap();
    let (visit, id) = {
        let s = state.peek();
        let a = &s.app_home.about.management.deletions.attempts[0];
        (a.original.visit, a.original.id)
    };
    {
        let mut s = state.write();
        sent(&mut s, 0);
        run::settle(&mut s, id, 0, Ok(json!({"ok":false,"error":"denied"})));
        run::finish(&mut s, id, true, "Retained.");
        let d = document_mut(&mut s, visit).unwrap();
        d.observed = Some(d.editor.clone());
    }
    write::use_current(state, visit);
    assert_eq!(document(&state.peek(), visit).unwrap().pending, Some(id));
    assert!(owns(&state.peek(), visit));
}
#[test]
fn confirmation_blocks_existing_edit_read_save_and_activity_admission() {
    let (mut s, visit, id) = prepared();
    run::finish(&mut s, id, false, "No send.");
    let original = Original::capture(&s, visit).unwrap();
    let (release, _receiver) = crate::app_home::inputs::native::test_pair();
    s.app_home.about.management.deletions.confirmation = Some(Confirmation { original, release });
    assert!(confirming(&s, visit));
    assert!(!editable(&s, visit));
    assert!(!activity::permitted(&s, visit));
    assert!(read::Lease::begin(&mut s, visit).is_none());
}
