use super::super::{file_matching::FileScored, file_title_source::Store, groups::Snapshot};
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
    state["offline"] = json!({"member":{},"channel":{},"mpim":{}});
    state["entityCtr"] = json!({"member":0.1,"channel":0.1,"mpim":0.1,"userGroup":0.2,"file":0.9});
    state
}

fn rank(
    state: &Value,
    members: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    files: &[FileScored<'_>],
) -> Result<Vec<Ranked>, Error> {
    let history = History::from_wire("{}").unwrap();
    let model = Model::from_weights(std::collections::BTreeMap::from([(
        "USER_ENTITY_TYPE_CTR_30DD".into(),
        3.0,
    )]))
    .unwrap();
    rank_mixed_files(
        "",
        members,
        groups,
        files,
        &Context {
            state,
            history: &history,
            model: &model,
            now_ms: 1760000000000.0,
            normalize: &|s| Ok(s.to_lowercase()),
        },
        &mut SimilarityCache::default(),
        |r| Ok(r["profile"]["display_name"].as_str().unwrap().into()),
        |a, b| Ok(a.cmp(b)),
        false,
    )
}

#[test]
fn files_share_the_member_group_shortlist_and_global_occurrence_indexes() {
    let members: Vec<_> = (0..30).map(|i| json!({"id":format!("U{i:08}"),"name":"Person","team_id":"T00000001",
        "profile":{"real_name":"Person","display_name":"Person"},"_real_name_normalized_lc":"person","_display_name_normalized_lc":"person"})).collect();
    let people: Vec<_> = members
        .iter()
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
        .collect();
    let group = Snapshot::capture(
        json!({"id":"S00000001"}),
        json!({"id":"S00000001","name":"Report","handle":"report","is_usergroup":true}),
    )
    .unwrap();
    let groups = [GroupScored {
        snapshot: &group,
        id: group.id(),
        score: 1,
        is_exact_match: Some(false),
    }];
    let mut store = Store::new();
    for i in 0..60 {
        store.ingest(&json!({"id":format!("F{i:08}"),"mode":"quip","filetype":"quip","name":"Canvas","title":"Canvas","user":"U00000001","created":1750000000})).unwrap();
    }
    let snapshots: Vec<_> = (0..60)
        .map(|i| store.candidate(&format!("F{i:08}")).unwrap())
        .collect();
    let files: Vec<_> = snapshots
        .iter()
        .map(|snapshot| FileScored {
            snapshot,
            id: snapshot.id(),
            score: 1,
            is_exact_match: Some(false),
        })
        .collect();
    let ranked = rank(&state(), &people, &groups, &files).unwrap();
    assert_eq!(ranked.len(), 25);
    assert!(ranked.iter().all(|r| r.row["numResults"] == 75));
    assert!(ranked.iter().all(|r| r.row["type"] == "file"));
    for result in ranked {
        assert!(result.original_index >= people.len() + groups.len());
        let original = &snapshots[result.original_index - people.len() - groups.len()];
        assert_eq!(result.row["id"], original.id());
        assert_eq!(result.row["suggestionType"], "file-navigation");
    }
}

#[test]
fn duplicate_file_occurrences_and_missing_file_observations_are_preserved() {
    let mut store = Store::new();
    store
        .ingest(
            &json!({"id":"F00000001","mode":"list","filetype":"list","name":"List","title":"List"}),
        )
        .unwrap();
    let snapshot = store.candidate("F00000001").unwrap();
    let files = [
        FileScored {
            snapshot: &snapshot,
            id: snapshot.id(),
            score: 0,
            is_exact_match: Some(true),
        },
        FileScored {
            snapshot: &snapshot,
            id: snapshot.id(),
            score: 0,
            is_exact_match: Some(true),
        },
    ];
    let mut state = state();
    let result = rank(&state, &[], &[], &files).unwrap();
    assert_eq!(
        result.iter().map(|r| r.original_index).collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(result.iter().all(|r| r.row["isExactMatch"] == true));
    state["entityCtr"].as_object_mut().unwrap().remove("file");
    assert!(matches!(
        rank(&state, &[], &[], &files),
        Err(Error::MissingObservation(_))
    ));
    assert!(rank(&state, &[], &[], &[]).unwrap().is_empty());
}
