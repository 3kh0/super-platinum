use super::*;
struct TestAdopter {
    owner: Owner,
    facts: Facts,
    seen: usize,
    fail: Option<usize>,
    stale: bool,
}
impl Adopter for TestAdopter {
    fn current(&self, owner: &Owner) -> bool {
        !self.stale
            && owner.matches(
                &self.owner.account,
                &self.owner.session,
                &self.owner.cookie,
                self.owner.epoch,
            )
    }
    fn facts(&self) -> Facts {
        self.facts.clone()
    }
    fn apply(&mut self, e: &Effect) -> Result<(), Error> {
        if self.fail == Some(self.seen) {
            return Err(Error::Unsupported(e.kind));
        }
        self.seen += 1;
        if e.kind == Kind::InternalWorkspaces {
            self.facts.workspace_ids =
                Some(serde_json::from_value(e.payload["internalIds"].clone()).unwrap());
        }
        Ok(())
    }
}
fn owner() -> Owner {
    Owner::capture(
        "account",
        &crate::config::WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "test".into(),
            url: "https://example.slack.com".into(),
            token: "fixture-token".into(),
        },
        "fixture-cookie",
        7,
    )
}
fn facts(v: &Value) -> Facts {
    Facts {
        team: v["team"].as_str().map(str::to_owned),
        enterprise: v["enterprise"].as_str().map(str::to_owned),
        workspace_ids: serde_json::from_value(v["workspace_ids"].clone()).ok(),
        auth_workspace_ids: serde_json::from_value(v["auth_workspace_ids"].clone()).ok(),
        temp_mute_enabled: v["temp_mute_enabled"].as_bool(),
        unix_seconds: v["unix_seconds"].as_f64(),
        screen_reader_enabled: v["screen_reader_enabled"].as_bool(),
    }
}
#[test]
fn retained_init_and_extras_dispatch_order_and_partial_failures() {
    let fixture: Value = serde_json::from_str(include_str!("cases.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let kind = if case["kind"] == "init" {
            PayloadKind::Init
        } else {
            PayloadKind::Extras
        };
        let original = case["input_bytes"].as_str().unwrap().to_string();
        let payload = Payload::observed(kind, owner(), original.clone())
            .map_err(|e| e.0)
            .unwrap();
        let mut adopter = TestAdopter {
            owner: owner(),
            facts: facts(&case["observations"]),
            seen: 0,
            fail: case["fail_at"].as_u64().map(|n| n as usize),
            stale: false,
        };
        let result = payload.dispatch(&mut adopter);
        assert_eq!(
            serde_json::to_value(result.applied()).unwrap(),
            case["effects"],
            "{}",
            case["name"]
        );
        assert_eq!(
            result.result().is_err(),
            case["failed"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(result.original_bytes(), original);
        assert_eq!(
            result.is_fatal(),
            kind == PayloadKind::Init && case["failed"] == true
        );
        if case["failed"] == true {
            assert!(result.failed_effect().is_some());
        }
    }
}
#[test]
fn current_original_owner_and_unknown_facts_never_publish() {
    let bytes = r#"{"ok":true,"self":{"id":"U1"},"team":{"id":"T1"}}"#.to_string();
    let mut adopter = TestAdopter {
        owner: owner(),
        facts: Facts::default(),
        seen: 0,
        fail: None,
        stale: true,
    };
    let result = Payload::observed(PayloadKind::Init, owner(), bytes.clone())
        .map_err(|e| e.0)
        .unwrap()
        .dispatch(&mut adopter);
    assert_eq!(result.result(), &Err(Error::Superseded));
    assert!(result.applied().is_empty());
    adopter.stale = false;
    let result = Payload::observed(PayloadKind::Init, owner(), bytes.clone())
        .map_err(|e| e.0)
        .unwrap()
        .dispatch(&mut adopter);
    assert_eq!(
        result.result(),
        &Err(Error::Unavailable("screen reader observation"))
    );
    assert_eq!(result.applied().len(), 1);
    assert_eq!(result.original_bytes(), bytes);
    let mut changed = owner().session;
    changed.token = "replaced".into();
    assert!(!owner().matches("account", &changed, "fixture-cookie", 7));
    assert!(!owner().matches("account", &owner().session, "replaced", 7));
}
#[test]
fn critical_reducers_preserve_merge_replace_and_unsupported_boundaries() {
    let fixture: Value = serde_json::from_str(include_str!("cases.json")).unwrap();
    for case in fixture["reducers"].as_array().unwrap() {
        let mut stores = Stores::default();
        let (kind, actual) = match case["key"].as_str().unwrap() {
            "PZ" => {
                stores.user_prefs =
                    serde_json::from_value(case["initial"]["user"].clone()).unwrap();
                (Kind::UserPrefs, "user")
            }
            "Gk" => {
                stores.team_prefs =
                    serde_json::from_value(case["initial"]["team"].clone()).unwrap();
                (Kind::TeamPrefs, "team")
            }
            "dX" => {
                stores.user_group_membership =
                    serde_json::from_value(case["initial"].clone()).unwrap();
                (Kind::Subteams, "")
            }
            "b1" => (Kind::InternalWorkspaces, "internalWorkspaceIds"),
            "xL" => (Kind::DefaultWorkspace, "defaultWorkspaceId"),
            _ => unreachable!(),
        };
        stores
            .apply(&Effect {
                kind,
                payload: case["payload"].clone(),
            })
            .unwrap();
        let value = match kind {
            Kind::UserPrefs => json!(stores.user_prefs),
            Kind::TeamPrefs => json!(stores.team_prefs),
            Kind::Subteams => json!(stores.user_group_membership),
            Kind::InternalWorkspaces => stores.internal_workspace_ids.clone().unwrap(),
            Kind::DefaultWorkspace => stores.default_workspace.clone().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(
            value,
            if actual.is_empty() {
                case["result"].clone()
            } else {
                case["result"][actual].clone()
            }
        );
    }
    let mut stores = Stores::default();
    assert_eq!(
        stores.apply(&Effect {
            kind: Kind::TeamUpsert,
            payload: json!({})
        }),
        Err(Error::Unsupported(Kind::TeamUpsert))
    );
}
#[test]
fn envelope_refusal_preserves_bytes_and_own_key_order() {
    for bytes in [
        r#"{"ok":true,"error":false}"#,
        r#"{"ok":false}"#,
        r#"{"ok":true,1:2}"#,
    ] {
        let (_, original) = Payload::observed(PayloadKind::Extras, owner(), bytes.into())
            .err()
            .unwrap();
        assert_eq!(original, bytes);
    }
    let bytes = r#"{"z":0,"10":1,"2":2,"a":3,"z":4}"#;
    let node: wire::Node = serde_json::from_str(bytes).unwrap();
    assert_eq!(
        node.entries()
            .unwrap()
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        ["2", "10", "z", "a"]
    );
    assert_eq!(node.get("z").unwrap().value(), json!(4));
}

#[test]
fn source_mute_short_circuit_number_strings_and_empty_enterprise() {
    let cases: Value = serde_json::from_str(include_str!("edge-cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let bytes = case["input_bytes"].as_str().unwrap().to_string();
        let mut adopter = TestAdopter {
            owner: owner(),
            facts: facts(&case["observations"]),
            seen: 0,
            fail: None,
            stale: false,
        };
        let settled = Payload::observed(PayloadKind::Init, owner(), bytes.clone())
            .map_err(|e| e.0)
            .unwrap()
            .dispatch(&mut adopter);
        assert_eq!(settled.result(), &Ok(()), "{}", case["name"]);
        assert_eq!(
            serde_json::to_value(settled.applied()).unwrap(),
            case["effects"],
            "{}",
            case["name"]
        );
        assert_eq!(settled.original_bytes(), bytes);
    }
}

#[test]
fn scalar_numeric_strings_match_ecmascript_bits_and_nan() {
    let cases: Value = serde_json::from_str(include_str!("number-cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let n = prefs::number(&case["input"]).unwrap();
        if case["bits"] == "nan" {
            assert!(n.is_nan(), "{}", case["input"]);
        } else {
            assert_eq!(
                format!("{:016x}", n.to_bits()),
                case["bits"].as_str().unwrap(),
                "{}",
                case["input"]
            );
        }
    }
}

#[test]
fn init_expands_role_thunks_and_observes_screen_reader_separately() {
    let mut adopter = TestAdopter {
        owner: owner(),
        facts: Facts {
            team: Some("T00000001".into()),
            enterprise: Some("E00000001".into()),
            screen_reader_enabled: Some(true),
            ..Facts::default()
        },
        seen: 0,
        fail: None,
        stale: false,
    };
    let raw = json!({"ok":true,"self":{"id":"U00000001","team_id":"T00000001",
        "is_primary_owner":true,"is_owner":true,"is_admin":false,
        "enterprise_user":{"is_admin":true}},"team":{"id":"T00000001"}});
    let settled = Payload::observed(PayloadKind::Init, owner(), raw.to_string())
        .map_err(|e| e.0)
        .unwrap()
        .dispatch(&mut adopter);
    assert_eq!(settled.result(), &Ok(()));
    assert_eq!(
        settled.applied().iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            Kind::SelfId,
            Kind::ScreenReaderEnabled,
            Kind::TeamUpsert,
            Kind::MemberUpsert,
            Kind::MemberPrimaryOwner,
            Kind::PrimaryOwner,
            Kind::Owner,
            Kind::Admin,
            Kind::EnterprisePrimaryOwner,
            Kind::EnterpriseOwner,
            Kind::EnterpriseAdmin,
        ]
    );
    assert_eq!(settled.applied()[1].payload, true);
    let mut stores = Stores::default();
    for effect in &settled.applied()[4..] {
        stores.apply(effect).unwrap();
    }
    assert_eq!(
        stores.member_roles["T00000001"]["U00000001"],
        json!({"isPrimaryOwner":true})
    );
    assert_eq!(
        stores.workspace_roles["T00000001"],
        json!({"id":"T00000001","isWorkspacePrimaryOwner":true,"isWorkspaceOwner":true,"isWorkspaceAdmin":false})
    );
    assert_eq!(
        stores.enterprise_roles["E00000001"],
        json!({"isEnterprisePrimaryOwner":false,"isEnterpriseOwner":false,"isEnterpriseAdmin":true})
    );
}

#[test]
fn member_role_replacement_and_explicit_clear_do_not_rewrite_self_data() {
    let mut stores = Stores::default();
    let payload = json!({"memberId":"U1","teamId":"T1"});
    for kind in [Kind::MemberPrimaryOwner, Kind::MemberAdmin] {
        stores
            .apply(&Effect {
                kind,
                payload: payload.clone(),
            })
            .unwrap();
    }
    assert_eq!(stores.member_roles["T1"]["U1"], json!({"isAdmin":true}));
    stores
        .apply(&Effect {
            kind: Kind::MemberRoleClear,
            payload,
        })
        .unwrap();
    assert!(!stores.member_roles["T1"].contains_key("U1"));
    assert!(stores.workspace_roles.is_empty());
    for user in [
        json!({"id":"U1"}),
        json!({"id":"U1","enterprise_id":"E00000001","is_admin":true}),
    ] {
        let mut adopter = TestAdopter {
            owner: owner(),
            facts: Facts {
                screen_reader_enabled: Some(false),
                ..Facts::default()
            },
            seen: 0,
            fail: None,
            stale: false,
        };
        let result = Payload::observed(
            PayloadKind::Init,
            owner(),
            json!({"ok":true,"self":user}).to_string(),
        )
        .map_err(|e| e.0)
        .unwrap()
        .dispatch(&mut adopter);
        assert_eq!(result.result(), &Ok(()));
        assert!(!result.applied().iter().any(|e| matches!(
            e.kind,
            Kind::MemberRoleClear
                | Kind::MemberAdmin
                | Kind::MemberOwner
                | Kind::MemberPrimaryOwner
        )));
    }
}
