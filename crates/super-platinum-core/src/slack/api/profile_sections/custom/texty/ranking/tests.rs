//! Retained factory case 2 is the channel-only 90 -> 75 -> 25 oracle.
//! Test fixtures expand its selector facts into explicit canonical raw fields;
//! this is not a production default for absent source observations.
use super::super::matching::{self, ChannelName};
use super::*;
use crate::block_actions::home::workflow::directory_local as local;
use std::collections::BTreeSet;

fn raw(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"is_channel":true,"is_member":true,"fromAnotherTeam":false})
}

fn list_matched<'a>(query: &str, rows: &'a [Value]) -> Result<Vec<Matched<'a>>, Error> {
    matching::list_channels(
        query,
        &local::PreparedQuery {
            normalized: query.into(),
            matcher_normalized: query.into(),
        },
        rows,
        &BTreeSet::new(),
        false,
        names,
        |s| Ok(s.to_lowercase()),
    )
}
fn list_rank(query: &str, rows: &[Matched<'_>], state: &Value) -> Result<Observation, Error> {
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    rank_list_channels(
        query,
        rows,
        &Context {
            state,
            history: &history,
            model: &model,
            now_ms: 1760000000000.0,
            normalize: &|s| Ok(s.to_lowercase()),
        },
        &mut SimilarityCache::default(),
        |a, b| Ok(a.cmp(b)),
        false,
    )
}

#[test]
fn list_caller_includes_private_other_team_and_previous_names() {
    let mut private = raw("G12345678", "zzzzzz");
    private["is_private"] = json!(true);
    private["fromAnotherTeam"] = json!(true);
    private["previous_names"] = json!(["mpdm-ignore", "alpha"]);
    private["opaque"] = json!({"source":"retain"});
    let rows = vec![
        private,
        raw("C12345678", "alpha"),
        raw("C12345678", "alpha"),
    ];
    let before = rows.clone();
    let matched = list_matched("alpha", &rows).unwrap();
    assert_eq!(
        matched.iter().map(Matched::id).collect::<Vec<_>>(),
        ["C12345678", "C12345678", "G12345678"]
    );
    assert_eq!(matched[2].previous_match(), Some("alpha"));
    assert_eq!(matched[2].raw(), &rows[0]);
    assert!(std::ptr::eq(matched[2].raw(), &rows[0]));
    let result = list_rank("alpha", &matched, &state(&rows)).unwrap();
    assert_eq!(result.rows.len(), 3);
    assert!(result.rows.iter().any(|r| r.row["id"] == "G12345678"));
    assert!(
        rank("alpha", &matched, &state(&rows)).is_err(),
        "Profile cannot accept List-only locality"
    );
    assert_eq!(rows, before);
}

#[test]
fn list_locality_is_observed_not_inferred_and_special_categories_remain_unavailable() {
    let mut other = raw("C12345678", "alpha");
    other["fromAnotherTeam"] = json!(true);
    assert!(ordinary_list_channel(&other));
    assert!(!ordinary_local(&other));
    for value in [Value::Null, json!("false"), json!(0), json!({})] {
        let mut row = other.clone();
        row["fromAnotherTeam"] = value;
        assert!(!ordinary_list_channel(&row));
        assert!(list_matched("", &[row]).is_err());
    }
    let mut missing = other.clone();
    missing.as_object_mut().unwrap().remove("fromAnotherTeam");
    missing["team_id"] = json!("T_CURRENT");
    assert!(!ordinary_list_channel(&missing));
    for patch in [
        json!({"id":"C_bad"}),
        json!({"isUnknown":true}),
        json!({"is_im":true}),
        json!({"properties":{"record_channel":{"record_id":"R1"}}}),
        json!({"name":"mc:meeting"}),
    ] {
        let mut row = other.clone();
        row.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(!ordinary_list_channel(&row));
    }
}

#[test]
fn list_empty_query_keeps_shared_75_then_25_cap_and_profile_compatibility() {
    let rows: Vec<_> = (0..90)
        .map(|i| raw(&format!("C{i:08}"), &format!("alpha{i:03}")))
        .collect();
    let source = state(&rows);
    let profile = rank("", &matched("", &rows), &source).unwrap();
    let list = list_rank("", &list_matched("", &rows).unwrap(), &source).unwrap();
    assert_eq!(list.latest_ids, profile.latest_ids);
    assert_eq!(
        list.rows
            .iter()
            .map(|r| (&r.row, r.original_index))
            .collect::<Vec<_>>(),
        profile
            .rows
            .iter()
            .map(|r| (&r.row, r.original_index))
            .collect::<Vec<_>>()
    );
    let mut other = rows.clone();
    for row in &mut other {
        row["fromAnotherTeam"] = json!(true);
    }
    let cross = list_rank("", &list_matched("", &other).unwrap(), &state(&other)).unwrap();
    assert_eq!(cross.latest_ids.len(), 75);
    assert_eq!(cross.rows.len(), 25);
    assert!(cross.rows.iter().all(|r| r.original_index < 90));
}
fn names(row: &Value) -> Result<ChannelName, Error> {
    let name = row["name"].as_str().unwrap().to_lowercase();
    Ok(ChannelName {
        comparison_lowercase: name.clone(),
        normalized: name,
    })
}
fn matched<'a>(query: &str, rows: &'a [Value]) -> Vec<Matched<'a>> {
    matching::channels(
        query,
        &local::PreparedQuery {
            normalized: query.into(),
            matcher_normalized: query.into(),
        },
        rows,
        &BTreeSet::new(),
        false,
        names,
        |s| Ok(s.to_lowercase()),
    )
    .unwrap()
}
fn state(rows: &[Value]) -> Value {
    let mut state = json!({"selfId":"U_SELF","teamId":"T1","enterprise":false,
        "metadata":{},"navigationHistory":{},"experiments":{},"offline":{"channel":{}},
        "entityCtr":{"channel":0.148},"channelPriority":{}});
    for row in rows {
        state["metadata"][row["id"].as_str().unwrap()] = json!({"latest":null,"lastRead":null,"draft":null,"starred":null,"unread":null,"priority":null});
    }
    state
}
fn rank(query: &str, rows: &[Matched<'_>], state: &Value) -> Result<Observation, Error> {
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    rank_channels(
        query,
        rows,
        &Context {
            state,
            history: &history,
            model: &model,
            now_ms: 1760000000000.0,
            normalize: &|s| Ok(s.to_lowercase()),
        },
        &mut SimilarityCache::default(),
        |a, b| Ok(a.cmp(b)),
        false,
    )
}
#[test]
fn actual_factory_channel_shortlist_and_final_order() {
    let oracle: Value = serde_json::from_str(include_str!("channel-oracle.json")).unwrap();
    let case = &oracle["case"];
    let inputs = case["groups"]["channels"].as_array().unwrap();
    let rows: Vec<_> = inputs
        .iter()
        .map(|r| {
            raw(
                r["id"].as_str().unwrap(),
                r["item"]["name"].as_str().unwrap(),
            )
        })
        .collect();
    let scored: Vec<_> = rows
        .iter()
        .zip(inputs)
        .map(|(item, source)| Matched {
            scored: local::ChannelScored {
                item,
                id: item["id"].as_str().unwrap(),
                score: source["score"].as_u64().map(|v| v as u32),
                is_exact_match: None,
                previous_name_scores: None,
                best_previous_name_match: None,
                is_flannel_fuzzy_passthrough: false,
            },
            comparison: item["name"].as_str().unwrap().to_owned(),
        })
        .collect();
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    let source_state = state(&rows);
    let normalize = |s: &str| {
        oracle["normalization"][s]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::MissingObservation(s.into()))
    };
    let result = rank_channels(
        case["query"].as_str().unwrap(),
        &scored,
        &Context {
            state: &source_state,
            history: &history,
            model: &model,
            now_ms: case["state"]["now"].as_f64().unwrap(),
            normalize: &normalize,
        },
        &mut SimilarityCache::default(),
        |a, b| {
            oracle["collation"][a][b]
                .as_i64()
                .map(|n| n.cmp(&0))
                .ok_or_else(|| Error::MissingObservation(format!("collation {a}/{b}")))
        },
        false,
    )
    .unwrap();
    assert_eq!(result.latest_ids.len(), 75);
    assert_eq!(result.rows.len(), 25);
    assert_eq!(json!(result.latest_ids), case["latest"]);
    assert_eq!(
        json!(result.rows.iter().map(|r| &r.row["id"]).collect::<Vec<_>>()),
        case["order"]
    );
    assert!(result.rows.iter().all(|r| r.original_index < 90));
}
#[test]
fn missing_offline_channel_uses_texty_fallback_but_null_does_not() {
    let rows = vec![raw("C12345678", "alpha")];
    let matched = matched("", &rows);
    let mut source = state(&rows);
    let missing = rank("", &matched, &source).unwrap();
    assert_eq!(
        missing.rows[0].row["offlineFeatures"],
        json!({"user_ctr_30dd":0.148,"team_ctr_90dd":0})
    );
    assert!(
        source["offline"]["channel"].get("C12345678").is_none(),
        "input never mutated"
    );
    source["offline"]["channel"]["C12345678"] = Value::Null;
    let null = rank("", &matched, &source).unwrap();
    assert!(null.rows[0].row.get("offlineFeatures").is_none());
    source["offline"]["channel"]["C12345678"] = json!({"user_ctr_30dd":0.75,"team_ctr_90dd":0.25});
    let observed = rank("", &matched, &source).unwrap();
    assert_eq!(
        observed.rows[0].row["offlineFeatures"],
        source["offline"]["channel"]["C12345678"]
    );
    assert_ne!(observed.rows[0].features, missing.rows[0].features);
}
#[test]
fn empty_query_public_then_private_and_duplicate_occurrences_survive() {
    let mut private = raw("G12345678", "secret");
    private["is_private"] = json!(true);
    let rows = vec![
        private,
        raw("C12345678", "alpha"),
        raw("C12345678", "alpha"),
    ];
    let matches = matched("", &rows);
    assert_eq!(
        matches.iter().map(Matched::id).collect::<Vec<_>>(),
        ["C12345678", "C12345678", "G12345678"]
    );
    assert!(
        matches
            .iter()
            .all(|m| m.exact().is_none() && m.score() == Some(0))
    );
    let ranked = rank("", &matches, &state(&rows)).unwrap();
    let mut indices = ranked
        .rows
        .iter()
        .filter(|r| r.row["id"] == "C12345678")
        .map(|r| r.original_index)
        .collect::<Vec<_>>();
    indices.sort();
    assert_eq!(indices, [0, 1]);
}
#[test]
fn exact_previous_and_remote_passthrough_keep_distinct_source_metadata() {
    let mut renamed = raw("C12345678", "zzzzzz");
    renamed["previous_names"] = json!(["mpdm-ignored", "alpha"]);
    let rows = vec![renamed];
    let prior = matched("alpha", &rows);
    assert_eq!(prior[0].previous_match(), Some("alpha"));
    assert_eq!(prior[0].exact(), Some(false));
    assert!(!prior[0].passthrough());
    assert_eq!(matched("C12345678", &rows)[0].score(), Some(0));
    let rows = vec![raw("C12345678", "zzzzzz")];
    let query = local::PreparedQuery {
        normalized: "alpha".into(),
        matcher_normalized: "alpha".into(),
    };
    let remote = BTreeSet::from(["C12345678".into()]);
    let call = |enabled, ids: &BTreeSet<String>| {
        matching::channels(
            "alpha",
            &query,
            &rows,
            ids,
            enabled,
            names,
            |s| Ok(s.into()),
        )
        .unwrap()
    };
    assert!(call(false, &remote).is_empty());
    assert!(call(true, &BTreeSet::new()).is_empty());
    let result = call(true, &remote);
    assert!(result[0].passthrough());
    assert_eq!(result[0].score(), Some(2));
}
#[test]
fn unsupported_categories_and_missing_canonical_locality_refuse() {
    let base = raw("C12345678", "alpha");
    let mut variants = Vec::new();
    for key in [
        "is_im",
        "is_mpim",
        "is_file",
        "is_usergroup",
        "isUnknown",
        "isNonExistent",
    ] {
        let mut v = base.clone();
        v[key] = json!(true);
        variants.push(v);
    }
    for properties in [
        json!({"huddles":{"is_huddle_channel":true}}),
        json!({"record_channel":{"record_id":"S:a:b"}}),
        json!({"meetings_channel":{"is_meeting_channel":true}}),
    ] {
        let mut v = base.clone();
        v["properties"] = properties;
        variants.push(v);
    }
    for locality in [Value::Null, json!(true)] {
        let mut v = base.clone();
        v["fromAnotherTeam"] = locality;
        variants.push(v);
    }
    let mut meeting = base.clone();
    meeting["name"] = json!("mc:meeting");
    variants.push(meeting);
    for variant in variants {
        assert!(!ordinary_local(&variant), "{variant}");
    }
    assert!(ordinary_local(&base));
}
#[test]
fn wire_ranking_facts_are_discarded() {
    let mut row = raw("C12345678", "alpha");
    row["facts"] = json!({"member":true,"channel":false,"archived":true,"comparisonName":"Injected","channelMember":false});
    let rows = vec![row];
    let result = rank("alpha", &matched("alpha", &rows), &state(&rows)).unwrap();
    let facts = &result.rows[0].row["item"]["facts"];
    assert_eq!(facts["member"], false);
    assert_eq!(facts["channel"], true);
    assert_eq!(facts["archived"], false);
    assert_eq!(facts["channelMember"], true);
    assert_eq!(facts["comparisonName"], "alpha");
}
#[test]
fn absent_selector_or_renderer_observation_refuses_without_fallback() {
    let rows = vec![raw("C12345678", "alpha"), raw("C23456789", "beta")];
    let matched = matched("", &rows);
    let good = state(&rows);
    for key in [
        "metadata",
        "navigationHistory",
        "experiments",
        "enterprise",
        "teamId",
        "offline",
    ] {
        let mut source = good.clone();
        source.as_object_mut().unwrap().remove(key);
        assert!(
            matches!(
                rank("", &matched, &source),
                Err(Error::MissingObservation(_))
            ),
            "{key}"
        );
    }
    let mut source = good.clone();
    source["metadata"]["C12345678"]
        .as_object_mut()
        .unwrap()
        .remove("draft");
    assert!(rank("", &matched, &source).is_err());
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    let context = Context {
        state: &good,
        history: &history,
        model: &model,
        now_ms: 1.0,
        normalize: &|s| Ok(s.into()),
    };
    assert!(matches!(
        rank_channels(
            "",
            &matched,
            &context,
            &mut SimilarityCache::default(),
            |_, _| Err(Error::MissingObservation("collation".into())),
            false
        ),
        Err(Error::MissingObservation(_))
    ));
    let bad = Context {
        normalize: &|_| Err(Error::MissingObservation("normalization".into())),
        ..context
    };
    assert!(matches!(
        rank_channels(
            "",
            &matched,
            &bad,
            &mut SimilarityCache::default(),
            |a, b| Ok(a.cmp(b)),
            false
        ),
        Err(Error::MissingObservation(_))
    ));
    assert!(
        matching::channels(
            "",
            &local::PreparedQuery {
                normalized: String::new(),
                matcher_normalized: String::new()
            },
            &rows,
            &BTreeSet::new(),
            false,
            |_| Err(Error::MissingObservation("name".into())),
            |s| Ok(s.into())
        )
        .is_err()
    );
}
