use super::*;
use serde_json::json;

fn strings(value: &Value) -> Vec<String> {
    serde_json::from_value(value.clone()).unwrap()
}
fn input<'a>(internal: &'a [String]) -> Inputs<'a> {
    Inputs {
        unified: Some(true),
        channel_override: None,
        internal_workspace_ids: Some(internal),
        bot_enterprise_teams: None,
        active_team_id: None,
        workspace_filter_selected: None,
        default_workspace_id: None,
    }
}

#[test]
fn retained_original_nine_selector_vectors_match_order_and_context() {
    let reference: Value = serde_json::from_str(include_str!("reference.json")).unwrap();
    let vectors = reference["selector"].as_array().unwrap();
    assert_eq!(vectors.len(), 9);
    for vector in vectors {
        let source = &vector["input"];
        let ids = &source["selfTeamIds"];
        let internal = strings(&ids["internalWorkspaceIds"]);
        let bot = source["channels"]["DHOME001"]["user"]
            .as_str()
            .and_then(|user| source["members"][user]["enterprise_user"]["teams"].as_array())
            .map(|rows| strings(&Value::Array(rows.clone())));
        let selected = source["selectedFilter"]["workspace"].get("id");
        let mut inputs = input(&internal);
        inputs.unified = vector["isUnified"].as_bool();
        inputs.channel_override = source["appHomeContextTeam"]["DHOME001"].as_str();
        inputs.bot_enterprise_teams = bot.as_deref();
        inputs.active_team_id = ids["teamId"].as_str();
        inputs.default_workspace_id = ids["defaultWorkspaceId"].as_str();
        inputs.workspace_filter_selected = selected;
        assert_eq!(
            candidates(&internal, bot.as_deref()).unwrap(),
            strings(&vector["possible"]),
            "{}",
            vector["name"]
        );
        let actual = select(&inputs).unwrap();
        assert_eq!(
            actual.team_id(),
            vector["context"].as_str(),
            "{}",
            vector["name"]
        );
        assert_eq!(
            matches!(actual, Selection::NonUnified),
            inputs.unified == Some(false)
        );
    }
}

#[test]
fn retained_raw_filter_vectors_do_not_parse_or_coerce_preferences() {
    let reference: Value = serde_json::from_str(include_str!("reference.json")).unwrap();
    let internal = strings(&json!(["TWORK0001", "TWORK0002", "TWORK0003"]));
    let bot = strings(&json!(["TWORK0002", "TWORK0003"]));
    let vectors = reference["filters"].as_array().unwrap();
    assert_eq!(vectors.len(), 7);
    for vector in vectors {
        let mut inputs = input(&internal);
        inputs.bot_enterprise_teams = Some(&bot);
        inputs.active_team_id = Some("EORG00001");
        inputs.default_workspace_id = Some("TWORK0003");
        inputs.workspace_filter_selected = if vector["prefAbsent"] == true {
            None
        } else {
            vector
                .get("pref")
                .or_else(|| vector["filter"]["workspace"].get("id"))
        };
        assert_eq!(
            select(&inputs).unwrap().team_id(),
            vector["context"].as_str(),
            "{}",
            vector["name"]
        );
    }
    for raw in [
        json!(["TWORK0002"]),
        json!("\"TWORK0002\""),
        json!(true),
        json!(2),
        json!({"id":"TWORK0002"}),
    ] {
        let mut inputs = input(&internal);
        inputs.workspace_filter_selected = Some(&raw);
        inputs.default_workspace_id = Some("TWORK0003");
        assert_eq!(select(&inputs).unwrap().team_id(), Some("TWORK0003"));
    }
}

#[test]
fn ordered_intersection_fallback_and_active_validation_are_distinct() {
    let internal = strings(&json!(["TWORK0003", "TWORK0001", "TWORK0002", "TWORK0003"]));
    let bot = strings(&json!(["TWORK0002", "TWORK0003"]));
    assert_eq!(
        candidates(&internal, Some(&bot)).unwrap(),
        ["TWORK0003", "TWORK0002"]
    );
    for bot in [None, Some(&[][..])] {
        assert_eq!(
            candidates(&internal, bot).unwrap(),
            ["TWORK0003", "TWORK0001", "TWORK0002"]
        );
    }
    let odd = strings(&json!(["Tshort", "Tlower001", "EORG00001", "TWORK0002"]));
    for active in ["Tshort", "Tlower001", "EORG00001", "TUNKNOWN1"] {
        let mut inputs = input(&odd);
        inputs.active_team_id = Some(active);
        inputs.default_workspace_id = Some("TWORK0002");
        assert_eq!(select(&inputs).unwrap().team_id(), Some("TWORK0002"));
    }
}

#[test]
fn missing_observations_empty_candidates_and_override_precedence_do_not_conflate() {
    let mut inputs = input(&[]);
    assert_eq!(
        select(&inputs).unwrap(),
        Selection::Unified { team_id: None }
    );
    inputs.internal_workspace_ids = None;
    assert!(select(&inputs).is_err());
    inputs.channel_override = Some("TOVERRIDE");
    assert_eq!(select(&inputs).unwrap().team_id(), Some("TOVERRIDE"));
    inputs.unified = None;
    assert!(select(&inputs).is_err());
    inputs.unified = Some(false);
    assert_eq!(select(&inputs).unwrap(), Selection::NonUnified);
    inputs.unified = Some(true);
    inputs.channel_override = Some("");
    assert!(select(&inputs).is_err());
    for bad in [" ", "T\nOTHER", &"T".repeat(256)] {
        inputs.channel_override = Some(bad);
        assert!(select(&inputs).is_err());
    }
    assert!(candidates(&["T VALID".into()], None).is_err());
}
