use super::*;
use serde_json::json;
fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
#[test]
fn chooser_cancel_preserves_both_composers_and_newer_input() {
    let mut s = setup();
    s.core.composer.text = "untouched channel".into();
    s.core.thread_composer.text = "newer thread".into();
    let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    assert!(is_current(&s, id));
    query(&mut s, id, "Équipe 東京".into());
    choose(&mut s, id, 1);
    assert_eq!(
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .selected,
        Some(1)
    );
    cancel(&mut s);
    assert!(s.overlay.is_none());
    assert!(s.message_buttons.shortcuts.dialog.is_none());
    assert_eq!(s.core.composer.text, "untouched channel");
    assert_eq!(s.core.thread_composer.text, "newer thread");
}
#[test]
fn channel_aba_and_credentials_retire_choices_before_dispatch() {
    for change in 0..3 {
        let mut s = setup();
        let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
        match change {
            0 => s.channel_generation += 1,
            1 => s.core.account_epoch += 1,
            _ => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token
                .push('x'),
        }
        choose(&mut s, id, 0);
        assert_eq!(
            s.message_buttons
                .shortcuts
                .dialog
                .as_ref()
                .unwrap()
                .selected,
            None
        );
        poll(&mut s);
        assert!(s.message_buttons.shortcuts.dialog.is_none());
        assert!(s.overlay.is_none());
    }
}
#[test]
fn cancelled_token_cannot_open_modal_and_old_cancel_does_not_clear_other_tokens() {
    let mut s = setup();
    let t = crate::app_modal::register_action(&mut s, "A_REVIEW").unwrap();
    let token = t.token.clone();
    s.message_buttons.shortcuts.dialog.as_mut().unwrap().token = Some(t);
    let other = crate::app_modal::register_action(&mut s, "A_OTHER").unwrap();
    cancel(&mut s);
    let team = s.core.active_team.clone().unwrap();
    let generation = s.core.workspaces[&team].rt_generation;
    let event = |token: &str, app: &str| {
        super_platinum_core::slack::events::RtEvent::Unknown(serde_json::from_value(json!({
        "type":"view_opened","view_id":"V1","view_type":"modal","client_token":token,"app_id":app,
        "view":{"id":"V1","app_id":app,"type":"modal","title":{"type":"plain_text","text":"Review"},"blocks":[]}})).unwrap())
    };
    crate::app_modal::realtime(&mut s, &team, generation, &event(&token, "A_REVIEW"));
    assert!(s.overlay.is_none());
    crate::app_modal::realtime(&mut s, &team, generation, &event(&other.token, "A_OTHER"));
    assert_eq!(s.overlay, Some(Overlay::AppModal));
}
#[test]
fn rate_limit_survives_cancel_and_other_owner_cannot_set_it() {
    let mut s = setup();
    let ctx = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .unwrap()
        .owner
        .ctx
        .clone();
    retain_limit(
        &mut s,
        &ctx,
        Some(&Error::RateLimited {
            retry_after_secs: Some(120),
        }),
    );
    cancel(&mut s);
    assert!(cooling(&s, &ctx));
    s.core.account_epoch += 1;
    s.message_buttons.shortcuts.cooldowns.clear();
    retain_limit(
        &mut s,
        &ctx,
        Some(&Error::RateLimited {
            retry_after_secs: Some(120),
        }),
    );
    assert!(!cooling(&s, &ctx));
}

#[test]
fn old_cancel_cannot_dismiss_new_visit_and_paging_retains_all_choices() {
    let mut s = setup();
    let old = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    fixture::install(&mut s);
    let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    cancel_owned(&mut s, old);
    assert!(is_current(&s, id));
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    let seed = d.catalog.as_ref().unwrap().actions[0].clone();
    d.catalog.as_mut().unwrap().actions = (0..129)
        .map(|n| {
            let mut a = seed.clone();
            a.id = format!("row{n}");
            a.name = format!("Choice {n}");
            a
        })
        .collect();
    page(&mut s, id, true);
    page(&mut s, id, true);
    page(&mut s, id, true);
    let d = s.message_buttons.shortcuts.dialog.as_ref().unwrap();
    assert_eq!(d.page, 2);
    assert_eq!(matching(d).skip(d.page * 64).count(), 1);
    choose(&mut s, id, 128);
    assert_eq!(
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .selected,
        Some(128)
    );
    query(&mut s, id, "Choice 128".into());
    assert_eq!(s.message_buttons.shortcuts.dialog.as_ref().unwrap().page, 0);
    cancel_owned(&mut s, id);
    assert!(s.overlay.is_none());
}

#[test]
fn stale_shortcut_poll_requests_exactly_one_repaint() {
    let mut s = setup();
    s.channel_generation += 1;
    assert!(crate::app_modal::poll(&mut s));
    assert!(s.overlay.is_none());
    assert!(!crate::app_modal::poll(&mut s));
}

#[test]
fn real_admission_builds_once_and_owner_change_refuses_without_touching_draft() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    // The static fixture's C2 is not a valid production modal origin. Capture
    // the real admission owner only after selecting a valid synthetic channel.
    let team = s.core.active_team.clone().unwrap();
    let old_channel = s.core.active_channel.clone().unwrap();
    let workspace = s.core.workspaces.get_mut(&team).unwrap();
    let mut channel = workspace.channels[&old_channel].clone();
    channel.id = "C00000002".into();
    workspace.channels.insert(channel.id.clone(), channel);
    s.core.active_channel = Some("C00000002".into());
    s.refresh_from_core();
    fixture::install(&mut s);
    let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    s.message_buttons.shortcuts.dialog.as_mut().unwrap().fixture = false;
    s.core.composer.text = "later input remains".into();
    choose(&mut s, id, 0);
    let (_, request) = prepare_launch(&mut s, id).unwrap();
    assert!(request.url.contains("/api/apps.actions.v2.execute?"));
    let super_platinum_core::slack::client::RequestBody::Form(fields) = &request.body else {
        panic!("shortcut request must use the real form builder");
    };
    let context: serde_json::Value =
        serde_json::from_str(&fields.iter().find(|(key, _)| key == "context").unwrap().1).unwrap();
    assert_eq!(context, json!({"channel_id":"C00000002"}));
    assert_eq!(s.app_modal.as_ref().unwrap().modal.token_count(), 1);
    assert!(prepare_launch(&mut s, id).is_none());
    assert!(
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .token
            .is_some()
    );
    assert_eq!(s.core.composer.text, "later input remains");
    cancel_owned(&mut s, id);
    fixture::install(&mut s);
    let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    s.message_buttons.shortcuts.dialog.as_mut().unwrap().fixture = false;
    choose(&mut s, id, 0);
    s.channel_generation += 1;
    assert!(prepare_launch(&mut s, id).is_none());
    assert!(
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .token
            .is_none()
    );
}

#[test]
fn malformed_fixture_origin_is_refused_without_token_or_draft_change() {
    let mut s = setup();
    assert_eq!(s.core.active_channel.as_deref(), Some("C2"));
    let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    s.message_buttons.shortcuts.dialog.as_mut().unwrap().fixture = false;
    s.core.composer.text = "channel draft remains".into();
    s.core.thread_composer.text = "thread draft remains".into();
    choose(&mut s, id, 0);
    assert!(is_current(&s, id));
    assert_eq!(
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .selected,
        Some(0)
    );
    assert!(prepare_launch(&mut s, id).is_none());
    let dialog = s.message_buttons.shortcuts.dialog.as_ref().unwrap();
    assert!(dialog.phase == Phase::Choosing);
    assert!(dialog.token.is_none());
    assert_eq!(s.app_modal.as_ref().unwrap().modal.token_count(), 0);
    assert_eq!(s.core.composer.text, "channel draft remains");
    assert_eq!(s.core.thread_composer.text, "thread draft remains");
}

#[test]
fn enterprise_context_never_infers_unified_membership_from_saved_sessions() {
    let mut s = setup();
    let team = s.core.active_team.clone().unwrap();
    let channel = s.core.active_channel.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("E1".into());
    let ctx = Context::capture(&s).unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .remove("context_team_id");
    assert_eq!(related(&s, &ctx, &channel), None);
    let ch = s
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap();
    ch.extra.insert("context_team_id".into(), json!(team));
    assert_eq!(related(&s, &ctx, &channel), Some(vec![team.clone()]));
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .insert("is_org_shared".into(), json!(true));
    assert_eq!(related(&s, &ctx, &channel), None);
}

#[test]
fn fresh_grid_membership_is_owner_bound_and_never_creates_credentials() {
    let mut s = setup();
    let team = s.core.active_team.clone().unwrap();
    let channel = s.core.active_channel.clone().unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .insert("is_org_shared".into(), json!(true));
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .insert("is_global_shared".into(), json!(true));
    let ctx = Context::capture(&s).unwrap();
    let scope = channel_scope(&s, &ctx, &channel).unwrap();
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    d.owner.channel_scope = scope;
    d.phase = Phase::Loading;
    d.owner.related.clear();
    let id = d.id;
    let response = json!({"ok":true,"self":{"id":ctx.session.user_id},"team":{"id":team},
        "workspaces":[{"id":"T_NEW"},{"id":"T_EXT","allow_external_users":true}]});
    let membership = domain::grid::Membership::decode(response, &ctx.session).unwrap();
    let credentials = s.core.session.as_ref().unwrap().workspaces.len();
    s.core.composer.text = "later owned draft".into();
    assert!(adopt_membership(&mut s, id, membership.clone()));
    assert_eq!(
        s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .owner
            .related,
        vec!["T_NEW"]
    );
    assert_eq!(
        s.core.session.as_ref().unwrap().workspaces.len(),
        credentials
    );
    s.channel_generation += 1;
    assert!(!adopt_membership(&mut s, id, membership.clone()));
    cancel(&mut s);
    fixture::install(&mut s);
    assert!(!adopt_membership(&mut s, id, membership));
    assert_eq!(s.core.composer.text, "later owned draft");
}

#[test]
fn channel_routing_metadata_change_retires_loaded_shortcut() {
    let mut s = setup();
    let id = s.message_buttons.shortcuts.dialog.as_ref().unwrap().id;
    let team = s.core.active_team.clone().unwrap();
    let channel = s.core.active_channel.clone().unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut(&channel)
        .unwrap()
        .extra
        .insert("is_org_shared".into(), json!(true));
    assert!(!is_current(&s, id));
    assert!(poll(&mut s));
    assert!(s.message_buttons.shortcuts.dialog.is_none());
}
