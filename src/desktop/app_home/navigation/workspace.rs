//! C/0c redirects an app URI to its workspace before U6KP opens the app.
//! Only an already authenticated workspace in this account is a destination.
use super::*;
use home::navigation::Link;

fn destination(s: &ShellState, link: &Link, calendar: u64) -> Option<usize> {
    if !crate::message_buttons::shortcuts::calendar_home_navigation_current(s, calendar)
        || link.workspace != link.target.installed_team
        || s.core.active_team.as_deref() == Some(link.workspace.as_str())
    {
        return None;
    }
    Target::new(
        link.target.app.clone(),
        link.target.installed_team.clone(),
        link.target.requested,
    )
    .ok()?;
    let session = s.core.session.as_ref()?.workspaces.get(&link.workspace)?;
    let workspace = s.core.workspaces.get(&link.workspace)?;
    if session.team_id != link.workspace
        || session.user_id != workspace.self_user_id
        || session.token.is_empty()
    {
        return None;
    }
    let ctx = Context {
        transport: s.core.transport.clone()?,
        client: s.core.client.clone(),
        session: session.clone(),
        epoch: s.core.account_epoch,
    };
    if cooling(s, &ctx) {
        return None;
    }
    let mut matches = s
        .workspaces
        .iter()
        .enumerate()
        .filter(|(_, w)| w.id == link.workspace);
    let index = matches.next()?.0;
    matches.next().is_none().then_some(index)
}

pub(super) fn begin(s: &mut ShellState, link: Link, calendar: u64) -> Option<Pending> {
    // No await between this exact current-dialog check, its dismissal, and
    // switching. Unknown accounts/teams leave the confirmed event untouched.
    let index = destination(s, &link, calendar)?;
    clear(s);
    crate::message_buttons::shortcuts::close_calendar_home_navigation(s, calendar);
    // The current Calendar overlay excludes CanvasCreate. The existing route
    // still owns composer checkpointing, surface reset and channel generation.
    if !s.select_workspace(index) {
        return None;
    }
    // Capture the destination's credentials and runtime generation afresh.
    // Every later await uses the existing Pending guards and single executor.
    Pending::begin(s, link.target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (ShellState, u64, Link, String) {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        let id = crate::message_buttons::shortcuts::calendar_navigation_test_setup(&mut s);
        let original = s.core.active_team.clone().unwrap();
        let other = "T_CALENDAR_DESTINATION";
        let mut credentials = s.core.session.as_ref().unwrap().workspaces[&original].clone();
        credentials.team_id = other.into();
        credentials.user_id = "U_DESTINATION".into();
        credentials.token = "synthetic-destination".into();
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .insert(other.into(), credentials);
        let mut workspace = s.core.workspaces[&original].clone();
        workspace.self_user_id = "U_DESTINATION".into();
        s.core.workspaces.insert(other.into(), workspace);
        let mut projection = s.workspaces[s.active_workspace].clone();
        projection.id = other.into();
        s.workspaces.push(projection);
        let link = Link::parse(
            "slack://app?team=T_CALENDAR_DESTINATION&id=A_CALENDAR&tab=home",
            &original,
        )
        .unwrap();
        (s, id, link, original)
    }

    #[test]
    fn actual_workspace_handoff_captures_destination_token_and_retains_original_draft() {
        let (mut s, id, link, original) = setup();
        let channel = s.core.active_channel.clone().unwrap();
        s.core.composer.text = "original unsent composer".into();
        let pending = begin(&mut s, link.clone(), id).unwrap();
        assert_eq!(s.core.active_team.as_deref(), Some(link.workspace.as_str()));
        assert_eq!(pending.ctx.session.token, "synthetic-destination");
        assert_eq!(pending.ctx.session.user_id, "U_DESTINATION");
        let request = pending
            .target
            .prepare(&pending.ctx.client, &pending.ctx.session)
            .unwrap();
        let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
            panic!("actual app lookup form");
        };
        assert!(fields.contains(&("token".into(), "synthetic-destination".into())));
        assert!(fields.contains(&("app_installed_team_id".into(), link.workspace.clone())));
        assert!(pending.current(&s));
        assert!(s.overlay.is_none());
        assert!(!crate::message_buttons::shortcuts::calendar_home_navigation_current(&s, id));
        let original_index = s.workspaces.iter().position(|w| w.id == original).unwrap();
        assert!(s.select_workspace(original_index));
        let channel_index = s.channels.iter().position(|c| c.id == channel).unwrap();
        s.select_channel(channel_index, crate::state::ChannelOpen::Global);
        assert_eq!(s.core.composer.text, "original unsent composer");
        assert!(!pending.current(&s));
    }

    #[test]
    fn missing_or_mismatched_destination_and_stale_callback_do_not_dismiss_or_switch() {
        for change in 0..7 {
            let (mut s, id, mut link, original) = setup();
            match change {
                0 => {
                    s.core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .remove(&link.workspace);
                }
                1 => {
                    s.core.workspaces.remove(&link.workspace);
                }
                2 => {
                    s.core
                        .workspaces
                        .get_mut(&link.workspace)
                        .unwrap()
                        .self_user_id = "U_OTHER".into();
                }
                3 => {
                    link.target.installed_team = "T_OTHER".into();
                }
                4 => {
                    link.target.app = String::new();
                }
                5 => {
                    s.core.account_epoch += 1;
                }
                _ => {
                    s.channel_generation += 1;
                }
            }
            let visible = s.overlay;
            assert!(begin(&mut s, link, id).is_none());
            assert_eq!(s.core.active_team.as_deref(), Some(original.as_str()));
            assert_eq!(s.overlay, visible);
        }
    }

    #[test]
    fn destination_cooldown_refuses_handoff_and_newer_navigation_revokes_old_result() {
        let (mut s, id, link, original) = setup();
        s.app_home
            .cooldowns
            .insert((s.core.account_epoch, link.workspace.clone()), None);
        assert!(begin(&mut s, link.clone(), id).is_none());
        assert_eq!(s.core.active_team.as_deref(), Some(original.as_str()));
        assert!(crate::message_buttons::shortcuts::calendar_home_navigation_current(&s, id));
        s.app_home.cooldowns.clear();
        let pending = begin(&mut s, link, id).unwrap();
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&pending.ctx.session.team_id)
            .unwrap()
            .token
            .push('x');
        assert!(!pending.current(&s));
    }

    #[test]
    fn destination_url_or_enterprise_replacement_revokes_pending_lookup() {
        for change in 0..2 {
            let (mut s, id, link, _) = setup();
            let pending = begin(&mut s, link, id).unwrap();
            let session = s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&pending.ctx.session.team_id)
                .unwrap();
            if change == 0 {
                session.url = "https://replacement.example.test".into();
            } else {
                session.enterprise_id = Some("E_REPLACEMENT".into());
            }
            assert!(!pending.current(&s));
        }
    }
    #[test]
    fn created_calendar_about_target_preserves_authenticated_workspace_handoff() {
        let (mut s, id, mut link, _) = setup();
        link.target.requested = Tab::About;
        let expected = link.workspace.clone();
        let pending = begin(&mut s, link, id).unwrap();
        assert_eq!(pending.target.requested, Tab::About);
        assert_eq!(s.core.active_team.as_deref(), Some(expected.as_str()));
        assert!(pending.current(&s));
        s.channel_generation += 1;
        assert!(!pending.current(&s));
    }
}
