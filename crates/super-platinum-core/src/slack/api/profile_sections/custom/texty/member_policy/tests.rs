use super::*;
use serde_json::json;
fn user(fields: Value) -> User {
    let mut raw = json!({"id":"U00000002"});
    raw.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(raw).unwrap()
}
fn flags(keys: &[&str]) -> Policy {
    let rows: serde_json::Map<String, Value> = keys
        .iter()
        .map(|key| ((*key).into(), json!({"group":"on"})))
        .collect();
    Policy::decode(&json!({"ok":true,"assignments":rows})).unwrap()
}
#[test]
fn actual_profile_policy_matches_all_720_original_factory_cases() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("policy-oracle.json")).unwrap();
    assert_eq!(cases.len(), 720);
    for (i, case) in cases.into_iter().enumerate() {
        let policy = Policy::decode(&case["assignments"]).unwrap();
        let user = serde_json::from_value(case["user"].clone()).unwrap();
        assert_eq!(
            policy
                .allows(
                    &user,
                    Facts {
                        agent_allowed: case["agentAllowed"].as_bool(),
                        restricted_app: case["restrictedApp"].as_bool(),
                        salesforce_app: Some("A04T99UKKQE"),
                    }
                )
                .unwrap(),
            case["expected"].as_bool().unwrap(),
            "source case {i}"
        );
    }
}
#[test]
fn unconditional_exclusions_match_original_base_vectors() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("member-base-oracle.json")).unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        assert_eq!(
            member_base(&serde_json::from_value(case["raw"].clone()).unwrap()).unwrap(),
            case["expected"].as_bool().unwrap()
        );
    }
}
#[test]
fn complete_assignments_required_and_both_boolean_operands_validated() {
    assert!(Policy::decode(&json!({"ok":true,"assignments":{}})).is_ok());
    for raw in [
        json!({"assignments":{}}),
        json!({"ok":true}),
        json!({"ok":true,"error":"bad","assignments":{}}),
        json!({"ok":true,"assignments":{"slackbot_in_channels":{"group":"on"},"slackbot_in_channels_team_awareness":{"group":null}}}),
        json!({"ok":true,"assignments":{"loom_app_acl_search_filter":{"group":"off"},"app_acls_for_all_apps":{"group":false}}}),
    ] {
        assert!(Policy::decode(&raw).is_err());
    }
}
#[test]
fn missing_conditional_facts_never_become_denial_or_permission() {
    let app = user(json!({"is_bot":true,"profile":{"api_app_id":"A00000002"}}));
    let policy = flags(&["loom_app_acl_search_filter", "app_acls_for_all_apps"]);
    assert!(policy.allows(&app, Facts::default()).is_err());
    assert!(
        !policy
            .allows(
                &app,
                Facts {
                    restricted_app: Some(true),
                    ..Facts::default()
                }
            )
            .unwrap()
    );
    assert!(
        policy
            .allows(
                &app,
                Facts {
                    restricted_app: Some(false),
                    ..Facts::default()
                }
            )
            .unwrap()
    );
    let agent =
        user(json!({"is_bot":true,"is_agentforce_bot":true,"profile":{"api_app_id":"A00000002"}}));
    assert!(flags(&[]).allows(&agent, Facts::default()).is_err());
    assert!(
        !flags(&[])
            .allows(
                &agent,
                Facts {
                    agent_allowed: Some(false),
                    ..Facts::default()
                }
            )
            .unwrap()
    );
    assert!(
        flags(&[])
            .allows(
                &agent,
                Facts {
                    agent_allowed: Some(true),
                    ..Facts::default()
                }
            )
            .unwrap()
    );
    let salesforce = flags(&["slackforce_seamless_block_app_membership"]);
    assert!(salesforce.allows(&app, Facts::default()).is_err());
    assert!(
        !salesforce
            .allows(
                &app,
                Facts {
                    salesforce_app: Some("A00000002"),
                    ..Facts::default()
                }
            )
            .unwrap()
    );
}
#[test]
fn profile_self_deleted_humans_bots_guests_and_security_are_not_activity_options() {
    let policy = flags(&[]);
    for extra in [
        json!({}),
        json!({"deleted":true}),
        json!({"is_bot":true}),
        json!({"is_restricted":true}),
        json!({"is_profile_only_user":true}),
        json!({"id":"USLACKSECURITY"}),
    ] {
        assert!(policy.allows(&user(extra), Facts::default()).unwrap());
    }
    for id in ["USLACK", "USEARCH", "UCHANNELEXPERT", "USLACKBOT"] {
        assert!(
            !policy
                .allows(&user(json!({"id":id})), Facts::default())
                .unwrap()
        );
    }
    let enabled = flags(&[
        "enable_search_system_user",
        "enable_native_channel_expert",
        "slackbot_in_channels_team_awareness",
    ]);
    for id in ["USEARCH", "UCHANNELEXPERT", "USLACKBOT"] {
        assert!(
            enabled
                .allows(&user(json!({"id":id})), Facts::default())
                .unwrap()
        );
    }
}
#[test]
fn excluded_rows_need_no_app_agent_environment_acquisition() {
    let p = flags(&[
        "loom_app_acl_search_filter",
        "app_acls_for_all_apps",
        "slackforce_seamless_block_app_membership",
    ]);
    let u = user(
        json!({"is_workflow_bot":true,"is_agentforce_bot":true,"is_bot":true,"profile":{"api_app_id":"A00000002"}}),
    );
    assert_eq!(
        p.requirements(&u).unwrap(),
        Requirements {
            eligible: false,
            app_profile: false,
            agent_access: false,
            salesforce_environment: false
        }
    );
    assert!(!p.allows(&u, Facts::default()).unwrap());
    assert!(member_base(&user(json!({"is_workflow_bot":7}))).is_err());
    assert!(member_base(&user(json!({"restricted_user_type":false}))).is_err());
}
