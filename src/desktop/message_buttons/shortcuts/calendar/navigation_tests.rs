use super::*;

#[test]
fn actual_created_calendar_about_click_queues_owned_lookup_without_replaying_event() {
    use std::{cell::Cell, rc::Rc};
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    for retired in [false, true] {
        let slot = Slot::default();
        let mut dom = VirtualDom::new_with_props(
            |slot: Slot| {
                let state = use_signal(|| {
                    let (mut s, id, _, _) = prepared();
                    active_mut(&mut s, id).unwrap().app_link =
                        Some("slack://app?id=A_CALENDAR&team=T1&tab=about".into());
                    s
                });
                slot.set(Some(state));
                rsx! { div {} }
            },
            slot.clone(),
        );
        dom.rebuild_in_place();
        let mut state = slot.get().unwrap();
        let id = state
            .peek()
            .message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .id;
        if retired {
            dom.in_runtime(|| state.write().channel_generation += 1);
        }
        // Run the actual click callback but do not poll the queued async lookup.
        // No network/native claim. The existing navigation tests separately
        // prove the returned real bot adopts Tab::About with disabled Home.
        dom.in_runtime(|| app_home(state, id));
        let s = state.peek();
        assert_eq!(
            crate::app_home::navigation::pending_id(&s).is_some(),
            !retired
        );
        assert_eq!(s.message_buttons.shortcuts.calendar_attempts.len(), 1);
        assert!(matches!(
            s.message_buttons.shortcuts.calendar_attempts[0].result,
            Settlement::Created(_)
        ));
        assert_eq!(s.message_buttons.shortcuts.dialog.as_ref().unwrap().id, id);
        if !retired {
            assert!(
                active(&s, id).unwrap().notice.as_deref()
                    != Some("This app's About page is not available here yet.")
            );
        }
    }
}

fn prepared() -> (ShellState, u64, u64, String) {
    let (mut s, id) = super::super::tests::setup();
    let target = s
        .channels
        .iter()
        .find(|c| Some(&c.id) != s.core.active_channel.as_ref())
        .unwrap()
        .id
        .clone();
    let mut event = actions::event(&s, id).unwrap();
    event.share_channel = target.clone();
    event.share_thread.clear();
    let ctx = context(&s, id).unwrap();
    let attempt = 73;
    let c = active_mut(&mut s, id).unwrap();
    c.stage = Stage::Created;
    c.attempt = Some(attempt);
    let provider = c.provider;
    let account = c.account.clone();
    s.message_buttons.shortcuts.calendar_attempts.push(Attempt {
        id: attempt,
        epoch: ctx.epoch,
        team: ctx.session.team_id.clone(),
        provider,
        account,
        write: Write::Event(event),
        result: Settlement::Created(None),
    });
    (s, id, attempt, target)
}
#[test]
fn confirmed_shared_event_opens_actual_channel_and_closes_only_its_form() {
    let (mut s, id, attempt, target) = prepared();
    let prior = s.core.active_channel.clone().unwrap();
    s.core.composer.text = "unsent original channel".into();
    assert!(shared(&mut s, id, attempt));
    assert_eq!(s.core.active_channel.as_deref(), Some(target.as_str()));
    assert!(s.message_buttons.shortcuts.dialog.is_none());
    assert_ne!(s.overlay, Some(Overlay::AppShortcuts));
    assert!(matches!(
        s.message_buttons.shortcuts.calendar_attempts[0].result,
        Settlement::Created(_)
    ));
    assert!(actions::event(&s, id).is_none());
    let index = s.channels.iter().position(|c| c.id == prior).unwrap();
    s.select_channel(index, crate::state::ChannelOpen::Global);
    assert_eq!(s.core.composer.text, "unsent original channel");
}
#[test]
fn uncertainty_wrong_attempt_or_retired_owner_cannot_navigate_or_close() {
    for change in 1..5 {
        let (mut s, id, attempt, _) = prepared();
        let before = s.core.active_channel.clone();
        match change {
            1 => s.message_buttons.shortcuts.calendar_attempts[0].result = Settlement::Unknown,
            2 => active_mut(&mut s, id).unwrap().attempt = Some(attempt + 1),
            3 => s.channel_generation += 1,
            _ => active_mut(&mut s, id).unwrap().stage = Stage::Uncertain,
        }
        assert!(!shared(&mut s, id, attempt));
        assert_eq!(s.core.active_channel, before);
        assert_eq!(s.message_buttons.shortcuts.dialog.as_ref().unwrap().id, id);
    }
}

#[test]
fn confirmed_thread_share_closes_without_navigation_and_unshared_keeps_confirmation() {
    for in_thread in [false, true] {
        let (mut s, id, attempt, _) = prepared();
        let before = s.core.active_channel.clone();
        if let Write::Event(e) = &mut s.message_buttons.shortcuts.calendar_attempts[0].write {
            if in_thread {
                e.share_thread = "123.456".into();
            } else {
                e.share_channel.clear();
            }
        }
        assert!(!shared(&mut s, id, attempt));
        assert_eq!(s.core.active_channel, before);
        assert_eq!(s.message_buttons.shortcuts.dialog.is_none(), in_thread);
        if !in_thread {
            assert!(current(&s, id));
        }
        assert!(matches!(
            s.message_buttons.shortcuts.calendar_attempts[0].result,
            Settlement::Created(_)
        ));
    }
}
#[test]
fn missing_channel_metadata_does_not_guess_a_route_or_keep_a_shared_confirmation() {
    let (mut s, id, attempt, _) = prepared();
    let before = s.core.active_channel.clone();
    if let Write::Event(e) = &mut s.message_buttons.shortcuts.calendar_attempts[0].write {
        e.share_channel = "C_MISSING_METADATA".into();
    }
    assert!(!shared(&mut s, id, attempt));
    assert_eq!(s.core.active_channel, before);
    assert!(s.message_buttons.shortcuts.dialog.is_none());
    assert!(matches!(
        s.message_buttons.shortcuts.calendar_attempts[0].result,
        Settlement::Created(_)
    ));
}
