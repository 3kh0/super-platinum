use super::*;
use serde_json::json;
pub(super) fn setup() -> (ShellState, u64) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s, "app-calendar");
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    d.fixture = false;
    let id = d.id;
    let c = d.calendar.as_mut().unwrap();
    c.fixture = false;
    c.share = false;
    c.guests.clear();
    (s, id)
}
#[test]
fn calendar_cancel_and_late_old_cancel_preserve_newer_composer_and_dialog() {
    let (mut s, id) = setup();
    s.core.composer.text = "newer channel input".into();
    s.core.thread_composer.text = "thread input".into();
    close(&mut s, id, false);
    assert!(active(&s, id).unwrap().confirm_close);
    close(&mut s, id, true);
    assert!(s.message_buttons.shortcuts.dialog.is_none());
    fixture::install(&mut s, "app-calendar-modern");
    let newer = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    close(&mut s, id, true);
    assert_eq!(
        s.message_buttons.shortcuts.dialog.as_ref().unwrap().id,
        newer
    );
    assert_eq!(s.core.composer.text, "newer channel input");
    assert_eq!(s.core.thread_composer.text, "thread input");
}
#[test]
fn exact_owner_and_current_attendee_metadata_are_rechecked_at_capture() {
    for change in 0..4 {
        let (mut s, id) = setup();
        assert!(actions::event(&s, id).is_some());
        match change {
            0 => s.channel_generation += 1,
            1 => s.core.account_epoch += 1,
            2 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token
                .push('x'),
            _ => {
                let c = active_mut(&mut s, id).unwrap();
                c.slot_loading = true;
            }
        }
        assert!(actions::event(&s, id).is_none());
    }
    let (mut s, id) = setup();
    let team = s.core.active_team.clone().unwrap();
    let u = serde_json::from_value(
        json!({"id":"UCAL00001","name":"Guest","deleted":false,"is_bot":false}),
    )
    .unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .insert("UCAL00001".into(), u);
    active_mut(&mut s, id).unwrap().guests.push(Person {
        id: "UCAL00001".into(),
        label: "Guest".into(),
    });
    assert!(actions::event(&s, id).is_some());
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("UCAL00001")
        .unwrap()
        .deleted = true;
    assert!(actions::event(&s, id).is_none());
}
#[test]
fn sharing_uses_existing_dm_and_never_invents_thread_context() {
    let (mut s, id) = setup();
    let team = s.core.active_team.clone().unwrap();
    let w = s.core.workspaces.get_mut(&team).unwrap();
    w.users.insert(
        "UCAL00001".into(),
        serde_json::from_value(json!({"id":"UCAL00001","name":"Guest"})).unwrap(),
    );
    w.channels.insert(
        "D_CAL".into(),
        serde_json::from_value(json!({"id":"D_CAL","is_im":true,"user":"UCAL00001"})).unwrap(),
    );
    let c = active_mut(&mut s, id).unwrap();
    c.share = true;
    c.share_channel = "UCAL00001".into();
    let event = actions::event(&s, id).unwrap();
    assert_eq!(event.share_channel, "D_CAL");
    assert!(event.share_thread.is_empty());
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .remove("D_CAL");
    let event = actions::event(&s, id).unwrap();
    assert!(event.share_channel.is_empty());
    assert!(event.share_thread.is_empty());
}
#[test]
fn static_calendar_surfaces_remain_read_only_across_all_named_variants() {
    for variant in [
        "app-calendar",
        "app-calendar-modern",
        "app-calendar-connect",
        "app-calendar-uncertain",
    ] {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut s, variant);
        let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
        assert!(current(&s, id));
        assert!(!editable(&s, id));
        assert!(actions::event(&s, id).is_none());
        assert!(s.message_buttons.shortcuts.calendar_attempts.is_empty());
    }
}
