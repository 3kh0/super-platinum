use super::*;
use crate::composer_emoji::{history::History, model::Model};

fn lookup(query: &str) -> MentionLookup {
    MentionLookup {
        kind: MentionKind::Member,
        query: query.into(),
    }
}
fn flags() -> Flags {
    Flags {
        unified_user: Some(true),
        salesforce_blocked: Some(false),
        beyond: Some(false),
    }
}
fn plan(query: &str) -> Plan {
    Plan::capture(&lookup(query), flags()).unwrap().unwrap()
}
fn facts() -> MemberFacts {
    MemberFacts {
        interaction_allowed: Some(true),
        slackbot: Some(false),
        system_notification: Some(false),
        search_bot: Some(false),
        channel_expert_bot: Some(false),
        agent_environment: Some(false),
        workflow: Some(false),
        deleted: Some(false),
        external_huddle_guest: Some(false),
        profile_only: Some(false),
        ..Default::default()
    }
}
fn row(index: usize) -> Value {
    json!({"id":format!("U{index:08}"),"team_id":"T00000001","name":"different_username",
        "profile":{"display_name":"harbor","real_name":"Harbor Person"},"opaque":{"keep":index}})
}
fn derived(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(|row| local::derive_member_names(row, str::to_lowercase).unwrap())
        .collect()
}
fn group(index: usize) -> groups::Snapshot {
    let id = format!("S{index:08}");
    groups::Snapshot::capture(json!({"id":id,"name":"incoming partial"}),
        json!({"id":id,"name":"Harbor group","handle":"harbor","is_usergroup":true,"_name_lc":"harbor group","opaque":{"keep":"effective"}})).unwrap()
}
fn group_names(row: &groups::Snapshot) -> Result<groups::Names, crate::slack::Error> {
    groups::Names::observe(
        row,
        |s| Ok(s.to_lowercase()),
        |s| Ok(local::normalize_query_with_lowercase(s, str::to_lowercase)),
    )
}
fn matches<'a>(
    p: &Plan,
    raw: &'a [Value],
    derived: &'a [Value],
    groups: &'a [groups::Snapshot],
) -> Matched<'a> {
    let prepared = local::prepare_query(p.formatted_query(), str::to_lowercase);
    p.matching(
        &Query {
            formatted: p.formatted_query(),
            prepared: &prepared,
            maybe_self: Some(false),
            beyond_boundary_eligible: None,
        },
        &Members::capture(raw, derived).unwrap(),
        groups,
        |_| Ok(facts()),
        |_| {
            Ok(GroupFacts {
                active: Some(true),
                ..Default::default()
            })
        },
        group_names,
    )
    .unwrap()
}
fn state() -> Value {
    json!({"selfId":"U99999999","self":{"facts":{"team":"T00000001"}},"teamId":"T00000001",
        "enterprise":false,"enterpriseId":null,"ims":{},"metadata":{},"navigationHistory":{},
        "experiments":{"autocomplete_affinity_score":"off","autocomplete_affinity_ml_model":"off"},
        "offline":{},"remoteDm":{},"remoteMpim":{},"affinity":{},"entityCtr":{"member":0.1,"userGroup":0.9}})
}
fn rank(p: &Plan, m: &Matched<'_>, state: &Value) -> Result<Observation, Error> {
    p.rank(
        m,
        &Context {
            state,
            history: &History::from_wire("{}").unwrap(),
            model: &Model::from_weights(std::collections::BTreeMap::from([(
                "USER_ENTITY_TYPE_CTR_30DD".into(),
                3.0,
            )]))
            .unwrap(),
            now_ms: 1760000000000.0,
            normalize: &|s| Ok(s.to_lowercase()),
        },
        &mut SimilarityCache::default(),
        |row| {
            Ok(row["profile"]["display_name"]
                .as_str()
                .unwrap()
                .to_lowercase())
        },
        |a, b| Ok(a.cmp(b)),
        false,
    )
}
#[test]
fn requested_options_and_missing_flags_are_not_popup_defaults() {
    assert!(
        Plan::capture(&lookup(""), Flags::default())
            .unwrap()
            .is_none()
    );
    assert!(Plan::capture(&lookup("harbor"), Flags::default()).is_err());
    let p = plan("@harbor");
    assert_eq!(p.formatted_query(), "harbor");
    assert_eq!(
        p.requested(),
        json!({"query":"@harbor","options":{"entities":{"userGroups":{"includeFromAnotherTeam":true},
        "members":{"includeSelf":true,"includeSlackbot":false,"excludeSalesforceApp":false}},"limit":25,"sort":{"source":"texty-autocomplete"}}})
    );
    assert_eq!(plan(":harbor").formatted_query(), ":harbor");
    assert_eq!(plan(" @harbor").formatted_query(), " @harbor");
}
#[test]
fn eligibility_refuses_reached_missing_facts_and_keeps_short_circuit() {
    assert!(facts().allows(false).unwrap());
    assert!(facts().allows(true).is_err());
    let mut f = facts();
    f.interaction_allowed = Some(false);
    f.agent_environment = None;
    assert!(!f.allows(true).unwrap());
    let mut f = facts();
    f.slackbot = Some(true);
    f.system_notification = None;
    assert!(!f.allows(false).unwrap());
    f.agent_environment = None;
    assert!(f.allows(false).is_err());
    let mut f = facts();
    f.agent_environment = Some(true);
    f.agent = Some(true);
    assert!(f.allows(false).is_err());
    f.admitted_agent = Some(false);
    f.workflow = None;
    assert!(!f.allows(false).unwrap());
}
#[test]
fn deleted_profile_only_workflow_and_huddle_guests_never_use_popup_admission() {
    for field in [0, 1, 2, 3] {
        let mut f = facts();
        match field {
            0 => f.deleted = Some(true),
            1 => f.profile_only = Some(true),
            2 => f.workflow = Some(true),
            _ => f.external_huddle_guest = Some(true),
        }
        assert!(!f.allows(false).unwrap());
    }
    assert!(
        GroupFacts {
            active: Some(true),
            ..Default::default()
        }
        .allows(true)
        .unwrap()
    );
    assert!(
        GroupFacts {
            active: Some(true),
            ..Default::default()
        }
        .allows(false)
        .is_err()
    );
    assert!(
        !GroupFacts {
            from_another_team: Some(true),
            active: None
        }
        .allows(false)
        .unwrap()
    );
}
#[test]
fn source_and_derived_names_cannot_change_identity_or_other_fields() {
    let raw = vec![row(1)];
    let d = derived(&raw);
    assert!(Members::capture(&raw, &d).is_ok());
    for key in ["id", "name", "opaque"] {
        let mut changed = d.clone();
        changed[0][key] = json!("changed");
        assert!(Members::capture(&raw, &changed).is_err());
    }
    assert!(Members::capture(&raw, &[]).is_err());
}
#[test]
fn matching_uses_derived_display_real_names_not_raw_username_or_email() {
    let raw = vec![row(1)];
    let d = derived(&raw);
    assert_eq!(matches(&plan("harbor"), &raw, &d, &[]).members().count(), 1);
    assert_eq!(
        matches(&plan("different_username"), &raw, &d, &[])
            .members()
            .count(),
        0
    );
    let m = matches(&plan("harbor"), &raw, &d, &[]);
    assert_eq!(m.members[0].is_exact_match, None);
    assert!(!m.members[0].is_self);
    let result = rank(&plan("harbor"), &m, &state()).unwrap();
    assert_eq!(result.search.rows, raw);
}
#[test]
fn group_failure_prevents_partial_member_result_and_missing_self_is_not_false() {
    let p = plan("harbor");
    let raw = vec![row(1)];
    let d = derived(&raw);
    let gs = vec![group(1)];
    let prepared = local::prepare_query("harbor", str::to_lowercase);
    let query = Query {
        formatted: "harbor",
        prepared: &prepared,
        maybe_self: Some(false),
        beyond_boundary_eligible: None,
    };
    assert!(
        p.matching(
            &query,
            &Members::capture(&raw, &d).unwrap(),
            &gs,
            |_| Ok(facts()),
            |_| Ok(GroupFacts::default()),
            group_names
        )
        .is_err()
    );
    let query = Query {
        maybe_self: None,
        ..query
    };
    assert!(
        p.matching(
            &query,
            &Members::capture(&raw, &d).unwrap(),
            &[],
            |_| Ok(facts()),
            |_| Ok(GroupFacts::default()),
            group_names
        )
        .is_err()
    );
}
#[test]
fn one_mixed_shortlist_cap_preserves_effective_group_and_original_member_rows() {
    for count in [40, 80] {
        let raw: Vec<_> = (0..count).map(row).collect();
        let d = derived(&raw);
        let gs: Vec<_> = (0..count).map(group).collect();
        let p = plan("harbor");
        let m = matches(&p, &raw, &d, &gs);
        let result = rank(&p, &m, &state()).unwrap();
        assert_eq!(result.search.rows.len(), 25);
        for value in &result.search.rows {
            assert!(raw.contains(value) || gs.iter().any(|g| g.effective() == value));
            assert!(value.get("facts").is_none());
        }
        if count == 80 {
            assert!(
                result
                    .search
                    .rows
                    .iter()
                    .all(|r| r["id"].as_str().unwrap().starts_with('U'))
            );
        }
    }
}
#[test]
fn ranking_refuses_unobserved_state_and_foreign_query() {
    let raw = vec![row(1)];
    let d = derived(&raw);
    let p = plan("harbor");
    let m = matches(&p, &raw, &d, &[]);
    assert!(rank(&p, &m, &json!({})).is_err());
    assert!(rank(&plan("other"), &m, &state()).is_err());
}
#[test]
fn resolver_names_are_original_five_cyv_not_display_policy_or_username() {
    let raw = row(1);
    let n = names(&raw).unwrap();
    assert_eq!(
        (n.primary.as_str(), n.secondary.as_str()),
        ("@harbor", "@Harbor Person")
    );
    assert_eq!(n.raw, raw);
    let n =
        names(&json!({"id":"U00000001","name":"not-a-fallback","real_name":"Top","profile":{}}))
            .unwrap();
    assert_eq!((n.primary.as_str(), n.secondary.as_str()), ("@Top", ""));
    let n = names(&json!({"id":"U00000001","name":"not-a-fallback"})).unwrap();
    assert_eq!(n.primary, "@");
    let n = names(group(1).effective()).unwrap();
    assert_eq!((n.primary.as_str(), n.secondary.as_str()), ("@harbor", ""));
    assert!(names(&json!({"profile":{"display_name":{}}})).is_err());
    for raw in [
        json!({"is_usergroup":true}),
        json!({"is_usergroup":true,"handle":null}),
        json!({"is_usergroup":true,"handle":0}),
    ] {
        assert!(matches!(names(&raw), Err(Error::UnsupportedFeature(_))));
    }
    assert_eq!(
        names(&json!({"is_usergroup":true,"handle":""}))
            .unwrap()
            .primary,
        "@"
    );
}
#[test]
fn resolver_deburr_preserves_case_and_every_whitespace_until_renderer_lowercase() {
    assert_eq!(local::normalize_name_source("  DÉJÀ\tVu  "), "  DEJA\tVu  ");
    assert_eq!(local::normalize_name_source("Æ Œ e\u{301}"), "Ae Oe e");
    assert_ne!(
        local::normalize_name_source("  a\t\tb  "),
        local::normalize_lowered_query("  a\t\tb  ")
    );
}
#[test]
fn unsupported_empty_normalized_query_is_not_a_fabricated_empty_lookup() {
    let p = plan(" ");
    let prepared = local::prepare_query(" ", str::to_lowercase);
    assert!(matches!(
        p.matching(
            &Query {
                formatted: " ",
                prepared: &prepared,
                maybe_self: Some(false),
                beyond_boundary_eligible: None
            },
            &Members::capture(&[], &[]).unwrap(),
            &[],
            |_| Ok(facts()),
            |_| Ok(GroupFacts::default()),
            group_names
        ),
        Err(Error::UnsupportedFeature(_))
    ));
}

#[test]
fn actual_mention_planner_keeps_hyphenated_unknown_as_one_lookup() {
    let ops = vec![json!({"insert":"Known @harbor and @not-found"})];
    let plan = super::super::super::prepare::MentionPlan::capture(&ops, true, true).unwrap();
    assert_eq!(plan.lookups(), &[lookup("harbor"), lookup("not-found")]);
}

#[test]
fn effective_group_handle_ranks_resolves_and_encodes_without_promoting_name_or_partial() {
    use crate::slack_lists::comments::edit_seed::{
        mode::EditMode,
        prepare::{
            EncodePlan, EncodingObservations, EncodingStep, MentionNeed, MentionObservations,
            MentionPlan, MentionStep,
        },
    };

    let source = json!({"id":"SGROUP001","name":"Incoming partial","opaque":{"source":true}});
    let effective = json!({"id":"SGROUP001","name":"Planning","_name_lc":"planning",
        "handle":"sail-team","is_usergroup":true,"opaque":{"effective":true}});
    let groups = vec![groups::Snapshot::capture(source.clone(), effective.clone()).unwrap()];
    let humans = vec![row(1)]; // Actual matching input, deliberately not a match.
    let derived = derived(&humans);
    for (label, positive) in [("sail-team", true), ("sail", false), ("Planning", false)] {
        let text = format!("Known @{label} and @not-found\n");
        let input = vec![json!({"insert":text})];
        let before = input.clone();
        let mention_plan = MentionPlan::capture(&input, true, true).unwrap();
        let mut observed = MentionObservations {
            // Explicit synthetic current membership/highlights observation.
            highlight_words: Some(vec!["<!subteam^SGROUP001".into()]),
            ..Default::default()
        };
        let mut complete = None;
        for _ in 0..64 {
            match mention_plan.advance(&observed).unwrap() {
                MentionStep::Need(MentionNeed::Search(lookup)) => {
                    let p = Plan::capture(&lookup, flags()).unwrap().unwrap();
                    let matched = matches(&p, &humans, &derived, &groups);
                    assert_eq!(matched.members().count(), 0);
                    let result = rank(&p, &matched, &state()).unwrap();
                    if lookup.query == "not-found" {
                        assert!(result.search.rows.is_empty());
                    } else {
                        // Name and partial-handle controls genuinely matched
                        // and ranked; only the resolver refuses their splice.
                        assert_eq!(result.search.rows, vec![effective.clone()]);
                        assert_eq!(result.ranked_ids, ["SGROUP001"]);
                    }
                    observed.searches.push(result.search);
                }
                MentionStep::Need(MentionNeed::Names(raw)) => {
                    assert_eq!(raw, effective);
                    observed.names.push(names(&raw).unwrap());
                }
                MentionStep::Need(MentionNeed::Normalization(value)) => {
                    // Same bounded synthetic ASCII convention as resolver
                    // tests; no claim of executing renderer JS here.
                    assert!(value.is_ascii());
                    observed
                        .normalizations
                        .insert(value.clone(), value.to_lowercase());
                }
                MentionStep::Complete { ops, choices } => {
                    complete = Some((ops, choices));
                    break;
                }
                other => panic!("unexpected observation demand: {other:?}"),
            }
        }
        let (ops, choices) = complete.expect("original observation demands settle");
        if positive {
            assert_eq!(choices.len(), 1);
            assert_eq!(choices[0].raw, effective);
            assert_ne!(choices[0].raw, source);
            assert_eq!(choices[0].id, "SGROUP001");
            assert_eq!(choices[0].label, "@sail-team");
            assert_eq!(
                ops.iter()
                    .find(|op| op["attributes"]["slackmention"].is_object())
                    .unwrap(),
                &json!({"insert":"@sail-team","attributes":{"slackmention":{
                    "id":"SGROUP001","label":"@sail-team","mention":true,"unverified":false}}})
            );
        } else {
            assert!(choices.is_empty());
            assert_eq!(ops, input);
        }
        let encoding = EncodePlan::capture(&ops, EditMode::PlainText, false).unwrap();
        let EncodingStep::Complete(blocks) =
            encoding.advance(&EncodingObservations::default()).unwrap()
        else {
            panic!("group semantic encoding must not ask for fabricated file/emoji facts")
        };
        let elements = if positive {
            json!([{"type":"text","text":"Known "},{"type":"usergroup","usergroup_id":"SGROUP001"},
                {"type":"text","text":" and @not-found"}])
        } else {
            json!([{"type":"text","text":format!("Known @{label} and @not-found")}])
        };
        assert_eq!(
            blocks,
            vec![
                json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":elements}]})
            ]
        );
        assert_eq!(input, before);
        assert_eq!(groups[0].source(), &source);
        assert_eq!(groups[0].effective(), &effective);
    }
}

#[test]
fn mixed_full_label_history_requires_thirty_normalized_points_for_either_identity() {
    use crate::slack_lists::comments::edit_seed::{
        mode::EditMode,
        prepare::{
            EncodePlan, EncodingObservations, EncodingStep, MentionNeed, MentionObservations,
            MentionPlan, MentionStep,
        },
    };

    let humans = vec![row(1)];
    let derived = derived(&humans);
    let groups = vec![group(1)];
    let source = groups[0].source().clone();
    let effective = groups[0].effective().clone();
    let original_human = humans[0].clone();
    let input = vec![json!({"insert":"Known @harbor and @not-found\n"})];
    let before = input.clone();
    let mention_plan = MentionPlan::capture(&input, true, true).unwrap();
    const NOW: f64 = 1_760_000_000_000.0; // Explicit synthetic clock, no host IO.

    // Assert actual normalized scores, not an assumed relationship between
    // counts and the resolver's threshold. Exercise either entity as leader.
    for (human_count, group_count, expected_scores, winner) in [
        (200, 200, [100.0, 100.0], None),
        (141, 200, [70.5, 100.0], None),
        (140, 200, [70.0, 100.0], Some("S00000001")),
        (200, 141, [100.0, 70.5], None),
        (200, 140, [100.0, 70.0], Some("U00000001")),
    ] {
        let pref = json!(format!(
            r#"{{"U00000001":{{"id":"U00000001","count":{human_count},"visits":[1760000000000]}},"S00000001":{{"id":"S00000001","count":{group_count},"visits":[1760000000000]}}}}"#
        ));
        let history = History::from_pref(&pref).unwrap();
        let scores = history.score_normalized("@harbor", &["U00000001", "S00000001"], NOW);
        assert_eq!([scores[0].raw, scores[1].raw], expected_scores);
        let gap = (scores[0].raw - scores[1].raw).abs();
        assert_eq!(
            gap,
            if winner.is_some() {
                30.0
            } else if human_count == group_count {
                0.0
            } else {
                29.5
            }
        );
        let history_before = history.to_wire().unwrap();
        let rank_state = state();
        let rank_model = Model::from_weights(std::collections::BTreeMap::from([(
            "USER_ENTITY_TYPE_CTR_30DD".into(),
            3.0,
        )]))
        .unwrap();
        let mut observed = MentionObservations {
            highlight_words: Some(vec!["<@U00000001".into(), "<!subteam^S00000001".into()]),
            ..Default::default()
        };
        let mut demands = Vec::new();
        let mut complete = None;
        for _ in 0..96 {
            match mention_plan.advance(&observed).unwrap() {
                MentionStep::Need(MentionNeed::Search(lookup)) => {
                    let p = Plan::capture(&lookup, flags()).unwrap().unwrap();
                    let matched = matches(&p, &humans, &derived, &groups);
                    let result = p
                        .rank(
                            &matched,
                            &Context {
                                state: &rank_state,
                                history: &history,
                                model: &rank_model,
                                now_ms: NOW,
                                normalize: &|s| Ok(s.to_lowercase()),
                            },
                            &mut SimilarityCache::default(),
                            |raw| {
                                Ok(raw["profile"]["display_name"]
                                    .as_str()
                                    .unwrap()
                                    .to_lowercase())
                            },
                            |a, b| Ok(a.cmp(b)),
                            false,
                        )
                        .unwrap();
                    if lookup.query == "not-found" {
                        assert!(result.search.rows.is_empty());
                    } else {
                        assert_eq!(lookup.query, "harbor");
                        assert_eq!(matched.members().count(), 1);
                        assert_eq!(result.search.rows.len(), 2);
                        assert!(result.search.rows.contains(&original_human));
                        assert!(result.search.rows.contains(&effective));
                        assert!(!result.search.rows.contains(&source));
                    }
                    observed.searches.push(result.search);
                }
                MentionStep::Need(MentionNeed::Names(raw)) => {
                    assert!(raw == original_human || raw == effective);
                    let names = names(&raw).unwrap();
                    assert_eq!(names.primary, "@harbor");
                    observed.names.push(names);
                }
                MentionStep::Need(MentionNeed::Normalization(value)) => {
                    // Synthetic bounded ASCII observation, not renderer replay.
                    assert!(value.is_ascii());
                    observed
                        .normalizations
                        .insert(value.clone(), value.to_lowercase());
                }
                MentionStep::Need(MentionNeed::History) => {
                    demands.push("history");
                    observed.history = Some(history.clone());
                }
                MentionStep::Need(MentionNeed::HistoryQuery(query)) => {
                    assert_eq!(query, "@harbor");
                    demands.push("history-query");
                    observed.history_queries.insert(query.clone(), query);
                }
                MentionStep::Need(MentionNeed::Clock) => {
                    demands.push("clock");
                    observed.now_ms = Some(NOW);
                }
                MentionStep::Complete { ops, choices } => {
                    complete = Some((ops, choices));
                    break;
                }
                other => panic!("unexpected observation demand: {other:?}"),
            }
        }
        assert_eq!(demands, ["history", "history-query", "clock"]);
        let (ops, choices) = complete.expect("mixed observation demands settle");
        let elements = if let Some(id) = winner {
            assert_eq!(choices.len(), 1);
            assert_eq!(choices[0].id, id);
            assert_eq!(choices[0].label, "@harbor");
            assert_eq!(
                choices[0].raw,
                if id.starts_with('S') {
                    effective.clone()
                } else {
                    original_human.clone()
                }
            );
            let semantic = if id.starts_with('S') {
                json!({"type":"usergroup","usergroup_id":id})
            } else {
                json!({"type":"user","user_id":id})
            };
            json!([{"type":"text","text":"Known "}, semantic, {"type":"text","text":" and @not-found"}])
        } else {
            assert!(choices.is_empty());
            assert_eq!(ops, input);
            json!([{"type":"text","text":"Known @harbor and @not-found"}])
        };
        let EncodingStep::Complete(blocks) = EncodePlan::capture(&ops, EditMode::PlainText, false)
            .unwrap()
            .advance(&EncodingObservations::default())
            .unwrap()
        else {
            panic!("member/group semantic encoding needs no unrelated observations")
        };
        assert_eq!(
            blocks,
            vec![
                json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":elements}]})
            ]
        );
        assert_eq!(
            observed.history.as_ref().unwrap().to_wire().unwrap(),
            history_before
        );
        assert_eq!(input, before);
        assert_eq!(humans[0], original_human);
        assert_eq!(groups[0].source(), &source);
        assert_eq!(groups[0].effective(), &effective);
    }
}
