use super::*;
use crate::block_actions::app_eligibility::agents::Settings;
use serde_json::json;
use std::collections::BTreeSet;

const USER: &str = "UMANAG001";
const APP: &str = "AAGENT001";
fn raw() -> Value {
    json!({"id":USER,"team_id":"TLOCAL001","profile":{"real_name":"Manager"},"opaque":{"keep":[1,true]}})
}
fn bot(agentforce: bool) -> Member {
    let mut raw = raw();
    raw["is_bot"] = json!(true);
    raw["is_agentforce_bot"] = json!(agentforce);
    raw["profile"]["api_app_id"] = json!(APP);
    Member::decode(&raw).unwrap()
}
fn profile_raw(agent: bool, agentforce: bool) -> Value {
    json!({"ok":true,"app_profile":{"id":APP,"is_agent_app":agent,
        "bot_user":{"id":USER},"agentforce":if agentforce {json!({})} else {Value::Null},
        "opaque":{"keep":true}}})
}
fn profile(agent: bool, agentforce: bool) -> Profile {
    decode_profile(APP, &profile_raw(agent, agentforce)).unwrap()
}
fn access(prefs: Value, accessible: bool, profiles: Vec<Profile>) -> Access {
    Access::new(
        Settings::decode(&prefs, None).unwrap(),
        if accessible {
            BTreeSet::from([APP.into()])
        } else {
            BTreeSet::new()
        },
        profiles
            .into_iter()
            .map(|p| (p.app().to_owned(), p))
            .collect(),
    )
}
fn classify(raw: &Value) -> Eligibility {
    Member::decode(raw)
        .unwrap()
        .eligibility("TLOCAL001", None, None, None)
}

#[test]
fn self_and_enterprise_local_members_are_included_without_extra_exclusions() {
    let mut value = raw();
    for key in [
        "is_self",
        "is_stranger",
        "is_ultra_restricted",
        "is_invited_user",
        "is_pending",
        "is_inactive",
        "is_forgotten",
    ] {
        value[key] = json!(true);
    }
    assert_eq!(classify(&value), Eligibility::Allowed);
    value["team_id"] = json!("TOTHER001");
    assert_eq!(classify(&value), Eligibility::Denied);
    value["enterprise_user"] = json!({"teams":["TOTHER001","TLOCAL001"]});
    assert_eq!(classify(&value), Eligibility::Allowed);
    value["enterprise_user"] = json!({"enterprise_id":"TLOCAL001"});
    assert_eq!(classify(&value), Eligibility::Denied);
    value.as_object_mut().unwrap().remove("team_id");
    value["team"] = json!("TLOCAL001");
    assert_eq!(classify(&value), Eligibility::Denied);
}

#[test]
fn exclusions_precede_and_cannot_be_overridden_by_agent_grants() {
    let p = profile(true, true);
    let a = access(json!({}), true, vec![p.clone()]);
    for key in [
        "deleted",
        "is_restricted",
        "is_profile_only_user",
        "is_workflow_bot",
        "is_slackbot",
        "is_slacksecuritybot",
        "isUnknown",
        "isNonExistent",
    ] {
        let mut value = bot(true).raw().clone();
        value[key] = json!(true);
        assert_eq!(
            Member::decode(&value).unwrap().eligibility(
                "TLOCAL001",
                Some(true),
                Some(&p),
                Some(&a)
            ),
            Eligibility::Denied,
            "{key}"
        );
    }
    for id in [
        "USLACKBOT",
        "USLACKSECURITY",
        "USEARCH",
        "UCHANNELEXPERT",
        "USLACK",
    ] {
        let mut value = raw();
        value["id"] = json!(id);
        assert_eq!(classify(&value), Eligibility::Denied, "{id}");
    }
}

#[test]
fn ordinary_bots_need_exact_acquired_agent_profile() {
    let member = bot(false);
    assert_eq!(
        member.eligibility("TLOCAL001", None, None, None),
        Eligibility::NeedsProfile { app: APP.into() }
    );
    assert_eq!(
        member.eligibility("TLOCAL001", None, Some(&profile(false, true)), None),
        Eligibility::Denied
    );
    assert_eq!(
        member.eligibility("TLOCAL001", None, Some(&profile(true, false)), None),
        Eligibility::Allowed
    );
    let mut value = profile_raw(true, false);
    value["app_profile"]["id"] = json!("AOTHER001");
    let wrong = decode_profile("AOTHER001", &value).unwrap();
    assert_eq!(
        member.eligibility("TLOCAL001", None, Some(&wrong), None),
        Eligibility::Denied
    );
    let mut value = member.raw().clone();
    value["profile"]
        .as_object_mut()
        .unwrap()
        .remove("api_app_id");
    assert_eq!(
        Member::decode(&value)
            .unwrap()
            .eligibility("TLOCAL001", None, None, None),
        Eligibility::Denied
    );
}

#[test]
fn app_user_uses_profile_exception_too() {
    let mut value = bot(false).raw().clone();
    value["is_bot"] = json!(false);
    value["is_app_user"] = json!(true);
    let m = Member::decode(&value).unwrap();
    assert_eq!(
        m.eligibility("TLOCAL001", None, None, None),
        Eligibility::NeedsProfile { app: APP.into() }
    );
    assert_eq!(
        m.eligibility("TLOCAL001", None, Some(&profile(true, false)), None),
        Eligibility::Allowed
    );
}

#[test]
fn agentforce_requires_client_context_and_exact_access_before_bot_exception() {
    let member = bot(true);
    let p = profile(true, true);
    assert_eq!(
        member.eligibility("TLOCAL001", None, Some(&p), None),
        Eligibility::NeedsClientContext
    );
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), Some(&p), None),
        Eligibility::NeedsAgentAccess
    );
    let denied = access(json!({}), false, vec![p.clone()]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), Some(&p), Some(&denied)),
        Eligibility::Denied
    );
    let allowed = access(json!({}), true, vec![p]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, Some(&allowed)),
        Eligibility::Allowed
    );
    assert_eq!(
        member.eligibility("TLOCAL001", Some(false), Some(&profile(true, false)), None),
        Eligibility::Allowed
    );
    assert_eq!(
        member.eligibility("TLOCAL001", Some(false), Some(&profile(false, true)), None),
        Eligibility::Denied
    );
}

#[test]
fn raw_agentforce_flag_does_not_invent_client_access() {
    let mut value = raw();
    value["is_agentforce_bot"] = json!(true);
    let member = Member::decode(&value).unwrap();
    assert_eq!(
        member.eligibility("TLOCAL001", None, None, None),
        Eligibility::NeedsClientContext
    );
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, None),
        Eligibility::NeedsAgentAccess
    );
    let a = access(json!({}), false, vec![]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, Some(&a)),
        Eligibility::Denied
    );
    // No invented app association or ordinary-bot branch: the original raw
    // member is not a bot/app user, but the acquired profile names its bot ID.
    let a = access(json!({}), true, vec![profile(false, true)]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, Some(&a)),
        Eligibility::Allowed
    );
}

#[test]
fn ordinary_agent_profile_does_not_require_agentforce_bot_identity() {
    let member = bot(false);
    let mut raw = profile_raw(true, false);
    raw["app_profile"]["bot_user"]["id"] = json!("UOTHER001");
    let profile = decode_profile(APP, &raw).unwrap();
    assert_eq!(
        member.eligibility("TLOCAL001", None, Some(&profile), None),
        Eligibility::Allowed
    );
}

#[test]
fn default_agent_requires_matching_enabled_profile_and_bot_identity() {
    let member = bot(true);
    let prefs = json!({"default_channel_knowledge_agent":{"app_id":APP,"status":"inactive"}});
    let a = access(prefs.clone(), false, vec![profile(true, true)]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, Some(&a)),
        Eligibility::Allowed
    );
    let mut disabled = prefs;
    disabled["ai_apps_settings"] = json!({"apps":[{"app_id":APP,"is_ai_enabled":false}]});
    let a = access(disabled, true, vec![profile(true, true)]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, Some(&a)),
        Eligibility::Denied
    );
    let mut value = profile_raw(true, true);
    value["app_profile"]["bot_user"]["id"] = json!("UOTHER001");
    let a = access(json!({}), true, vec![decode_profile(APP, &value).unwrap()]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, Some(&a)),
        Eligibility::Denied
    );
    let a = access(json!({}), true, vec![profile(false, true)]);
    assert_eq!(
        member.eligibility("TLOCAL001", Some(true), None, Some(&a)),
        Eligibility::Denied
    );
}

#[test]
fn manager_profile_intake_refuses_unknown_wrong_identity_and_malformed_flags() {
    for key in ["isUnknown", "isNonExistent"] {
        let mut value = profile_raw(true, true);
        value["app_profile"][key] = json!(true);
        assert!(decode_profile(APP, &value).is_err());
        value["app_profile"][key] = json!(false);
        assert!(decode_profile(APP, &value).is_ok());
        value["app_profile"][key] = json!("false");
        assert!(decode_profile(APP, &value).is_err());
    }
    assert!(decode_profile("AOTHER001", &profile_raw(true, true)).is_err());
    let mut value = profile_raw(true, true);
    value["app_profile"]["is_agent_app"] = json!("true");
    assert!(decode_profile(APP, &value).is_err());
    value = profile_raw(true, true);
    value["ok"] = json!(false);
    assert!(decode_profile(APP, &value).is_err());
}

#[test]
fn acquired_pages_preserve_cursor_and_refuse_unknown_rows() {
    let mut raw = json!({"ok":true,"apps":[profile_raw(true,true)["app_profile"].clone()],"response_metadata":{"next_cursor":"opaque+next"}});
    let page = decode_page(&raw).unwrap();
    assert_eq!(page.cursor.as_deref(), Some("opaque+next"));
    assert_eq!(page.accessible, BTreeSet::from([APP.into()]));
    raw["apps"][0]["isUnknown"] = json!(true);
    assert!(decode_page(&raw).is_err());
}

#[test]
fn decode_preserves_original_row_and_never_rewrites_existing_collaborators() {
    let value = raw();
    let m = Member::decode(&value).unwrap();
    assert_eq!(m.raw(), &value);
    let mut malformed = value;
    malformed["is_bot"] = json!("true");
    assert!(Member::decode(&malformed).is_err());
    // Eligibility is a pure new-choice decision; the existing manager Draft's
    // resolved IDs are independent of label hydration (covered by its tests).
    let before = m.raw().clone();
    assert_eq!(
        m.eligibility("TOTHER001", None, None, None),
        Eligibility::Denied
    );
    assert_eq!(m.raw(), &before);
}
