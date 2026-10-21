use super::*;
use crate::composer_emoji::{history::History, model::Model};

fn member(index: usize) -> Value {
    json!({"id":format!("U{index:08}"),"name":"Person","team_id":"T00000001",
        "profile":{"real_name":"Person","display_name":"Person"},
        "_real_name_normalized_lc":"person","_display_name_normalized_lc":"person"})
}

fn run(rows: &[Value], list: Option<&ListContext<'_>>) -> Result<Vec<Ranked>, Error> {
    let state = json!({"selfId":"U99999999","self":{"facts":{"team":"T00000001"}},
        "ims":{},"metadata":{},"navigationHistory":{},"enterprise":null,
        "enterpriseId":null,"teamId":"T00000001","offline":false,
        "experiments":{"autocomplete_affinity_score":"off","autocomplete_affinity_ml_model":"off"}});
    let history = History::from_wire("{}").unwrap();
    let model = Model::from_weights(BTreeMap::from([
        ("MEMBER_OF_CURRENT_CHANNEL".into(), 10.0),
        ("USER_IS_IN_CURRENT_THREAD".into(), 20.0),
    ]))
    .unwrap();
    let normalize = |s: &str| Ok(s.to_lowercase());
    let context = Context {
        state: &state,
        history: &history,
        model: &model,
        now_ms: 1760000000000.0,
        normalize: &normalize,
    };
    let matched = rows
        .iter()
        .map(|item| local::Scored {
            item,
            id: item["id"].as_str().unwrap(),
            score: 0,
            score_details: None,
            is_exact_match: None,
            exact_match_details: None,
            is_self: false,
            is_flannel_fuzzy_passthrough: false,
        })
        .collect::<Vec<_>>();
    let mut cache = SimilarityCache::default();
    let comparison = |item: &Value| Ok(item["profile"]["display_name"].as_str().unwrap().into());
    let collate = |a: &str, b: &str| Ok(a.cmp(b));
    match list {
        Some(list) => rank_list_members(
            "", &matched, list, &context, &mut cache, comparison, collate, false,
        ),
        None => rank_members(
            "", &matched, &context, &mut cache, comparison, collate, false,
        ),
    }
}

fn feature(row: &Ranked, key: &str) -> f64 {
    row.features
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
}

#[test]
fn observed_absent_channel_and_empty_root_preserve_profile_ranking_and_caps() {
    let rows: Vec<_> = (0..90).map(member).collect();
    let membership = BTreeMap::new();
    let list = ListContext {
        current_channel: None,
        current_thread_users: &[],
        channel_membership: &membership,
    };
    let profile = run(&rows, None).unwrap();
    let actual = run(&rows, Some(&list)).unwrap();
    assert_eq!(actual.len(), 25);
    for (actual, profile) in actual.iter().zip(profile) {
        assert_eq!(actual.original_index, profile.original_index);
        assert_eq!(actual.model_score, profile.model_score);
        assert_eq!(actual.features, profile.features);
        assert_eq!(actual.row["numResults"], 75);
        assert!(actual.row.get("isMemberOfCurrentChannel").is_none());
        assert_eq!(actual.row["isMemberOfCurrentThread"], false);
    }
}

#[test]
fn original_root_reply_users_promote_only_observed_users_and_keep_duplicates() {
    let rows = vec![member(1), member(2), member(2)];
    let users = vec!["U00000002".into(), "U00000002".into()];
    let membership = BTreeMap::new();
    let list = ListContext {
        current_channel: None,
        current_thread_users: &users,
        channel_membership: &membership,
    };
    let actual = run(&rows, Some(&list)).unwrap();
    assert_eq!(
        actual.iter().map(|r| r.original_index).collect::<Vec<_>>(),
        [1, 2, 0]
    );
    assert_eq!(feature(&actual[0], "USER_IS_IN_CURRENT_THREAD"), 1.0);
    assert_eq!(feature(&actual[2], "USER_IS_IN_CURRENT_THREAD"), 0.0);
    assert!(
        actual
            .iter()
            .all(|r| feature(r, "MEMBER_OF_CURRENT_CHANNEL") == 0.0)
    );
}

#[test]
fn present_virtual_channel_uses_explicit_membership_and_rejects_wire_facts() {
    let mut rows = vec![member(1), member(2)];
    rows[0]["facts"] = json!({"currentChannelMember":true});
    rows[1]["facts"] = json!({"currentChannelMember":false});
    rows[0]["presence"] = json!("active");
    rows[1]["presence"] = json!("away");
    let before = rows.clone();
    // Virtual List channel has no ordinary conversation shape. Incoming facts
    // cannot turn it into an IM and suppress the explicit membership feature.
    let channel = json!({"id":"C00000001","is_file":true,"facts":{"im":true}});
    let membership = BTreeMap::from([("U00000001".into(), false), ("U00000002".into(), true)]);
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &[],
        channel_membership: &membership,
    };
    let actual = run(&rows, Some(&list)).unwrap();
    assert_eq!(actual[0].original_index, 1);
    assert_eq!(actual[0].row["isMemberOfCurrentChannel"], true);
    assert_eq!(
        actual[0].row["bonusPointMap"]["isMemberOfThisChannel"],
        25.0
    );
    assert_eq!(actual[1].row["isMemberOfCurrentChannel"], false);
    assert_eq!(feature(&actual[1], "NOT_MEMBER_OF_CURRENT_CHANNEL"), 1.0);
    assert_eq!(rows, before);
    // The existing Profile entry does not consume these caller observations.
    assert!(run(&rows, None).unwrap().iter().all(|r| {
        r.row.get("isMemberOfCurrentChannel").is_none()
            && r.row["item"]["facts"].get("currentChannelMember").is_none()
    }));
}

#[test]
fn missing_channel_membership_is_not_false_and_missing_channel_is_not_null_row() {
    let rows = vec![member(1), member(2)];
    let channel = json!({"id":"C00000001","is_file":true});
    let membership = BTreeMap::from([("U00000001".into(), false)]);
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &[],
        channel_membership: &membership,
    };
    assert!(matches!(
        run(&rows, Some(&list)),
        Err(Error::MissingObservation(_))
    ));
    for bad in [Value::Null, json!({}), json!({"id":""})] {
        let list = ListContext {
            current_channel: Some(&bad),
            ..list
        };
        assert_eq!(run(&rows, Some(&list)).unwrap_err(), Error::InvalidInput);
    }
}

#[test]
fn membership_bonus_keeps_source_presence_gate_but_model_membership_does_not() {
    let mut rows = vec![member(1), member(2)];
    rows[1]["presence"] = json!("away");
    let channel = json!({"id":"C00000001","is_file":true});
    let membership = BTreeMap::from([("U00000001".into(), true), ("U00000002".into(), true)]);
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &[],
        channel_membership: &membership,
    };
    let actual = run(&rows, Some(&list)).unwrap();
    for row in actual {
        assert_eq!(feature(&row, "MEMBER_OF_CURRENT_CHANNEL"), 1.0);
        let bonus = &row.row["bonusPointMap"]["isMemberOfThisChannel"];
        if row.original_index == 0 {
            assert!(bonus.is_null());
        } else {
            assert_eq!(*bonus, 25.0);
        }
    }
}

#[test]
fn observed_unknown_channel_placeholder_is_not_replaced_by_absence() {
    // EmXR.OL may return this original placeholder for a valid missing ID.
    // 1IsG.B observes absent membership cache and returns false, without
    // establishing known nonmembership or a complete channel member list.
    let rows = vec![member(1)];
    let channel = json!({"id":"C00000001","isUnknown":true});
    let membership = BTreeMap::from([("U00000001".into(), false)]);
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &[],
        channel_membership: &membership,
    };
    let actual = run(&rows, Some(&list)).unwrap();
    assert_eq!(actual[0].row["isMemberOfCurrentChannel"], false);
    assert_eq!(feature(&actual[0], "NOT_MEMBER_OF_CURRENT_CHANNEL"), 1.0);
}
