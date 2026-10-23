use super::super::prepare::{MentionObservations, MentionPlan, MentionStep};
use super::*;
use crate::composer_emoji::{history::History, model::Model};

fn lookup(query: &str) -> MentionLookup {
    MentionLookup {
        kind: MentionKind::Channel,
        query: query.into(),
    }
}
fn plan(query: &str) -> Plan {
    Plan::capture(&lookup(query), Some(false)).unwrap().unwrap()
}
fn raw(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"is_channel":true,"is_member":true,"fromAnotherTeam":false})
}
fn names(row: &Value) -> Result<Name, Error> {
    let name = row["name"].as_str().unwrap();
    assert!(name.is_ascii(), "synthetic ASCII-only test observation");
    Ok(Name {
        normalized: name.to_lowercase(),
        comparison_lowercase: name.to_lowercase(),
    })
}
fn matches<'a>(plan: &Plan, rows: &'a [Value]) -> Vec<Matched<'a>> {
    let prepared = local::prepare_query(plan.formatted_query(), str::to_lowercase);
    plan.matching(
        &Query {
            formatted: plan.formatted_query(),
            prepared: &prepared,
            beyond_boundary_eligible: None,
        },
        rows,
        names,
    )
    .unwrap()
}
fn state(rows: &[Value]) -> Value {
    let mut state = json!({"selfId":"U_SELF","teamId":"T1","enterprise":false,
        "metadata":{},"navigationHistory":{},"experiments":{},"offline":{"channel":{}},
        "entityCtr":{"channel":0.148},"channelPriority":{}});
    for row in rows {
        state["metadata"][row["id"].as_str().unwrap()] = json!({
        "latest":null,"lastRead":null,"draft":null,"starred":null,"unread":null,"priority":null});
    }
    state
}
fn rank(plan: &Plan, matches: &[Matched<'_>], state: &Value) -> Result<Observation, Error> {
    plan.rank(
        matches,
        &Context {
            state,
            history: &History::from_wire("{}").unwrap(),
            model: &Model::fallback(),
            now_ms: 1760000000000.0,
            normalize: &|s| Ok(s.to_lowercase()),
        },
        &mut SimilarityCache::default(),
        |a, b| Ok(a.cmp(b)),
    )
}
#[test]
fn channel_options_and_trigger_formatting_are_not_interactive_defaults() {
    assert!(Plan::capture(&lookup(""), None).unwrap().is_none());
    assert!(Plan::capture(&lookup("alpha"), None).is_err());
    assert!(
        Plan::capture(
            &MentionLookup {
                kind: MentionKind::Member,
                query: "alpha".into()
            },
            Some(false)
        )
        .is_err()
    );
    for (raw, want) in [
        ("#alpha", "alpha"),
        ("\u{ff03}alpha", "alpha"),
        (":wave", ":wave"),
        (" ##alpha", " ##alpha"),
        ("##alpha", "#alpha"),
    ] {
        assert_eq!(plan(raw).formatted_query(), want);
    }
    assert_eq!(
        plan("alpha").requested(),
        json!({"query":"alpha","options":{
        "entities":{"channels":{"includePrivate":false,"includeFromAnotherTeam":true}},
        "limit":25,"sort":{"source":"texty-autocomplete"}}})
    );
}
#[test]
fn public_source_order_archived_cross_team_and_no_previous_names() {
    let rows = vec![
        raw("C00000009", "alpha"),
        json!({"id":"G00000002","is_channel":true,"is_private":true,"name":"alpha"}),
        json!({"id":"C00000003","is_channel":true,"name":"zzzzz","previous_names":["alpha"],"fromAnotherTeam":false}),
        json!({"id":"C00000004","is_channel":true,"name":"alpha","is_archived":true,"fromAnotherTeam":true}),
    ];
    let before = rows.clone();
    let p = plan("alpha");
    let m = matches(&p, &rows);
    assert_eq!(
        m.iter().map(Matched::id).collect::<Vec<_>>(),
        ["C00000009", "C00000004"]
    );
    assert!(std::ptr::eq(m[0].raw(), &rows[0]));
    assert!(std::ptr::eq(m[1].raw(), &rows[3]));
    assert!(
        m.iter()
            .all(|m| m.scored.previous_name_scores.is_none()
                && !m.scored.is_flannel_fuzzy_passthrough)
    );
    assert_eq!(rows, before);
}
#[test]
fn absent_normalization_locality_and_special_names_do_not_become_empty_results() {
    let p = plan("alpha");
    let q = local::prepare_query("alpha", str::to_lowercase);
    let query = Query {
        formatted: "alpha",
        prepared: &q,
        beyond_boundary_eligible: None,
    };
    let mut row = raw("C00000001", "alpha");
    assert!(
        p.matching(&query, &[row.clone()], |_| Err(Error::MissingObservation(
            "name".into()
        )))
        .is_err()
    );
    row.as_object_mut().unwrap().remove("fromAnotherTeam");
    assert!(p.matching(&query, &[row], names).is_err());
    for patch in [
        json!({"properties":{"record_channel":{"record_id":"R1"}}}),
        json!({"name":"mc:meeting"}),
    ] {
        let mut row = raw("C00000001", "alpha");
        row.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(matches!(
            p.matching(&query, &[row], names),
            Err(Error::UnsupportedFeature(_))
        ));
    }
    let rows = vec![raw("C00000001", "alpha"); 2];
    assert!(p.matching(&query, &rows, names).is_err());
    let rows = vec![json!({"id":"C00000001","is_channel":true,"isUnknown":true})];
    assert!(
        p.matching(&query, &rows, |_| panic!("excluded row cannot demand name"))
            .unwrap()
            .is_empty()
    );
}
#[test]
fn boundary_toggle_requires_reached_grapheme_observation() {
    let p = Plan::capture(&lookup("lpha"), Some(true)).unwrap().unwrap();
    let rows = vec![raw("C00000001", "alpha")];
    let q = local::prepare_query("lpha", str::to_lowercase);
    assert!(
        p.matching(
            &Query {
                formatted: "lpha",
                prepared: &q,
                beyond_boundary_eligible: None
            },
            &rows,
            names
        )
        .is_err()
    );
    assert!(
        p.matching(
            &Query {
                formatted: "lpha",
                prepared: &q,
                beyond_boundary_eligible: Some(false)
            },
            &rows,
            names
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        p.matching(
            &Query {
                formatted: "lpha",
                prepared: &q,
                beyond_boundary_eligible: Some(true)
            },
            &rows,
            names
        )
        .unwrap()
        .len(),
        1
    );
    assert!(
        p.matching(
            &Query {
                formatted: "other",
                prepared: &q,
                beyond_boundary_eligible: Some(true)
            },
            &rows,
            names
        )
        .is_err()
    );
}
#[test]
fn rank_keeps_original_raw_and_case_sensitive_filter_then_existing_local_preference() {
    let p = plan("alpha");
    let mut foreign = raw("C00000001", "alpha");
    foreign["fromAnotherTeam"] = json!(true);
    let mut local = raw("C00000002", "alpha");
    local["opaque"] = json!({"keep":1});
    local["facts"] = json!({"name":"wire-forgery"});
    let rows = vec![foreign, local, raw("C00000003", "Alpha")];
    let before = rows.clone();
    let result = rank(&p, &matches(&p, &rows), &state(&rows)).unwrap();
    assert_eq!(result.search.rows.len(), 2);
    assert!(result.search.rows.contains(&rows[1]));
    let mention = MentionPlan::capture(&[json!({"insert":"#alpha\n"})], true, true).unwrap();
    let observed = MentionObservations {
        searches: vec![result.search],
        highlight_words: Some(vec!["<@U_SELF".into()]),
        ..Default::default()
    };
    let MentionStep::Complete { choices, .. } = mention.advance(&observed).unwrap() else {
        panic!()
    };
    assert_eq!(choices[0].id, "C00000002");
    assert_eq!(rows, before);
}
#[test]
fn missing_rank_facts_and_foreign_query_matches_are_errors() {
    let p = plan("alpha");
    let rows = vec![raw("C00000001", "alpha")];
    let m = matches(&p, &rows);
    let mut s = state(&rows);
    s["metadata"]["C00000001"]
        .as_object_mut()
        .unwrap()
        .remove("draft");
    assert!(rank(&p, &m, &s).is_err());
    let s = state(&rows);
    assert!(rank(&plan("other"), &m, &s).is_err());
    let q = plan("no-match");
    let result = rank(&q, &matches(&q, &rows), &s).unwrap();
    assert!(result.search.rows.is_empty());
}
#[test]
fn original_factory_90_to_75_to_25_order_precedes_exact_filter() {
    let oracle: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/slack/api/profile_sections/custom/texty/ranking/channel-oracle.json"
    )))
    .unwrap();
    let case = &oracle["case"];
    let inputs = case["groups"]["channels"].as_array().unwrap();
    let p = plan(case["query"].as_str().unwrap());
    let rows: Vec<_> = inputs
        .iter()
        .map(|r| {
            raw(
                r["id"].as_str().unwrap(),
                r["item"]["name"].as_str().unwrap(),
            )
        })
        .collect();
    // Original scorer outputs isolate exact original 4ucY/Ejs0 ranking vectors.
    let m: Vec<_> = rows
        .iter()
        .zip(inputs)
        .map(|(item, source)| Matched {
            scored: local::ChannelScored {
                item,
                id: item["id"].as_str().unwrap(),
                score: source["score"].as_u64().map(|n| n as u32),
                is_exact_match: None,
                previous_name_scores: None,
                best_previous_name_match: None,
                is_flannel_fuzzy_passthrough: false,
            },
            comparison: item["name"].as_str().unwrap().into(),
            query: p.lookup.query.clone(),
        })
        .collect();
    let result = p
        .rank(
            &m,
            &Context {
                state: &state(&rows),
                history: &History::from_wire("{}").unwrap(),
                model: &Model::fallback(),
                now_ms: case["state"]["now"].as_f64().unwrap(),
                normalize: &|s| {
                    oracle["normalization"][s]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or(Error::InvalidInput)
                },
            },
            &mut SimilarityCache::default(),
            |a, b| {
                oracle["collation"][a][b]
                    .as_i64()
                    .map(|n| n.cmp(&0))
                    .ok_or(Error::InvalidInput)
            },
        )
        .unwrap();
    assert_eq!(json!(result.latest_ids), case["latest"]);
    assert_eq!(json!(result.ranked_ids), case["order"]);
    assert_eq!(result.latest_ids.len(), 75);
    assert_eq!(result.ranked_ids.len(), 25);
    assert!(
        result
            .search
            .rows
            .iter()
            .all(|r| r["name"] == p.lookup.query)
    );
}
