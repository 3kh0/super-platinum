use super::*;
use serde_json::json;

fn setup() -> (ShellState, Target) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install(&mut s);
    s.app_home.surface.as_mut().unwrap().fixture = false;
    inputs::hydrate(&mut s);
    let team = s.core.active_team.clone().unwrap();
    let target = Target::new("A_CALENDAR".into(), team, Tab::Home).unwrap();
    (s, target)
}
fn response(target: &Target) -> (Home, Channel, User) {
    let home = Home {
        channel: "D_RETURNED".into(),
        app: target.app.clone(),
        installed_team: target.installed_team.clone(),
        enabled: true,
        messages: true,
        view_id: None,
    };
    let channel =
        serde_json::from_value(json!({"id":"D_RETURNED","is_im":true,"user":"U_CALENDAR"}))
            .unwrap();
    let user =
        serde_json::from_value(json!({"id":"U_CALENDAR","name":"Calendar","is_bot":true})).unwrap();
    (home, channel, user)
}
#[test]
fn returned_bot_conversation_opens_global_home_and_preserves_outgoing_draft() {
    let (mut s, target) = setup();
    let previous = s.core.active_channel.clone().unwrap();
    s.core.composer.text = "unsent original draft".into();
    let pending = Pending::begin(&mut s, target.clone()).unwrap();
    assert!(Pending::begin(&mut s, target.clone()).is_none());
    let (home, channel, user) = response(&target);
    assert!(pending.adopt(&mut s, &home, Tab::Home, channel, user));
    assert_eq!(s.core.active_channel.as_deref(), Some("D_RETURNED"));
    assert_eq!(s.main_view, MainView::Home);
    assert_eq!(s.channel_tab, ChannelTab::AppHome);
    assert!(s.app_home.navigation.is_none());
    let index = s.channels.iter().position(|c| c.id == previous).unwrap();
    s.select_channel(index, crate::state::ChannelOpen::Global);
    assert_eq!(s.core.composer.text, "unsent original draft");
}
#[test]
fn exact_visit_view_account_and_cancel_guards_refuse_late_navigation_without_cache_adoption() {
    for change in 0..8 {
        let (mut s, target) = setup();
        let pending = Pending::begin(&mut s, target.clone()).unwrap();
        match change {
            0 => s.channel_generation += 1,
            1 => s.thread_generation += 1,
            2 => s.core.account_epoch += 1,
            3 => s.app_home.surface.as_mut().unwrap().revision += 1,
            4 => s.overlay = Some(Overlay::Palette),
            5 => cancel(&mut s, pending.id),
            6 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&target.installed_team)
                .unwrap()
                .token
                .push('x'),
            _ => {
                s.core
                    .workspaces
                    .get_mut(&target.installed_team)
                    .unwrap()
                    .rt_generation += 1
            }
        }
        let (home, channel, user) = response(&target);
        assert!(!pending.adopt(&mut s, &home, Tab::Home, channel, user));
        let w = &s.core.workspaces[&target.installed_team];
        assert!(!w.channels.contains_key("D_RETURNED"));
        assert!(!w.users.contains_key("U_CALENDAR"));
    }
}
#[test]
fn old_cleanup_and_cancel_cannot_clear_new_navigation_and_retired_429_keeps_account_deadline() {
    let (mut s, target) = setup();
    let old = Pending::begin(&mut s, target.clone()).unwrap();
    let mut other = target.clone();
    other.app = "A_OTHER".into();
    let new = Pending::begin(&mut s, other).unwrap();
    old.release(&mut s);
    cancel(&mut s, old.id);
    assert!(new.current(&s));
    retain_limit(&mut s, &old.ctx, Some(30));
    assert!(cooling(&s, &old.ctx));
    assert!(!new.read_current(&s));
    s.core.account_epoch += 1;
    s.app_home.cooldowns.clear();
    retain_limit(&mut s, &old.ctx, Some(30));
    assert!(s.app_home.cooldowns.is_empty());
}
#[test]
fn human_or_wrong_identity_response_never_navigates() {
    for change in 0..3 {
        let (mut s, target) = setup();
        let pending = Pending::begin(&mut s, target.clone()).unwrap();
        let (mut home, mut channel, mut user) = response(&target);
        let tab = match change {
            0 => {
                user.is_bot = false;
                Tab::Home
            }
            1 => {
                home.app = "A_OTHER".into();
                Tab::Home
            }
            _ => {
                channel.user = Some("U_OTHER".into());
                Tab::Home
            }
        };
        assert!(!pending.adopt(&mut s, &home, tab, channel, user));
        assert!(
            !s.core.workspaces[&target.installed_team]
                .channels
                .contains_key("D_RETURNED")
        );
    }
}

#[test]
fn calendar_lookup_cancel_retains_confirmation_and_late_result_cannot_close_it() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    // Existing closed fixture installation grants no production dispatch. The
    // confirmed Calendar arrangement is supplied through its cfg(test) seam.
    let id = crate::message_buttons::shortcuts::calendar_navigation_test_setup(&mut s);
    let target = Target::new(
        "A_CALENDAR".into(),
        s.core.active_team.clone().unwrap(),
        Tab::Home,
    )
    .unwrap();
    let pending = Pending::begin_from(&mut s, target.clone(), Some(id)).unwrap();
    cancel(&mut s, pending.id);
    let (home, channel, user) = response(&target);
    assert!(!pending.adopt(&mut s, &home, Tab::Home, channel, user));
    assert!(crate::message_buttons::shortcuts::calendar_home_navigation_current(&s, id));
    let next = Pending::begin_from(&mut s, target.clone(), Some(id)).unwrap();
    let (home, channel, user) = response(&target);
    assert!(next.adopt(&mut s, &home, Tab::Home, channel, user));
    assert!(!crate::message_buttons::shortcuts::calendar_home_navigation_current(&s, id));
    assert!(s.overlay.is_none());
}

#[test]
fn navigation_replacement_releases_old_mount_and_stale_cancel_cannot_unlock_new_mount() {
    use tokio::sync::oneshot::error::TryRecvError;
    let (mut s, target) = setup();
    let old = Pending::begin(&mut s, target.clone()).unwrap();
    let (release, mut released) = inputs::native::test_pair();
    s.app_home.navigation.as_mut().unwrap().native_lease = Some(release);
    let mut target = target;
    target.app = "A_NEW".into();
    let new = Pending::begin(&mut s, target).unwrap();
    assert_eq!(released.try_recv(), Ok(()));
    let (release, mut released) = inputs::native::test_pair();
    s.app_home.navigation.as_mut().unwrap().native_lease = Some(release);
    old.release(&mut s);
    cancel(&mut s, old.id);
    assert_eq!(released.try_recv(), Err(TryRecvError::Empty));
    assert!(new.current(&s));
    cancel(&mut s, new.id);
    assert_eq!(released.try_recv(), Ok(()));
    assert!(s.app_home.navigation.is_none());
}

#[test]
fn exact_about_destination_adopts_real_bot_and_does_not_substitute_disabled_home() {
    let (mut s, mut target) = setup();
    target.requested = Tab::About;
    let pending = Pending::begin(&mut s, target.clone()).unwrap();
    let (mut home, channel, user) = response(&target);
    home.enabled = false;
    home.messages = false;
    assert!(pending.adopt(&mut s, &home, Tab::About, channel, user));
    assert_eq!(s.channel_tab, ChannelTab::AppAbout);
    assert_eq!(s.core.active_channel.as_deref(), Some("D_RETURNED"));
}
