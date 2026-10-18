use super::*;
use crate::composer_emoji::ranking::SimilarityCache;
fn owner() -> crate::config::WorkspaceSession {
    crate::config::WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000999".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
fn oracle() -> Value {
    serde_json::from_str(include_str!("oracle.json")).unwrap()
}
fn scored(rows: &[Value]) -> Vec<local::Scored<'_>> {
    rows.iter()
        .enumerate()
        .map(|(i, item)| local::Scored {
            item,
            id: item["id"].as_str().unwrap(),
            score: i as u32,
            score_details: None,
            is_exact_match: None,
            exact_match_details: None,
            is_self: false,
            is_flannel_fuzzy_passthrough: i == 0,
        })
        .collect()
}
#[test]
fn actual_renderer_receipt_and_source_toggle_change_only_observed_order() {
    for case in oracle()["cases"].as_array().unwrap() {
        let query = Query::capture(&owner(), case["query"].as_str().unwrap()).unwrap();
        let rows: Vec<_> = case["raw"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| local::derive_member_names(r, str::to_lowercase).unwrap())
            .collect();
        let matched = scored(&rows);
        let names: Vec<String> = serde_json::from_value(case["comparisonNames"].clone()).unwrap();
        let reply: RendererReply = serde_json::from_value(case["reply"].clone()).unwrap();
        let plan = ObservationPlan::capture(
            &reply.generation,
            &query,
            SearchPath::Local,
            &matched,
            &names,
        )
        .unwrap();
        assert_eq!(json!(plan.normalization_fields()), case["fields"]);
        assert_eq!(json!(plan.collation_names()), case["names"]);
        // Distinct first/last fields are mandatory even when the display name
        // happens to match another candidate or the model has zero weights.
        for name in ["Robert", "Separate", "Alicia", "Distinct"] {
            assert!(plan.normalization_fields().iter().any(|s| s == name));
        }
        let locale = reply.locale.clone();
        let obs = plan.decode(&locale, reply).unwrap();
        let history = History::from_wire("{}").unwrap();
        let model = Model::from_weights(BTreeMap::from([(
            "IS_FLANNEL_FUZZY_PASSTHROUGH".into(),
            100.0,
        )]))
        .unwrap();
        let input = RankInputs {
            state: &case["state"],
            history: &history,
            model: &model,
            now_ms: 1760000000000.0,
        };
        let order = rank_members(
            &query,
            SearchPath::Local,
            &matched,
            &names,
            &input,
            &obs,
            &mut SimilarityCache::default(),
        )
        .unwrap();
        let ids: Vec<_> = order.iter().map(|i| matched[*i].id).collect();
        assert_eq!(json!(ids), case["ids"]);
    }
}
#[test]
fn missing_fields_stale_receipts_and_incomplete_collation_refuse() {
    let data = oracle();
    let case = &data["cases"][0];
    let query = Query::capture(&owner(), "ali").unwrap();
    let rows = case["raw"].as_array().unwrap();
    let matched = scored(rows);
    let names: Vec<String> = serde_json::from_value(case["comparisonNames"].clone()).unwrap();
    let original: RendererReply = serde_json::from_value(case["reply"].clone()).unwrap();
    for variant in 0..7 {
        let plan = ObservationPlan::capture(
            &original.generation,
            &query,
            SearchPath::Local,
            &matched,
            &names,
        )
        .unwrap();
        let mut reply = original.clone();
        match variant {
            0 => {
                reply.lowered.pop();
            }
            1 => reply.generation = "stale".into(),
            2 => reply.locale = "changed".into(),
            3 => reply.collation_locale = "tr".into(),
            4 => {
                reply.classes.pop();
            }
            5 => reply.classes = vec![(0, 0), (0, 1)],
            _ => reply.classes = vec![(0, 0), (1, 2)],
        }
        assert!(plan.decode(&original.locale, reply).is_err());
    }
}
#[test]
fn changed_query_rows_and_names_cannot_reuse_observation_plan() {
    let data = oracle();
    let case = &data["cases"][0];
    let query = Query::capture(&owner(), "ali").unwrap();
    let rows = case["raw"].as_array().unwrap();
    let matched = scored(rows);
    let names: Vec<String> = serde_json::from_value(case["comparisonNames"].clone()).unwrap();
    let reply: RendererReply = serde_json::from_value(case["reply"].clone()).unwrap();
    let locale = reply.locale.clone();
    let plan = ObservationPlan::capture(
        &reply.generation,
        &query,
        SearchPath::Local,
        &matched,
        &names,
    )
    .unwrap();
    let obs = plan.decode(&locale, reply).unwrap();
    let retained = super::super::ordering::rows(&matched, &names).unwrap();
    assert!(obs.validate("ali", &retained).is_ok());
    assert!(obs.validate("alice", &retained).is_err());
    let mut changed = retained.clone();
    changed[0]["item"]["profile"]["first_name"] = json!("Replacement");
    assert!(obs.validate("ali", &changed).is_err());
    let mut renamed = names;
    renamed[0] = "Renamed".into();
    assert!(
        obs.validate(
            "ali",
            &super::super::ordering::rows(&matched, &renamed).unwrap()
        )
        .is_err()
    );
    assert!(obs.normalize("uncaptured").is_err());
    assert_eq!(obs.compare("uncaptured", "Alice Doe"), None);
}
#[test]
fn newline_segments_and_empty_comparison_fallback_are_explicit() {
    let query = Query::capture(&owner(), "@@Ada").unwrap();
    let rows = vec![
        json!({"id":"U00000001","name":"Raw\n", "profile":{"display_name":"Shown\nSecond","first_name":"SeparateFirst","last_name":"SeparateLast"}}),
    ];
    let matched = scored(&rows);
    let plan =
        ObservationPlan::capture("g", &query, SearchPath::Remote, &matched, &[String::new()])
            .unwrap();
    for s in [
        "@Ada",
        "Ada",
        "Raw\n",
        "Raw",
        "",
        "Shown\nSecond",
        "Shown",
        "Second",
        "SeparateFirst",
        "SeparateLast",
    ] {
        assert!(plan.normalization_fields().iter().any(|v| v == s), "{s:?}");
    }
    assert_eq!(plan.collation_names(), ["Raw\n"]);
}

#[test]
fn actual_numeric_collation_ties_preserve_observed_equivalence_classes() {
    let data = oracle();
    let names: Vec<String> = serde_json::from_value(data["tieNames"].clone()).unwrap();
    let rows: Vec<_> = names.iter().enumerate().map(|(i, name)| json!({"id":format!("U{i:08}"),"name":name,"profile":{"display_name":name}})).collect();
    let matched = scored(&rows);
    let query = Query::capture(&owner(), "").unwrap();
    let plan =
        ObservationPlan::capture("ties", &query, SearchPath::Local, &matched, &names).unwrap();
    assert_eq!(plan.collation_names(), names);
    let reply = RendererReply {
        generation: "ties".into(),
        locale: data["ties"]["locale"].as_str().unwrap().into(),
        collation_locale: data["ties"]["collationLocale"].as_str().unwrap().into(),
        lowered: plan
            .normalization_fields()
            .iter()
            .map(|s| s.to_lowercase())
            .collect(),
        classes: serde_json::from_value(data["ties"]["classes"].clone()).unwrap(),
        demote_passthrough: true,
    };
    let locale = reply.locale.clone();
    let observed = plan.decode(&locale, reply).unwrap();
    assert_eq!(observed.compare("Name2", "Name02"), Some(Ordering::Equal));
    assert_eq!(observed.compare("Name10", "Name2"), Some(Ordering::Greater));
    assert_eq!(observed.compare("", "\u{200d}"), Some(Ordering::Less));
}
