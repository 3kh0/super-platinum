use super::*;
use crate::slack::{SlackClient, client::RequestBody};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
const T: &str = "T00000001";
const E: &str = "E00000001";
const U: &str = "U00000001";
#[test]
fn producer_configuration_roles_membership_and_connector_absence_are_explicit() {
    let config = Config::ordinary_client_v2(&json!({"ok":true,"assignments":{}})).unwrap();
    assert_eq!(config.org_unified, Some(true));
    assert_eq!(config.slackforce_pnp_wfb, Some(false));
    assert_eq!(
        Config::decode(&json!({"ok":true,"assignments":{}}), None)
            .unwrap()
            .org_unified,
        None
    );
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"assignments":{"wfb_hermes_trials":null}}),
        json!({"ok":true,"error":false,"assignments":{}}),
    ] {
        assert!(Config::ordinary_client_v2(&raw).is_err());
    }
    let mut raw = boot(T);
    raw["team"]["enterprise_id"] = json!(E);
    raw["workspaces"][0]["enterprise_id"] = json!(E);
    raw["self"]["enterprise_user"] = json!({"is_admin":true,"is_owner":false});
    let mut w = workspace(T);
    let no_self_enterprise = Scope::decode(&w, &raw).unwrap();
    assert!(RoleRead::capture(&no_self_enterprise, config, T).is_err());
    w.enterprise_id = Some(E.into());
    let scope = Scope::decode(&w, &raw).unwrap();
    let role = RoleRead::capture(&scope, config, T).unwrap();
    assert!(
        role.decode(&json!({"ok":true,"user":{"id":U}}))
            .unwrap()
            .is_none()
    );
    let observed = role
        .decode(&json!({"ok":true,"user":{"id":U,"team_user":{"is_owner":true}}}))
        .unwrap()
        .unwrap();
    assert_eq!(
        observed,
        Roles {
            workspace_owner: true,
            workspace_admin: false,
            enterprise_owner: false,
            enterprise_admin: true
        }
    );
    assert!(RoleRead::capture(&scope, Config::default(), T).is_err());
    assert!(RoleRead::capture(&scope, config, "T00000009").is_err());
    let membership = MembershipRead::capture(&scope);
    assert_eq!(
        membership
            .decode(&json!({"ok":true,"usergroups":["S00000001","S00000001"]}))
            .unwrap()
            .len(),
        1
    );
    assert!(
        membership
            .decode(&json!({"ok":false,"error":"user_is_restricted"}))
            .unwrap()
            .is_empty()
    );
    assert!(
        membership
            .decode(&json!({"ok":true,"error":"user_is_restricted"}))
            .is_err()
    );
    assert!(
        membership
            .decode(&json!({"ok":true,"usergroups":[null]}))
            .is_err()
    );
    let extras = json!({"ok":true,"team_prefs":{"toplevel_prefs":{}}});
    let unavailable =
        PreferenceFacts::decode(&scope, &extras, None, BTreeMap::new(), None).unwrap();
    assert!(unavailable.connector_visibility(config).is_err());
    for (locked, unlocked, expected) in [
        (json!({}), json!({}), None),
        (
            json!({}),
            json!({"wfb_default_connector_visibility":"hidden"}),
            Some(json!("hidden")),
        ),
        (
            json!({"wfb_default_connector_visibility":null}),
            json!({"wfb_default_connector_visibility":"hidden"}),
            Some(Value::Null),
        ),
    ] {
        let enterprise = json!({"ok":true,"org_prefs":{"wfb_default_connector_visibility":"ignored"},"locked_prefs":locked,"unlocked_prefs":unlocked});
        let facts =
            PreferenceFacts::decode(&scope, &extras, Some(&enterprise), BTreeMap::new(), None)
                .unwrap();
        assert_eq!(
            facts.connector_visibility(config).unwrap().cloned(),
            expected
        );
    }
    let client = SlackClient::default();
    let mut enterprise = workspace(E);
    enterprise.token = "synthetic-enterprise".into();
    let request = role.request(&client, &w, &enterprise).unwrap();
    let RequestBody::Form(form) = request.body else {
        panic!("form")
    };
    assert!(form.contains(&("team".into(), T.into())));
    assert!(form.contains(&("self".into(), "true".into())));
    assert!(form.contains(&("token".into(), "synthetic-enterprise".into())));
    assert!(role.request(&client, &w, &w).is_err());
    let mut foreign_enterprise = enterprise.clone();
    foreign_enterprise.user_id = "U00000009".into();
    assert!(role.request(&client, &w, &foreign_enterprise).is_err());
    let mut changed = w.clone();
    changed.token.push('x');
    assert!(role.request(&client, &changed, &enterprise).is_err());
    assert!(membership.request(&client, &changed).is_err());
}
fn workspace(team: &str) -> WorkspaceSession {
    WorkspaceSession {
        team_id: team.into(),
        enterprise_id: team.starts_with('E').then(|| E.into()),
        user_id: U.into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn boot(team: &str) -> Value {
    json!({"ok":true,"self":{"id":U,"is_restricted":false},"team":{"id":team,"plan":"pro"},"workspaces":[{"id":T}]})
}
fn config() -> Config {
    Config {
        slackforce_pnp_wfb: Some(false),
        wfb_hermes_trials: Some(false),
        org_unified: Some(false),
    }
}
fn policies(scope: &Scope) -> FeaturePolicies {
    FeaturePolicies::decode(scope,&json!({"ok":true,"feature_policies":{"hermes__read":{"is_enabled":true},"hermes__write":{"is_enabled":true}}})).unwrap()
}
fn prefs(scope: &Scope, value: Value) -> PreferenceFacts {
    PreferenceFacts::decode(
        scope,
        &json!({"ok":true,"team_prefs":{"toplevel_prefs":value,"workspace_prefs":{}}}),
        None,
        BTreeMap::new(),
        Some(BTreeSet::new()),
    )
    .unwrap()
}

#[test]
fn actual_retained_selectors_match_known_policy_cases() {
    let cases: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let state = &case["state"];
        let mut workspace = workspace(state["team"]["id"].as_str().unwrap());
        workspace.enterprise_id = state["selfTeamIds"]["enterpriseId"]
            .as_str()
            .map(str::to_owned);
        let scope=Scope::decode(&workspace,&json!({"ok":true,"self":state["self"],"team":state["team"],"workspaces":state["workspaces"]})).unwrap();
        let config = Config {
            slackforce_pnp_wfb: Some(state["flags"]["slackforce_pnp_wfb"] == "on"),
            wfb_hermes_trials: Some(state["flags"]["wfb_hermes_trials"] == "on"),
            org_unified: state["unified"].as_bool(),
        };
        let policies = FeaturePolicies::decode(
            &scope,
            &json!({"ok":true,"feature_policies":state["featureAccess"]}),
        )
        .unwrap();
        let mut roles = BTreeMap::new();
        for (id, raw) in state["roles"].as_object().unwrap() {
            roles.insert(
                id.clone(),
                Roles {
                    workspace_owner: raw["workspace_owner"] == true,
                    workspace_admin: raw["workspace_admin"] == true,
                    enterprise_owner: raw["enterprise_owner"] == true,
                    enterprise_admin: raw["enterprise_admin"] == true,
                },
            );
        }
        let subteams = Some(
            state["subteams"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().into())
                .collect(),
        );
        let mut enterprise = state["enterprisePrefs"].clone();
        enterprise["ok"] = json!(true);
        let preferences=PreferenceFacts::decode(&scope,&json!({"ok":true,"team_prefs":{"toplevel_prefs":state["teamPrefs"][T],"workspace_prefs":state["teamPrefs"]}}),Some(&enterprise),roles,subteams).unwrap();
        let read = PermissionRead::capture(&scope).unwrap();
        let permission=read.decode(&json!({"ok":true,"allowed":if state["permission"]==true {vec!["CREATE_WORKFLOWS"]}else{vec![]}})).unwrap();
        let access = Access::observe(
            &scope,
            config,
            Some(&policies),
            Some(&preferences),
            Some(&permission),
        );
        assert_eq!(
            access.can_create(),
            Decision::observed(case["result"]["create"] == true),
            "{}",
            case["label"]
        );
        assert_eq!(
            access.can_view_managed(),
            Decision::observed(case["result"]["managed"] == true),
            "{}",
            case["label"]
        );
        assert_eq!(
            access.can_enter_editor(),
            Decision::observed(case["result"]["editor"] == true),
            "{}",
            case["label"]
        );
    }
}

#[test]
fn unloaded_preferences_roles_flags_and_features_are_not_grants() {
    let scope = Scope::decode(&workspace(T), &boot(T)).unwrap();
    let policy = policies(&scope);
    assert_eq!(
        Access::observe(&scope, config(), Some(&policy), None, None).can_create(),
        Decision::Unknown
    );
    let owner = prefs(&scope, json!({"who_can_create_workflows":"owner"}));
    assert_eq!(
        Access::observe(&scope, config(), Some(&policy), Some(&owner), None).can_create(),
        Decision::Unknown
    );
    let empty = prefs(&scope, json!({}));
    assert_eq!(
        Access::observe(&scope, Config::default(), Some(&policy), Some(&empty), None).can_create(),
        Decision::Unknown
    );
    assert_eq!(
        Access::observe(&scope, config(), None, Some(&empty), None).can_enter_editor(),
        Decision::Unknown
    );
    // The source OR really allows an independently confirmed scoped permission.
    let permission = PermissionRead::capture(&scope)
        .unwrap()
        .decode(&json!({"ok":true,"allowed":["CREATE_WORKFLOWS"]}))
        .unwrap();
    assert_eq!(
        Access::observe(
            &scope,
            config(),
            Some(&policy),
            Some(&owner),
            Some(&permission)
        )
        .can_create(),
        Decision::Allowed
    );
}

#[test]
fn missing_inventory_is_unknown_but_observed_empty_is_not() {
    let mut raw = boot(T);
    raw.as_object_mut().unwrap().remove("workspaces");
    let scope = Scope::decode(&workspace(T), &raw).unwrap();
    assert_eq!(prefs(&scope, json!({})).enabled(), Decision::Unknown);
    raw["workspaces"] = json!([]);
    let scope = Scope::decode(&workspace(T), &raw).unwrap();
    assert_eq!(prefs(&scope, json!({})).enabled(), Decision::Denied);
}

#[test]
fn permission_scope_is_current_team_or_exact_internal_enterprise_set() {
    let scope = Scope::decode(&workspace(T), &boot(T)).unwrap();
    assert_eq!(
        PermissionRead::capture(&scope).unwrap().arguments(),
        json!({"user_id":U,"permissions":["CREATE_WORKFLOWS"],"as_admin":true})
    );
    let mut raw = boot(E);
    raw["workspaces"] =
        json!([{"id":T},{"id":"T00000002"},{"id":"T00000003","allow_external_users":true}]);
    let scope = Scope::decode(&workspace(E), &raw).unwrap();
    let read = PermissionRead::capture(&scope).unwrap();
    assert_eq!(
        read.arguments(),
        json!({"user_id":U,"permissions":["CREATE_WORKFLOWS"],"team_ids":[T,"T00000002"]})
    );
    assert_eq!(
        read.decode(&json!({"ok":true,"allowed_per_team":{"T00000003":["CREATE_WORKFLOWS"]}}))
            .unwrap()
            .decision(),
        Decision::Denied
    );
    assert_eq!(
        read.decode(&json!({"ok":true,"allowed_per_team":{"T00000002":["CREATE_WORKFLOWS"]}}))
            .unwrap()
            .decision(),
        Decision::Allowed
    );
    raw["workspaces"] = json!([]);
    let scope = Scope::decode(&workspace(E), &raw).unwrap();
    assert_eq!(
        PermissionRead::capture(&scope).unwrap().arguments()["as_admin"],
        true
    );
}

#[test]
fn permissions_use_observed_endpoint_and_expire_before_ttl() {
    let w = workspace(T);
    let scope = Scope::decode(&w, &boot(T)).unwrap();
    let read = PermissionRead::capture(&scope).unwrap();
    let now = Instant::now();
    let discovery =
        json!({"ok":true,"ttl_seconds":3,"url":format!("https://edgeapi.slack.com/cache/{T}")});
    let c = SlackClient::default();
    let request = read
        .request(&c, &w, &discovery, now, now + Duration::from_millis(499))
        .unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        format!("/cache/{T}/permissions/info")
    );
    let RequestBody::Json(body) = request.body else {
        panic!("JSON")
    };
    assert_eq!(
        body,
        json!({"token":"synthetic","user_id":U,"permissions":["CREATE_WORKFLOWS"],"as_admin":true})
    );
    assert!(
        read.request(&c, &w, &discovery, now, now + Duration::from_millis(500))
            .is_err()
    );
    let mut changed = w.clone();
    changed.token = "replacement".into();
    assert!(read.request(&c, &changed, &discovery, now, now).is_err());
    for endpoint in [
        "https://foreign.test/cache/T00000001",
        "https://edgeapi.slack.com/cache/T00000002",
        "https://edgeapi.slack.com/cache/T00000001?redirect=1",
    ] {
        assert!(
            read.request(
                &c,
                &w,
                &json!({"ok":true,"ttl_seconds":60,"url":endpoint}),
                now,
                now
            )
            .is_err()
        );
    }
}

#[test]
fn contradictory_envelopes_and_cross_scope_snapshots_are_refused() {
    let scope = Scope::decode(&workspace(T), &boot(T)).unwrap();
    let read = PermissionRead::capture(&scope).unwrap();
    for error in [json!(false), json!(""), json!({}), json!("ratelimited")] {
        assert!(
            FeaturePolicies::decode(
                &scope,
                &json!({"ok":true,"error":error,"feature_policies":{}})
            )
            .is_err()
        );
        assert!(
            read.decode(&json!({"ok":true,"error":error,"allowed":[]}))
                .is_err()
        );
        assert!(
            Scope::decode(
                &workspace(T),
                &json!({"ok":true,"error":error,"self":{"id":U},"team":{"id":T}})
            )
            .is_err()
        );
    }
    let mut w = workspace(T);
    w.token = "new".into();
    let other = Scope::decode(&w, &boot(T)).unwrap();
    assert!(Facts::capture(other, config(), Some(policies(&scope)), None, None).is_err());
    let facts = Facts::capture(
        scope.clone(),
        config(),
        Some(policies(&scope)),
        Some(prefs(&scope, json!({}))),
        None,
    )
    .unwrap();
    assert!(facts.same_inputs(&facts.clone()));
    assert!(!facts.current(&w, config()));
    let changed = Facts::capture(
        scope.clone(),
        config(),
        Some(policies(&scope)),
        Some(prefs(&scope, json!({"workflow_builder_enabled":false}))),
        None,
    )
    .unwrap();
    assert!(!facts.same_inputs(&changed));
}

#[test]
fn closed_read_requests_keep_exact_wire_fields() {
    let client = SlackClient::default();
    let workspace = workspace(T);
    let request = PolicyReads::features(&client, &workspace);
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/features.access.policies.list"
    );
    let RequestBody::Form(body) = request.body else {
        panic!("form")
    };
    assert_eq!(
        body,
        vec![
            ("token".into(), "synthetic".into()),
            ("_x_reason".into(), "native-workflow-capabilities".into())
        ]
    );
    let request = PolicyReads::team_preferences(&client, &workspace);
    let RequestBody::Form(body) = request.body else {
        panic!("form")
    };
    assert_eq!(body[1], ("extras".into(), "[\"team_prefs\"]".into()));
}
