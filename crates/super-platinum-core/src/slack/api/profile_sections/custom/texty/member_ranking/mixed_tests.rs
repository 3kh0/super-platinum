use super::super::groups::Snapshot;
use super::*;
use crate::composer_emoji::{history::History, model::Model};

fn state() -> Value {
    let data: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let mut state = data["cases"][0]["state"].clone();
    state["ims"] = json!({});
    state["metadata"] = json!({});
    state["navigationHistory"] = json!({});
    state["remoteDm"] = json!({});
    state["remoteMpim"] = json!({});
    state["affinity"] = json!({});
    state["entityCtr"] = json!({"member":0.1,"userGroup":0.9});
    state
}
fn member(index: usize) -> Value {
    json!({"id":format!("U{index:08}"),"name":"Person","team_id":"T00000001","profile":{"real_name":"Person","display_name":"Person"},"_real_name_normalized_lc":"person","_display_name_normalized_lc":"person"})
}
fn scored(rows: &[Value]) -> Vec<local::Scored<'_>> {
    rows.iter()
        .map(|item| local::Scored {
            item,
            id: item["id"].as_str().unwrap(),
            score: 1,
            score_details: None,
            is_exact_match: None,
            exact_match_details: None,
            is_self: false,
            is_flannel_fuzzy_passthrough: false,
        })
        .collect()
}
fn group(index: usize) -> Snapshot {
    let id = format!("S{index:08}");
    Snapshot::capture(json!({"id":id,"name":"Incoming partial"}),json!({"id":id,"name":"Report people","handle":"report","is_usergroup":true,"_name_lc":"report people","opaque":{"retained":true},"facts":{"member":true,"self":true,"comparisonName":"injected"}})).unwrap()
}
fn groups(rows: &[Snapshot]) -> Vec<GroupScored<'_>> {
    rows.iter()
        .map(|snapshot| GroupScored {
            snapshot,
            id: snapshot.id(),
            score: 1,
            is_exact_match: Some(false),
        })
        .collect()
}
fn run(
    state: &Value,
    members: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    demote: bool,
) -> Result<Vec<Ranked>, Error> {
    run_query("", state, members, groups, demote)
}
fn run_query(
    query: &str,
    state: &Value,
    members: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    demote: bool,
) -> Result<Vec<Ranked>, Error> {
    let history = History::from_wire("{}").unwrap();
    let model = Model::from_weights(std::collections::BTreeMap::from([(
        "USER_ENTITY_TYPE_CTR_30DD".into(),
        3.0,
    )]))
    .unwrap();
    let normalize = |s: &str| Ok(s.to_lowercase());
    rank_mixed(
        query,
        members,
        groups,
        &Context {
            state,
            history: &history,
            model: &model,
            now_ms: 1760000000000.0,
            normalize: &normalize,
        },
        &mut SimilarityCache::default(),
        |raw| Ok(raw["profile"]["display_name"].as_str().unwrap().into()),
        |a, b| Ok(a.cmp(b)),
        demote,
    )
}

#[test]
fn both_categories_share_shortlist_and_final_cap_before_global_mapping() {
    for count in [40, 80] {
        let raw: Vec<_> = (0..count).map(member).collect();
        let snapshots: Vec<_> = (0..count).map(group).collect();
        let result = run(&state(), &scored(&raw), &groups(&snapshots), false).unwrap();
        assert_eq!(result.len(), 25);
        assert!(result.iter().all(|r| r.row["numResults"] == 75));
        // The source -25 group penalty puts members first in the initial
        // shared shortlist. The observed model then promotes retained groups.
        if count == 40 {
            assert!(result.iter().all(|r| r.row["type"] == "userGroup"));
            assert_eq!(
                result.iter().map(|r| r.original_index).collect::<Vec<_>>(),
                (40..65).collect::<Vec<_>>()
            );
        } else {
            assert!(result.iter().all(|r| r.row["type"] == "member"));
            assert!(result.iter().all(|r| r.original_index < 75));
        }
        for r in result {
            let expected = if r.original_index < raw.len() {
                &raw[r.original_index]
            } else {
                snapshots[r.original_index - raw.len()].effective()
            };
            assert_eq!(r.row["id"], expected["id"]);
        }
    }
}

#[test]
fn effective_groups_keep_duplicate_occurrences_exact_flags_and_safe_facts() {
    let raw = vec![member(1)];
    let snapshot = group(2);
    let matches = vec![
        GroupScored {
            snapshot: &snapshot,
            id: snapshot.id(),
            score: 0,
            is_exact_match: Some(true),
        },
        GroupScored {
            snapshot: &snapshot,
            id: snapshot.id(),
            score: 0,
            is_exact_match: Some(true),
        },
    ];
    let result = run(&state(), &scored(&raw), &matches, false).unwrap();
    let mut indexes = vec![];
    for result in result.iter().filter(|r| r.row["type"] == "userGroup") {
        indexes.push(result.original_index);
        assert_eq!(result.row["item"]["name"], "Report people");
        assert_eq!(result.row["item"]["opaque"], json!({"retained":true}));
        assert_eq!(result.row["item"]["facts"]["userGroup"], true);
        assert_eq!(result.row["item"]["facts"]["member"], false);
        assert_eq!(result.row["isExactMatch"], true);
        assert_eq!(result.row["bonusPointMap"]["isUserGroupOrKeyword"], -25.0);
    }
    indexes.sort();
    assert_eq!(indexes, [1, 2]);
}

#[test]
fn required_group_ctr_and_occurrence_identity_are_never_defaulted() {
    let snapshots = vec![group(1)];
    let matched = groups(&snapshots);
    let mut state = state();
    state["entityCtr"]
        .as_object_mut()
        .unwrap()
        .remove("userGroup");
    assert!(matches!(
        run(&state, &[], &matched, false),
        Err(Error::MissingObservation(_))
    ));
    assert!(run(&state, &[], &[], false).unwrap().is_empty());
    state["offline"] = Value::Null;
    assert!(run(&state, &[], &matched, false).is_ok());
    let invalid = vec![GroupScored {
        snapshot: &snapshots[0],
        id: "S99999999",
        score: 0,
        is_exact_match: None,
    }];
    assert_eq!(
        run(&state, &[], &invalid, false).unwrap_err(),
        Error::InvalidInput
    );
}

#[test]
fn member_comparison_and_passthrough_demotion_remain_shared() {
    let raw = vec![member(1)];
    let mut members = scored(&raw);
    members[0].is_flannel_fuzzy_passthrough = true;
    let snapshots = vec![group(1)];
    let groups = groups(&snapshots);
    let mut state = state();
    state["entityCtr"]["member"] = json!(1.0);
    state["entityCtr"]["userGroup"] = json!(0.0);
    let normal = run_query("report", &state, &members, &groups, false).unwrap();
    let demoted = run_query("report", &state, &members, &groups, true).unwrap();
    assert_eq!(normal[0].row["type"], "member");
    assert_eq!(demoted.last().unwrap().row["type"], "member");
    assert_eq!(normal[0].row["item"]["facts"]["comparisonName"], "Person");
}

#[test]
fn member_only_wrapper_matches_original_vectors_with_empty_group_slice() {
    let data: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in data["cases"].as_array().unwrap() {
        let raw = case["raw"].as_array().unwrap();
        let matched = super::tests::scored(raw, &case["scores"]);
        let history = History::from_wire("{}").unwrap();
        let model = Model::fallback();
        let normalize = |s: &str| {
            data["normalization"][s]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| missing("test normalize"))
        };
        let result = rank_mixed(
            case["query"].as_str().unwrap(),
            &matched,
            &[],
            &Context {
                state: &case["state"],
                history: &history,
                model: &model,
                now_ms: case["state"]["now"].as_f64().unwrap(),
                normalize: &normalize,
            },
            &mut SimilarityCache::default(),
            |raw| Ok(raw["profile"]["display_name"].as_str().unwrap().into()),
            |a, b| {
                data["collation"][a][b]
                    .as_i64()
                    .map(|n| n.cmp(&0))
                    .ok_or_else(|| missing("test collate"))
            },
            false,
        )
        .unwrap();
        let wrapped = super::tests::run(case, &case["state"], raw, &data).unwrap();
        assert_eq!(result.len(), wrapped.len());
        for (a, b) in result.iter().zip(wrapped) {
            assert_eq!(a.original_index, b.original_index);
            assert_eq!(a.row, b.row);
            assert_eq!(a.features, b.features);
            assert_eq!(a.model_score, b.model_score);
        }
    }
}

fn run_list(
    state: &Value,
    members: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    list: &ListContext<'_>,
) -> Result<Vec<Ranked>, Error> {
    let history = History::from_wire("{}").unwrap();
    let model = Model::from_weights(std::collections::BTreeMap::from([
        ("USER_ENTITY_TYPE_CTR_30DD".into(), 3.0),
        ("MEMBER_OF_CURRENT_CHANNEL".into(), 10.0),
        ("USER_IS_IN_CURRENT_THREAD".into(), 20.0),
    ]))
    .unwrap();
    let normalize = |s: &str| Ok(s.to_lowercase());
    rank_list_mixed(
        "",
        members,
        groups,
        list,
        &Context {
            state,
            history: &history,
            model: &model,
            now_ms: 1760000000000.0,
            normalize: &normalize,
        },
        &mut SimilarityCache::default(),
        |raw| Ok(raw["profile"]["display_name"].as_str().unwrap().into()),
        |a, b| Ok(a.cmp(b)),
        false,
    )
}

#[test]
fn list_mixed_keeps_original_channel_root_and_duplicate_occurrences() {
    let raw = vec![member(1), member(2), member(2)];
    let snapshots = vec![group(1), group(1)];
    let channel = json!({"id":"CLIST0001","is_file":true});
    let membership =
        std::collections::BTreeMap::from([("U00000001".into(), false), ("U00000002".into(), true)]);
    let thread_users = vec!["U00000002".into()];
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &thread_users,
        channel_membership: &membership,
    };
    let result = run_list(&state(), &scored(&raw), &groups(&snapshots), &list).unwrap();
    assert_eq!(result.len(), 5);
    assert_eq!(result[0].original_index, 1);
    assert_eq!(result[1].original_index, 2);
    let mut occurrences = result
        .iter()
        .map(|row| row.original_index)
        .collect::<Vec<_>>();
    occurrences.sort();
    assert_eq!(occurrences, [0, 1, 2, 3, 4]);
    for row in result {
        assert_eq!(row.row["numResults"], 5);
        if row.original_index < raw.len() {
            assert_eq!(row.row["id"], raw[row.original_index]["id"]);
            let contextual = row.original_index != 0;
            assert_eq!(row.row["isMemberOfCurrentChannel"], contextual);
            assert_eq!(row.row["isMemberOfCurrentThread"], contextual);
        } else {
            let group = &snapshots[row.original_index - raw.len()];
            assert_eq!(row.row["id"], group.id());
            assert_eq!(row.row["item"]["name"], "Report people");
            assert_eq!(row.row["item"]["opaque"], json!({"retained":true}));
            assert_eq!(row.row["item"]["facts"]["member"], false);
            assert_eq!(row.row["bonusPointMap"]["isUserGroupOrKeyword"], -25.0);
            assert!(row.row.get("isMemberOfCurrentChannel").is_none());
            assert!(row.row.get("isMemberOfCurrentThread").is_none());
        }
    }
}

#[test]
fn list_mixed_has_one_shortlist_and_final_cap_with_global_indexes() {
    let channel = json!({"id":"CLIST0001","is_file":true});
    for count in [40, 80] {
        let raw: Vec<_> = (0..count).map(member).collect();
        let snapshots: Vec<_> = (0..count).map(group).collect();
        let membership = raw
            .iter()
            .map(|row| (row["id"].as_str().unwrap().into(), false))
            .collect();
        let list = ListContext {
            current_channel: Some(&channel),
            current_thread_users: &[],
            channel_membership: &membership,
        };
        let result = run_list(&state(), &scored(&raw), &groups(&snapshots), &list).unwrap();
        assert_eq!(result.len(), 25);
        assert!(result.iter().all(|row| row.row["numResults"] == 75));
        if count == 40 {
            // Retained groups can win the model score only after the one shared
            // shortlist; their original indexes remain after all member rows.
            assert!(result.iter().all(|row| row.row["type"] == "userGroup"));
            assert_eq!(
                result
                    .iter()
                    .map(|row| row.original_index)
                    .collect::<Vec<_>>(),
                (40..65).collect::<Vec<_>>()
            );
        } else {
            // 80 members fill the initial shortlist. A separate group shortlist
            // would incorrectly allow the higher-CTR groups into final results.
            assert!(result.iter().all(|row| row.row["type"] == "member"));
            assert!(result.iter().all(|row| row.original_index < 75));
        }
    }
}

#[test]
fn list_mixed_refuses_missing_member_observation_or_group_identity() {
    let raw = vec![member(1)];
    let snapshots = vec![group(1)];
    let channel = json!({"id":"CLIST0001","isUnknown":true});
    let mut membership = std::collections::BTreeMap::new();
    {
        let list = ListContext {
            current_channel: Some(&channel),
            current_thread_users: &[],
            channel_membership: &membership,
        };
        assert!(matches!(
            run_list(&state(), &scored(&raw), &groups(&snapshots), &list),
            Err(Error::MissingObservation(_))
        ));
        // Group IDs need no fabricated member-membership observation.
        assert!(run_list(&state(), &[], &groups(&snapshots), &list).is_ok());
    }
    membership.insert("U00000001".into(), false);
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &[],
        channel_membership: &membership,
    };
    let invalid = [GroupScored {
        snapshot: &snapshots[0],
        id: "SOTHER001",
        score: 0,
        is_exact_match: None,
    }];
    assert_eq!(
        run_list(&state(), &scored(&raw), &invalid, &list).unwrap_err(),
        Error::InvalidInput
    );
    let mut missing_ctr = state();
    missing_ctr["entityCtr"]
        .as_object_mut()
        .unwrap()
        .remove("userGroup");
    assert!(matches!(
        run_list(&missing_ctr, &scored(&raw), &groups(&snapshots), &list),
        Err(Error::MissingObservation(_))
    ));
}
