use super::*;
use serde_json::{Value, json};
use std::time::Duration;
use super_platinum_core::{config::WorkspaceSession, slack::models::BotProfile};

const CURRENT: &str = "TCURRENT1";
const EXTERNAL: &str = "TEXTERNAL";
const OTHER: &str = "TOTHER123";

fn workspace() -> Workspace {
    Workspace::from_session(&WorkspaceSession {
        team_id: CURRENT.into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
    })
}
fn message(profile: Option<Value>) -> Message {
    let mut raw = json!({"bot_id":"B1","user":"U_APP","app_id":"AHINT","ts":"1.1"});
    if let Some(profile) = profile {
        raw["bot_profile"] = profile;
    }
    serde_json::from_value(raw).unwrap()
}
fn member(workspace: &mut Workspace, raw: Value) {
    let member: User = serde_json::from_value(raw).unwrap();
    workspace.users.insert(member.id.clone(), member);
}
fn cache(workspace: &mut Workspace, raw: Value) {
    let profile: BotProfile = serde_json::from_value(raw).unwrap();
    let ticket = workspace.bots.begin("B1", None, Duration::ZERO).unwrap();
    assert!(
        workspace
            .bots
            .complete(&ticket, Some(profile), Duration::ZERO)
    );
}

#[test]
fn inline_profile_wins_over_cache_and_lookup_hint_without_overriding_message_service_id() {
    let mut workspace = workspace();
    cache(
        &mut workspace,
        json!({"id":"B1","app_id":"ACACHED","team_id":OTHER}),
    );
    let message = message(Some(
        json!({"id":"B_PROFILE","app_id":"AINLINE","team_id":EXTERNAL}),
    ));
    let resolved = resolve(&workspace, &message).unwrap();
    assert_eq!(
        resolved,
        Service {
            service_id: "B1".into(),
            app_id: "AINLINE".into(),
            team_id: EXTERNAL.into()
        }
    );
    assert!(lookup_target(&workspace, &message).is_none());
}

#[test]
fn incomplete_inline_profile_never_borrows_app_or_team_from_cache() {
    let mut workspace = workspace();
    cache(
        &mut workspace,
        json!({"id":"B1","app_id":"ACACHED","team_id":EXTERNAL}),
    );
    let no_app = message(Some(json!({"id":"B1"})));
    assert_eq!(
        resolve(&workspace, &no_app),
        Err(MessageActionError::MissingServiceIdentity)
    );
    assert!(lookup_target(&workspace, &no_app).is_none());
    let no_team = message(Some(json!({"app_id":"AINLINE"})));
    assert_eq!(resolve(&workspace, &no_team).unwrap().team_id, CURRENT);
}

#[test]
fn missing_inline_profile_resolves_exact_cached_bot_and_hint_or_requests_lookup() {
    let mut workspace = workspace();
    let message = message(None);
    assert_eq!(
        lookup_target(&workspace, &message),
        Some(("B1".into(), Some("AHINT".into())))
    );
    assert_eq!(
        resolve(&workspace, &message),
        Err(MessageActionError::MissingServiceIdentity)
    );
    cache(
        &mut workspace,
        json!({"id":"B1","app_id":"AHINT","team_id":EXTERNAL}),
    );
    assert!(lookup_target(&workspace, &message).is_none());
    assert_eq!(resolve(&workspace, &message).unwrap().team_id, EXTERNAL);
    let mut different = message.clone();
    different.extra.insert("app_id".into(), json!("AOTHER"));
    assert!(resolve(&workspace, &different).is_err());
    assert!(lookup_target(&workspace, &different).is_some());
}

#[test]
fn profile_team_must_match_slacks_team_pattern_before_beating_member_fallback() {
    let mut workspace = workspace();
    member(&mut workspace, json!({"id":"U_APP","team_id":EXTERNAL}));
    for team in [
        json!(OTHER),
        json!("Tshort"),
        json!("Elong1234"),
        json!("E12345678"),
        json!("T1234567"),
        json!(""),
        Value::Null,
    ] {
        let message = message(Some(json!({"app_id":"A1","team_id":team})));
        assert_eq!(
            resolve(&workspace, &message).unwrap().team_id,
            if team == json!(OTHER) {
                OTHER
            } else {
                EXTERNAL
            }
        );
    }
    assert!(
        resolve(
            &workspace,
            &message(Some(json!({"app_id":"A1","team_id":[]})))
        )
        .is_err()
    );
}

#[test]
fn member_workspace_and_enterprise_membership_follow_captured_precedence() {
    let cases = [
        (
            json!({"id":"U_APP","team_id":EXTERNAL,"enterprise_user":{"teams":[CURRENT]}}),
            EXTERNAL,
        ),
        (
            json!({"id":"U_APP","team_id":EXTERNAL,"deleted":true}),
            EXTERNAL,
        ),
        (
            json!({"id":"U_APP","team_id":"E12345678","enterprise_user":{"teams":[OTHER,CURRENT]}}),
            CURRENT,
        ),
        (
            json!({"id":"U_APP","team_id":"E12345678","enterprise_user":{"teams":[EXTERNAL,OTHER]}}),
            EXTERNAL,
        ),
        (
            json!({"id":"U_APP","team_id":"E12345678","deleted":true,"enterprise_user":{"teams":[EXTERNAL]}}),
            CURRENT,
        ),
        (
            json!({"id":"U_APP","enterprise_user":{"teams":[EXTERNAL]}}),
            EXTERNAL,
        ),
        (
            json!({"id":"U_APP","enterprise_user":{"teams":[]}}),
            CURRENT,
        ),
        (json!({"id":"U_APP"}), CURRENT),
    ];
    for (raw, expected) in cases {
        let mut workspace = workspace();
        member(&mut workspace, raw);
        assert_eq!(
            resolve(&workspace, &message(Some(json!({"app_id":"A1"}))))
                .unwrap()
                .team_id,
            expected
        );
    }
}

#[test]
fn slackbot_and_slack_app_members_use_current_workspace_even_with_external_membership() {
    for raw in [
        json!({"id":"USLACKBOT","team_id":EXTERNAL}),
        json!({"id":"USLACK","team_id":EXTERNAL}),
        json!({"id":"U_APP","is_slackbot":true,"team_id":EXTERNAL}),
    ] {
        let mut workspace = workspace();
        let id = raw["id"].as_str().unwrap().to_owned();
        member(&mut workspace, raw);
        let mut message = message(Some(json!({"app_id":"A1"})));
        message.user = Some(id);
        assert_eq!(resolve(&workspace, &message).unwrap().team_id, CURRENT);
    }
}

#[test]
fn no_member_and_unusable_team_fall_back_to_current_not_message_author_team() {
    let workspace = workspace();
    let mut message = message(Some(json!({"app_id":"A1"})));
    message.team = Some(EXTERNAL.into());
    assert_eq!(resolve(&workspace, &message).unwrap().team_id, CURRENT);
    message.user = None;
    assert_eq!(resolve(&workspace, &message).unwrap().team_id, CURRENT);
}

#[test]
fn special_bots_and_member_only_identity_do_not_trigger_generic_lookup() {
    let mut workspace = workspace();
    for id in [
        "B01",
        "BSLACKBOT",
        "B03",
        "BSLACKSECURITY",
        "BCHANNELEXPERT",
    ] {
        let mut message = message(None);
        message.bot_id = Some(id.into());
        assert!(lookup_target(&workspace, &message).is_none());
    }
    member(
        &mut workspace,
        json!({"id":"U_APP","is_bot":true,"profile":{"bot_id":"B1","api_app_id":"A1"}}),
    );
    let mut message = message(None);
    message.bot_id = None;
    assert!(lookup_target(&workspace, &message).is_none());
    assert!(resolve(&workspace, &message).is_err());
}

#[test]
fn service_snapshot_changes_when_member_routing_changes_without_message_edit() {
    let mut workspace = workspace();
    let message = message(Some(json!({"app_id":"A1"})));
    let first = resolve(&workspace, &message).unwrap();
    member(&mut workspace, json!({"id":"U_APP","team_id":EXTERNAL}));
    let next = resolve(&workspace, &message).unwrap();
    assert_ne!(first, next);
    assert_eq!(next.team_id, EXTERNAL);
}
