use super::*;
use serde_json::json;

#[test]
fn current_acl_requires_completed_matching_profile_without_historical_assignment() {
    let (mut s, id, ctx, ch, ticket) = setup();
    assert!(!adopt(&mut s, &ticket, &raw(), None));
    assert!(!sharing::channel(&s, &ctx, &ch));
    assert!(adopt(&mut s, &ticket, &raw(), Some(profile(false))));
    assert!(actions::event(&s, id).is_none());
    // Current FS2H refuses an explicit false. A completed matching app profile
    // with the optional property omitted is distinct from no acquired profile.
    let omitted =
        policy::AppProfile::decode("ACAL", &json!({"ok":true,"app_profile":{"id":"ACAL"}}))
            .unwrap();
    assert!(adopt(&mut s, &ticket, &raw(), Some(omitted)));
    assert_eq!(actions::event(&s, id).unwrap().share_channel, "DAPPCAL");
    let other = policy::AppProfile::decode(
        "AOTHER",
        &json!({"ok":true,"app_profile":{"id":"AOTHER","is_interactable":true}}),
    )
    .unwrap();
    assert!(!adopt(&mut s, &ticket, &raw(), Some(other)));
    active_mut(&mut s, id).unwrap().share_generation += 1;
    assert!(!adopt(&mut s, &ticket, &raw(), Some(profile(true))));
    assert!(actions::event(&s, id).is_none());
}

fn setup() -> (ShellState, u64, Arc<Context>, Channel, Ticket) {
    let (mut s, id) = super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    let ch: Channel =
        serde_json::from_value(json!({"id":"DAPPCAL","is_im":true,"user":"UCALBOT"})).unwrap();
    let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    w.users.insert(
        "UCALBOT".into(),
        serde_json::from_value(
            json!({"id":"UCALBOT","is_bot":true,"profile":{"api_app_id":"ACAL"}}),
        )
        .unwrap(),
    );
    w.channels.insert(ch.id.clone(), ch.clone());
    let c = active_mut(&mut s, id).unwrap();
    c.share_generation = 7;
    c.share = true;
    c.share_channel = ch.id.clone();
    posting::test_acquire(&mut s, id, vec![raw()["channel"].clone()]);
    let ticket = capture(&s, id, 7, &ch).unwrap();
    (s, id, ctx, ch, ticket)
}
fn raw() -> serde_json::Value {
    json!({"ok":true,"channel":{"id":"DAPPCAL","is_im":true,"user":"UCALBOT"},"app_home":{"app_id":"ACAL","conversation_id":"DAPPCAL","messages_tab_enabled":true}})
}
fn profile(allow: bool) -> policy::AppProfile {
    policy::AppProfile::decode(
        "ACAL",
        &json!({"ok":true,"app_profile":{"id":"ACAL","is_interactable":allow}}),
    )
    .unwrap()
}
#[test]
fn owned_metadata_enables_actual_sharing_capture_without_membership_guess() {
    let (mut s, id, ctx, ch, ticket) = setup();
    assert!(!sharing::channel(&s, &ctx, &ch));
    assert!(sharing::resolve(&s, id).is_none());
    assert!(adopt(&mut s, &ticket, &raw(), Some(profile(true))));
    assert!(sharing::channel(&s, &ctx, &ch));
    assert_eq!(sharing::resolve(&s, id).as_deref(), Some("DAPPCAL"));
    assert_eq!(actions::event(&s, id).unwrap().share_channel, "DAPPCAL");
    assert!(adopt(&mut s, &ticket, &raw(), Some(profile(false))));
    assert!(actions::event(&s, id).is_none());
}
#[test]
fn changed_query_account_owner_peer_or_app_cannot_adopt_or_reuse_metadata() {
    for mutation in 0..7 {
        let (mut s, id, ctx, ch, ticket) = setup();
        assert!(adopt(&mut s, &ticket, &raw(), Some(profile(true))));
        match mutation {
            0 => active_mut(&mut s, id).unwrap().share_generation += 1,
            1 => s.core.account_epoch += 1,
            2 => s.channel_generation += 1,
            3 => {
                s.core
                    .workspaces
                    .get_mut(&ctx.session.team_id)
                    .unwrap()
                    .channels
                    .get_mut(&ch.id)
                    .unwrap()
                    .user = Some("UOTHER".into());
            }
            4 => {
                s.core
                    .workspaces
                    .get_mut(&ctx.session.team_id)
                    .unwrap()
                    .users
                    .get_mut("UCALBOT")
                    .unwrap()
                    .profile
                    .as_mut()
                    .unwrap()
                    .extra
                    .insert("api_app_id".into(), json!("AOTHER"));
            }
            5 => active_mut(&mut s, id).unwrap().stage = Stage::Uncertain,
            _ => close(&mut s, id, true),
        }
        assert!(!adopt(&mut s, &ticket, &raw(), Some(profile(true))));
        assert!(!allowed(&s, &ctx, &ch));
    }
}
#[test]
fn fresh_response_restrictions_and_wrong_peer_do_not_become_grants() {
    for case in 0..5 {
        let (mut s, _id, ctx, ch, ticket) = setup();
        let mut response = raw();
        match case {
            0 => response["channel"]["user"] = json!("UOTHER"),
            1 => response["channel"]["is_archived"] = json!(true),
            2 => response["app_home"]["messages_tab_enabled"] = json!(false),
            3 => response["app_home"]["messages_tab_read_only_enabled"] = json!(true),
            _ => response["app_home"]["app_id"] = json!("AOTHER"),
        }
        adopt(&mut s, &ticket, &response, Some(profile(true)));
        assert!(!sharing::channel(&s, &ctx, &ch));
    }
}

#[test]
fn query_retirement_discards_metadata_instead_of_accumulating_old_grants() {
    let (mut s, id, ctx, ch, ticket) = setup();
    assert!(adopt(&mut s, &ticket, &raw(), Some(profile(true))));
    let metadata = &mut active_mut(&mut s, id).unwrap().metadata;
    assert_eq!(metadata.homes.len(), 1);
    metadata.invalidate();
    assert!(metadata.homes.is_empty());
    assert!(!allowed(&s, &ctx, &ch));
}

#[test]
fn actual_channel_capture_distinguishes_app_user_flag_and_both_slackbot_shapes() {
    for (user_id, bot, app_user, slackbot, needs_metadata, admitted_without_metadata) in [
        ("UCALBOT", true, false, false, true, false),
        ("UCALBOT", false, true, false, false, true),
        ("UCALBOT", false, true, true, true, false),
        ("UCALBOT", false, false, true, false, false), // app-DM, but no actual app association
        ("USLACKBOT", true, false, true, false, true),
        ("USLACK", true, false, false, false, false),
    ] {
        let (mut s, id, ctx, mut ch, _) = setup();
        let user = serde_json::from_value(json!({"id":user_id,"is_bot":bot,"is_app_user":app_user,
            "is_slackbot":slackbot,"profile":{"api_app_id":"ACAL"}}))
        .unwrap();
        let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
        w.users.insert(user_id.into(), user);
        ch.user = Some(user_id.into());
        w.channels.insert(ch.id.clone(), ch.clone());
        let mut observed = raw()["channel"].clone();
        observed["user"] = json!(user_id);
        posting::test_acquire(&mut s, id, vec![observed]);
        let ticket = capture(&s, id, 7, &ch);
        assert_eq!(ticket.is_some(), needs_metadata);
        assert_eq!(actions::event(&s, id).is_some(), admitted_without_metadata);
        if let Some(ticket) = ticket {
            let mut response = raw();
            response["channel"]["user"] = json!(user_id);
            assert!(adopt(&mut s, &ticket, &response, Some(profile(true))));
            assert_eq!(actions::event(&s, id).unwrap().share_channel, ch.id);
        }
    }
}
