use super::*;

#[test]
fn explicit_mode_and_home_service_context_produce_original_additional_fields() {
    for (enterprise, team, expected) in [
        (false, "TCONTEXT1", json!({})),
        (false, "E12345678", json!({})),
        (
            true,
            "TCONTEXT1",
            json!({"allowed_internal_teams":["TCONTEXT1"],"should_allowed_internal_teams_match_any":true}),
        ),
        (true, "E12345678", json!({})),
        (
            true,
            "E123",
            json!({"allowed_internal_teams":["E123"],"should_allowed_internal_teams_match_any":true}),
        ),
    ] {
        let policy = ChannelTeamPolicy::new(enterprise, team).unwrap();
        assert_eq!(policy.request_fields(), expected);
        assert!(policy.request_fields().get("external_team").is_none());
        assert_eq!(
            policy.match_any(),
            expected
                .get("should_allowed_internal_teams_match_any")
                .is_some()
        );
    }
    for id in ["", " UTEAM001", "UTEAM001", "Tlower", "T\n", "E_12345678"] {
        assert!(ChannelTeamPolicy::new(true, id).is_err());
    }
}

#[test]
fn canonical_internal_alias_precedence_and_global_org_exception_match_original() {
    let policy = ChannelTeamPolicy::new(true, "TCONTEXT1").unwrap();
    assert_eq!(policy.allowed_internal_teams().unwrap(), &["TCONTEXT1"]);
    let cases = [
        (json!({"internalTeamIds":["TOTHER001","TCONTEXT1"]}), true),
        (json!({"internal_team_ids":["TCONTEXT1"]}), true),
        (
            json!({"internalTeamIds":null,"internal_team_ids":["TCONTEXT1"]}),
            true,
        ),
        (
            json!({"internalTeamIds":[],"internal_team_ids":["TCONTEXT1"]}),
            false,
        ),
        (
            json!({"internalTeamIds":["TOTHER001"],"internal_team_ids":["TCONTEXT1"]}),
            false,
        ),
        (json!({"connectedTeamIds":["TCONTEXT1"]}), false),
        (json!({"is_org_shared":true,"is_global_shared":true}), true),
        (json!({"is_global_shared":true}), false),
        (json!({"is_org_shared":true}), false),
        (json!({"is_org_shared":true,"is_im":true}), true),
        (json!({"is_org_shared":true,"is_mpim":true}), true),
        (json!({}), false),
    ];
    for (row, allowed) in cases {
        assert_eq!(policy.allows(&row), allowed, "{row}");
    }
    // IM/private/archive admission remains the separate current public source
    // filter; this helper grants only an additional team-restriction match.
    assert!(
        ChannelTeamPolicy::new(false, "TCONTEXT1")
            .unwrap()
            .allows(&json!({}))
    );
    assert!(
        ChannelTeamPolicy::new(true, "E12345678")
            .unwrap()
            .allows(&json!({}))
    );
}

#[test]
fn malformed_metadata_and_context_replacement_do_not_broaden_team_match() {
    let first = ChannelTeamPolicy::new(true, "TCONTEXT1").unwrap();
    let next = ChannelTeamPolicy::new(true, "TCONTEXT2").unwrap();
    let row = json!({"internalTeamIds":["TCONTEXT1"]});
    assert!(first.allows(&row));
    assert!(!next.allows(&row));
    assert_ne!(first, next);
    for row in [
        Value::Null,
        json!([]),
        json!({"internalTeamIds":"TCONTEXT1"}),
        json!({"internalTeamIds":["TCONTEXT1",null]}),
        json!({"internalTeamIds":["TCONTEXT1"],"is_org_shared":"true"}),
    ] {
        assert!(!first.allows(&row), "{row}");
    }
}
