use super::*;
use serde_json::json;

fn setup() -> (ShellState, u64, User) {
    let (mut s, id) = super::super::tests::setup();
    let user: User = serde_json::from_value(
        json!({"id":"UCALAGENT","is_bot":true,"profile":{"api_app_id":"ACALAGENT"}}),
    )
    .unwrap();
    let c = active_mut(&mut s, id).unwrap();
    c.agents.users.insert(user.id.clone(), user.clone());
    c.agents.profiles.insert("ACALAGENT".into(),facts::Profile::decode("ACALAGENT",
        &json!({"ok":true,"app_profile":{"id":"ACALAGENT","is_agent_app":true,"bot_user":{"id":"UCALAGENT"}}})).unwrap());
    c.guests.push(Person {
        id: user.id.clone(),
        label: "Agent".into(),
    });
    (s, id, user)
}
#[test]
fn actual_create_capture_admits_matching_agent_and_rechecks_newer_member_identity() {
    let (mut s, id, user) = setup();
    assert_eq!(
        actions::event(&s, id).unwrap().attendees,
        vec![user.id.clone()]
    );
    let team = context(&s, id).unwrap().session.team_id.clone();
    let mut changed = user.clone();
    changed
        .profile
        .as_mut()
        .unwrap()
        .extra
        .insert("api_app_id".into(), json!("AOTHER"));
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .insert(user.id.clone(), changed);
    assert!(actions::event(&s, id).is_none());
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .insert(user.id.clone(), user);
    assert!(actions::event(&s, id).is_some());
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("UCALAGENT")
        .unwrap()
        .deleted = true;
    assert!(actions::event(&s, id).is_none());
}
#[test]
fn app_profile_removal_or_agentforce_without_access_cannot_submit_old_selected_label() {
    let (mut s, id, mut user) = setup();
    assert!(actions::event(&s, id).is_some());
    let profile = active_mut(&mut s, id)
        .unwrap()
        .agents
        .profiles
        .remove("ACALAGENT")
        .unwrap();
    assert!(actions::event(&s, id).is_none());
    active_mut(&mut s, id)
        .unwrap()
        .agents
        .profiles
        .insert("ACALAGENT".into(), profile);
    user.extra.insert("is_agentforce_bot".into(), json!(true));
    active_mut(&mut s, id)
        .unwrap()
        .agents
        .users
        .insert(user.id.clone(), user);
    assert!(actions::event(&s, id).is_none());
}
#[test]
fn query_stage_owner_and_confirmation_retirement_block_read_and_adoption_ticket() {
    for mutation in 0..6 {
        let (mut s, id, _) = setup();
        assert!(current_query(&s, id, 0));
        match mutation {
            0 => active_mut(&mut s, id).unwrap().guest_generation += 1,
            1 => active_mut(&mut s, id).unwrap().stage = Stage::Creating,
            2 => active_mut(&mut s, id).unwrap().confirm_close = true,
            3 => s.channel_generation += 1,
            4 => s.core.account_epoch += 1,
            _ => {
                close(&mut s, id, true);
            }
        }
        assert!(!current_query(&s, id, 0));
    }
}
#[test]
fn guest_exclusions_survive_agent_exception_and_remote_plain_user_has_real_read_identity() {
    let (mut s, id, user) = setup();
    let ctx = context(&s, id).unwrap();
    let c = active(&s, id).unwrap();
    let canonical: User = serde_json::from_value(json!({"id":"USLACKBOT"})).unwrap();
    assert!(!eligible(c, &canonical, &ctx.session));
    for key in [
        "is_slackbot",
        "is_slacksecuritybot",
        "is_workflow_bot",
        "is_profile_only_user",
    ] {
        let mut blocked = user.clone();
        blocked.extra.insert(key.into(), json!(true));
        assert!(!eligible(c, &blocked, &ctx.session));
    }
    let normal: User = serde_json::from_value(json!({"id":"UCALREMOTE","name":"Remote"})).unwrap();
    let c = active_mut(&mut s, id).unwrap();
    c.guests = vec![Person {
        id: normal.id.clone(),
        label: "Remote".into(),
    }];
    assert!(actions::event(&s, id).is_none());
    active_mut(&mut s, id)
        .unwrap()
        .agents
        .users
        .insert(normal.id.clone(), normal);
    assert!(actions::event(&s, id).is_some());
}

#[test]
fn malformed_member_flags_never_bypass_actual_create_agentforce_or_exclusion_guards() {
    for key in [
        "is_app_user",
        "is_agentforce_bot",
        "is_slackbot",
        "is_workflow_bot",
        "is_profile_only_user",
        "is_slacksecuritybot",
    ] {
        for malformed in [json!("true"), json!(1), json!({}), json!([]), json!(null)] {
            let (mut s, id, mut user) = setup();
            assert!(actions::event(&s, id).is_some());
            user.extra.insert(key.into(), malformed);
            active_mut(&mut s, id)
                .unwrap()
                .agents
                .users
                .insert(user.id.clone(), user.clone());
            assert!(
                actions::event(&s, id).is_none(),
                "malformed {key} must refuse"
            );
            user.extra.insert(key.into(), json!(false));
            active_mut(&mut s, id)
                .unwrap()
                .agents
                .users
                .insert(user.id.clone(), user);
            assert!(
                actions::event(&s, id).is_some(),
                "restored {key} must admit"
            );
        }
    }
}

#[test]
fn actual_create_requires_observed_slackbot_permission_for_exact_current_account_scope() {
    let (mut s, id, _) = setup();
    let ctx = context(&s, id).unwrap();
    let user: User =
        serde_json::from_value(json!({"id":"USLACKBOT","name":"Slackbot","is_bot":false})).unwrap();
    let c = active_mut(&mut s, id).unwrap();
    c.guests = vec![Person {
        id: user.id.clone(),
        label: "Slackbot".into(),
    }];
    c.agents.users.insert(user.id.clone(), user);
    assert!(actions::event(&s, id).is_none());
    let permission = facts::slackbot::Observation::decode(
        &ctx.session,
        &json!({"ok":true,"allowed":[facts::slackbot::PERMISSION]}),
    )
    .unwrap();
    active_mut(&mut s, id).unwrap().agents.slackbot = Some(permission.clone());
    assert!(actions::event(&s, id).is_some());
    let mut wrong = ctx.session.clone();
    wrong.team_id = "TOTHER".into();
    active_mut(&mut s, id).unwrap().agents.slackbot = Some(
        facts::slackbot::Observation::decode(
            &wrong,
            &json!({"ok":true,"allowed":[facts::slackbot::PERMISSION]}),
        )
        .unwrap(),
    );
    assert!(actions::event(&s, id).is_none());
    active_mut(&mut s, id).unwrap().agents.slackbot = Some(permission);
    assert!(actions::event(&s, id).is_some());
    s.core.account_epoch += 1;
    assert!(actions::event(&s, id).is_none());
}

#[test]
fn slackbot_permission_does_not_override_other_member_exclusions_or_ordinary_bot_policy() {
    let (mut s, id, _) = setup();
    let ctx = context(&s, id).unwrap();
    active_mut(&mut s, id).unwrap().agents.slackbot = Some(
        facts::slackbot::Observation::decode(
            &ctx.session,
            &json!({"ok":true,"allowed":[facts::slackbot::PERMISSION]}),
        )
        .unwrap(),
    );
    let c = active(&s, id).unwrap();
    let mut user: User = serde_json::from_value(json!({"id":"USLACKBOT","is_bot":false})).unwrap();
    assert!(eligible(c, &user, &ctx.session));
    user.is_bot = true;
    assert!(!eligible(c, &user, &ctx.session));
    user.is_bot = false;
    for key in [
        "is_workflow_bot",
        "is_profile_only_user",
        "is_slacksecuritybot",
    ] {
        user.extra.insert(key.into(), json!(true));
        assert!(!eligible(c, &user, &ctx.session));
        user.extra.remove(key);
    }
    user.deleted = true;
    assert!(!eligible(c, &user, &ctx.session));
}
