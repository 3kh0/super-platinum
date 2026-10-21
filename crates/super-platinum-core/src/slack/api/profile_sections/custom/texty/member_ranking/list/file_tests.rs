//! The same ordinary effective file fixtures as the Profile mixed adapter,
//! retaining List context and global occurrence indexes across all categories.
use super::super::super::{file_matching::FileScored, file_title_source::Store, groups::Snapshot};
use super::*;
use crate::composer_emoji::{history::History, model::Model};

fn state() -> Value {
    let oracle: Value = serde_json::from_str(include_str!("../oracle.json")).unwrap();
    let mut state = oracle["cases"][0]["state"].clone();
    for key in [
        "ims",
        "metadata",
        "navigationHistory",
        "remoteDm",
        "remoteMpim",
        "affinity",
    ] {
        state[key] = json!({});
    }
    state["offline"] = json!({"member":{},"channel":{},"mpim":{}});
    state["entityCtr"] = json!({"member":0.1,"channel":0.1,"mpim":0.1,"userGroup":0.2,"file":0.9});
    state
}

fn member(index: usize) -> Value {
    json!({"id":format!("U{index:08}"),"name":"Person","team_id":"T00000001",
        "profile":{"real_name":"Person","display_name":"Person"},
        "_real_name_normalized_lc":"person","_display_name_normalized_lc":"person"})
}

fn members(rows: &[Value]) -> Vec<local::Scored<'_>> {
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

fn file_store(count: usize) -> Store {
    let mut store = Store::new();
    for i in 0..count {
        store
            .ingest(
                &json!({"id":format!("F{i:08}"),"mode":"quip","filetype":"quip",
            "name":"Canvas","title":"Canvas","user":"U00000001","created":1750000000}),
            )
            .unwrap();
    }
    store
}

fn group() -> Snapshot {
    Snapshot::capture(
        json!({"id":"S00000001"}),
        json!({"id":"S00000001","name":"Report","handle":"report","is_usergroup":true}),
    )
    .unwrap()
}

fn run(
    state: &Value,
    members: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    files: &[FileScored<'_>],
    list: &ListContext<'_>,
    legacy: bool,
) -> Result<Vec<Ranked>, Error> {
    let history = History::from_wire("{}").unwrap();
    let model = Model::from_weights(BTreeMap::from([
        ("USER_ENTITY_TYPE_CTR_30DD".into(), 3.0),
        ("MEMBER_OF_CURRENT_CHANNEL".into(), 10.0),
        ("USER_IS_IN_CURRENT_THREAD".into(), 20.0),
    ]))
    .unwrap();
    let context = Context {
        state,
        history: &history,
        model: &model,
        now_ms: 1760000000000.0,
        normalize: &|s| Ok(s.to_lowercase()),
    };
    let mut cache = SimilarityCache::default();
    let comparison = |r: &Value| Ok(r["profile"]["display_name"].as_str().unwrap().into());
    let collate = |a: &str, b: &str| Ok(a.cmp(b));
    if legacy {
        assert!(files.is_empty());
        rank_list_mixed(
            "", members, groups, list, &context, &mut cache, comparison, collate, false,
        )
    } else {
        rank_list_mixed_files(
            "", members, groups, files, list, &context, &mut cache, comparison, collate, false,
        )
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
fn mixed_files_preserve_list_context_and_duplicate_global_occurrences() {
    let people = [member(1), member(2), member(2)];
    let matched = members(&people);
    let group = group();
    let groups = [GroupScored {
        snapshot: &group,
        id: group.id(),
        score: 1,
        is_exact_match: Some(false),
    }];
    let store = file_store(1);
    let file = store.candidate("F00000000").unwrap();
    let files = [0, 1].map(|_| FileScored {
        snapshot: &file,
        id: file.id(),
        score: 1,
        is_exact_match: Some(false),
    });
    let channel = json!({"id":"C00000001","is_file":true});
    let membership = BTreeMap::from([("U00000001".into(), false), ("U00000002".into(), true)]);
    let users = ["U00000002".into()];
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &users,
        channel_membership: &membership,
    };
    let result = run(&state(), &matched, &groups, &files, &list, false).unwrap();
    assert_eq!(result.len(), 6);
    assert_eq!(
        result[..2]
            .iter()
            .map(|r| r.original_index)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    for row in &result[..2] {
        assert_eq!(feature(row, "MEMBER_OF_CURRENT_CHANNEL"), 1.0);
        assert_eq!(feature(row, "USER_IS_IN_CURRENT_THREAD"), 1.0);
    }
    let unpromoted = result.iter().find(|r| r.original_index == 0).unwrap();
    assert_eq!(feature(unpromoted, "MEMBER_OF_CURRENT_CHANNEL"), 0.0);
    assert_eq!(feature(unpromoted, "USER_IS_IN_CURRENT_THREAD"), 0.0);
    assert_eq!(
        result
            .iter()
            .filter(|r| r.row["type"] == "file")
            .map(|r| r.original_index)
            .collect::<Vec<_>>(),
        [4, 5]
    );
    assert_eq!(
        result.iter().find(|r| r.original_index == 3).unwrap().row["id"],
        group.id()
    );
    for row in result.iter().filter(|r| r.row["type"] == "file") {
        assert_eq!(row.row["id"], file.id());
        assert_eq!(row.row["suggestionType"], "file-navigation");
    }
    assert!(result.iter().all(|r| r.row["numResults"] == 6));
}

#[test]
fn list_context_and_files_share_one_75_shortlist_and_25_result_limit() {
    let people = (0..30).map(member).collect::<Vec<_>>();
    let matched = members(&people);
    let group = group();
    let groups = [GroupScored {
        snapshot: &group,
        id: group.id(),
        score: 1,
        is_exact_match: Some(false),
    }];
    let store = file_store(60);
    let snapshots = (0..60)
        .map(|i| store.candidate(&format!("F{i:08}")).unwrap())
        .collect::<Vec<_>>();
    let files = snapshots
        .iter()
        .map(|snapshot| FileScored {
            snapshot,
            id: snapshot.id(),
            score: 1,
            is_exact_match: Some(false),
        })
        .collect::<Vec<_>>();
    let channel = json!({"id":"C00000001","is_file":true});
    let membership = people
        .iter()
        .map(|p| (p["id"].as_str().unwrap().to_owned(), p["id"] == "U00000000"))
        .collect();
    let users = ["U00000000".into()];
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &users,
        channel_membership: &membership,
    };
    let result = run(&state(), &matched, &groups, &files, &list, false).unwrap();
    assert_eq!(result.len(), 25);
    assert!(result.iter().all(|r| r.row["numResults"] == 75));
    assert_eq!(result[0].original_index, 0);
    assert_eq!(feature(&result[0], "USER_IS_IN_CURRENT_THREAD"), 1.0);
    assert_eq!(
        result.iter().filter(|r| r.row["type"] == "file").count(),
        24
    );
    for row in result.iter().filter(|r| r.row["type"] == "file") {
        let index = row
            .original_index
            .checked_sub(matched.len() + groups.len())
            .unwrap();
        assert_eq!(row.row["id"], snapshots[index].id());
    }
}

#[test]
fn empty_file_compatibility_keeps_all_list_scores_features_and_rows() {
    let people = [member(1), member(2)];
    let matched = members(&people);
    let group = group();
    let groups = [GroupScored {
        snapshot: &group,
        id: group.id(),
        score: 1,
        is_exact_match: Some(false),
    }];
    let membership = BTreeMap::new();
    let users = ["U00000002".into()];
    let list = ListContext {
        current_channel: None,
        current_thread_users: &users,
        channel_membership: &membership,
    };
    let state = state();
    let old = run(&state, &matched, &groups, &[], &list, true).unwrap();
    let new = run(&state, &matched, &groups, &[], &list, false).unwrap();
    assert_eq!(old.len(), new.len());
    for (a, b) in old.iter().zip(new) {
        assert_eq!(a.original_index, b.original_index);
        assert_eq!(a.row, b.row);
        assert_eq!(a.features, b.features);
        assert_eq!(a.model_score, b.model_score);
    }
}

#[test]
fn files_do_not_fill_missing_list_membership_or_file_observations() {
    let people = [member(1)];
    let matched = members(&people);
    let store = file_store(1);
    let snapshot = store.candidate("F00000000").unwrap();
    let files = [FileScored {
        snapshot: &snapshot,
        id: snapshot.id(),
        score: 1,
        is_exact_match: Some(false),
    }];
    let channel = json!({"id":"C00000001","is_file":true});
    let missing = BTreeMap::new();
    let list = ListContext {
        current_channel: Some(&channel),
        current_thread_users: &[],
        channel_membership: &missing,
    };
    assert!(matches!(
        run(&state(), &matched, &[], &files, &list, false),
        Err(Error::MissingObservation(_))
    ));
    let observed = BTreeMap::from([("U00000001".into(), false)]);
    let list = ListContext {
        channel_membership: &observed,
        ..list
    };
    let mut state = state();
    assert!(run(&state, &matched, &[], &files, &list, false).is_ok());
    state["entityCtr"].as_object_mut().unwrap().remove("file");
    assert!(matches!(
        run(&state, &matched, &[], &files, &list, false),
        Err(Error::MissingObservation(_))
    ));
    assert!(run(&state, &matched, &[], &[], &list, false).is_ok());
}
