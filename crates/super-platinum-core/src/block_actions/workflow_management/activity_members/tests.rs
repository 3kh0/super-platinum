use super::*;
use crate::{
    block_actions::{
        app_eligibility::agents::{Access, Profile, Settings},
        home::workflow::directory_local as local,
    },
    composer_emoji::{history::History, model::Model, ranking::SimilarityCache},
    slack::client::RequestBody,
};
use std::{collections::BTreeMap, time::Duration};
fn owner() -> WorkspaceSession {
    WorkspaceSession {
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
#[test]
fn actual_activity_member_predicate_including_agent_exceptions() {
    for case in oracle()["members"].as_array().unwrap() {
        let label = case["label"].as_str().unwrap();
        let mut raw = case["profile"].clone();
        raw["id"] = json!("A1");
        let profile = Profile::decode("A1", &json!({"ok":true,"app_profile":raw})).unwrap();
        let prefs = if label == "agentforce-default" {
            json!({"default_channel_knowledge_agent":{"app_id":"A1"}})
        } else {
            json!({})
        };
        let accessible = if label == "agentforce-accessible" {
            BTreeSet::from(["A1".into()])
        } else {
            BTreeSet::new()
        };
        let access = Access::new(
            Settings::decode(&prefs, None).unwrap(),
            accessible,
            BTreeMap::new(),
        );
        let member = Member::decode(&case["user"]).unwrap();
        assert_eq!(
            member.eligibility(
                &owner().user_id,
                Some(label != "agentforce-nonclient"),
                Some(&profile),
                Some(&access)
            ) == Eligibility::Allowed,
            case["eligible"].as_bool().unwrap(),
            "{label}"
        );
        assert_eq!(member.raw(), &case["user"]);
    }
}
#[test]
fn missing_agent_observations_are_not_member_grants() {
    let user = json!({"id":"U00000001","is_bot":true,"profile":{"api_app_id":"A1"}});
    let member = Member::decode(&user).unwrap();
    assert_eq!(
        member.eligibility(&owner().user_id, Some(true), None, None),
        Eligibility::NeedsProfile { app: "A1".into() }
    );
    let mut agent = user.clone();
    agent["is_agentforce_bot"] = json!(true);
    let member = Member::decode(&agent).unwrap();
    assert_eq!(
        member.eligibility(&owner().user_id, None, None, None),
        Eligibility::NeedsClientContext
    );
    assert_eq!(
        member.eligibility(&owner().user_id, Some(true), None, None),
        Eligibility::NeedsAgentAccess
    );
    let mut me = user;
    me["id"] = json!(owner().user_id);
    assert_eq!(
        Member::decode(&me)
            .unwrap()
            .eligibility(&owner().user_id, None, None, None),
        Eligibility::Denied
    );
    assert!(Member::decode(&json!({"id":"U00000001","is_bot":"false"})).is_err());
}
#[test]
fn actual_two_stage_query_formatting_is_not_generic_trim() {
    for case in oracle()["queries"].as_array().unwrap() {
        let query = Query::capture(&owner(), case["raw"].as_str().unwrap()).unwrap();
        assert_eq!(query.text(SearchPath::Remote), case["remote"]);
        assert_eq!(query.text(SearchPath::Local), case["local"]);
        assert_eq!(query.raw(), case["raw"]);
    }
}
#[test]
fn actual_remote_request_keeps_agents_and_exact_logical_owner() {
    let w = owner();
    let now = Instant::now();
    let client = SlackClient::default();
    let endpoint = DirectoryEndpoint::decode(
        &w,
        &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/T00000001"}),
        now,
    )
    .unwrap();
    for case in oracle()["requests"].as_array().unwrap() {
        let query = Query::capture(&w, case["query"].as_str().unwrap()).unwrap();
        let request = query.request(&client, &w, &endpoint, now, None).unwrap();
        let RequestBody::Json(mut body) = request.body else {
            panic!("directory JSON request")
        };
        assert_eq!(
            body.as_object_mut().unwrap().remove("token"),
            Some(json!("synthetic"))
        );
        assert_eq!(body, case["captured"][0]["query"]);
        let mut replaced = w.clone();
        replaced.token = "replacement".into();
        assert!(!query.current(&replaced));
        assert!(
            query
                .request(&client, &replaced, &endpoint, now, None)
                .is_err()
        );
        assert!(
            query
                .request(&client, &w, &endpoint, now + Duration::from_secs(61), None)
                .is_err()
        );
        assert!(
            query
                .request(&client, &w, &endpoint, now, Some(""))
                .is_err()
        );
    }
}
#[test]
fn actual_paging_preserves_order_overflow_and_original_pages() {
    for case in oracle()["paging"].as_array().unwrap() {
        let mut pages = Pages::default();
        for raw in case["initial"].as_array().unwrap() {
            let keys = raw["results"]
                .is_object()
                .then(|| vec!["z".into(), "a".into()]);
            let page = Page::decode_ordered(raw.clone(), keys.as_deref()).unwrap();
            assert_eq!(page.raw(), raw);
            pages.append(page).unwrap();
        }
        assert_eq!(
            pages.rows().unwrap(),
            case["result"]["objects"].as_array().unwrap()
        );
        assert_eq!(pages.marker(), case["result"]["next_marker"].as_str());
    }
}
#[test]
fn incomplete_or_malformed_pages_cannot_become_complete_results() {
    let raw = json!({"results":[{"id":"U00000001"}],"next_marker":"again"});
    let mut pages = Pages::default();
    pages.append(Page::decode(raw.clone()).unwrap()).unwrap();
    assert!(pages.rows().is_err());
    assert!(pages.append(Page::decode(raw).unwrap()).is_err());
    assert_eq!(pages.pages().len(), 1);
    assert_eq!(pages.marker(), Some("again"));
    for error in [json!(""), json!(false), json!({})] {
        assert!(Page::decode(json!({"ok":true,"error":error,"results":[]})).is_err());
    }
    let object = json!({"results":{"z":[{"id":"U00000001"}],"a":[{"id":"U00000002"}]}});
    assert!(Page::decode(object.clone()).is_err());
    assert!(Page::decode_ordered(object, Some(&["z".into(), "z".into()])).is_err());
}
#[test]
fn exact_visible_external_normalization_does_not_replace_profile_only_rows() {
    let raw = json!({"results":[{"id":"U00000001","is_stranger":true,"profile":{"display_name":"Retain"}},{"id":"U00000002","is_stranger":true,"is_profile_only_user":true}]});
    let mut pages = Pages::default();
    pages.append(Page::decode(raw.clone()).unwrap()).unwrap();
    assert_eq!(
        pages.normalized_rows(false).unwrap(),
        raw["results"].as_array().unwrap().clone()
    );
    let normalized = pages.normalized_rows(true).unwrap();
    assert_eq!(normalized[0]["is_stranger"], false);
    assert_eq!(normalized[1]["is_stranger"], true);
    assert_eq!(normalized[0]["profile"], raw["results"][0]["profile"]);
    assert_eq!(pages.pages()[0].raw(), &raw);
}
#[test]
fn ranking_retains_duplicate_candidate_indices_and_discards_wire_facts() {
    let query = Query::capture(&owner(), "").unwrap();
    let prepared = local::prepare_query("", str::to_lowercase);
    let rows:Vec<_>=(0..22).map(|i|json!({"id":"U00000001","profile":{"display_name":format!("Name{i:02}")},"facts":{"member":false,"conversation":true,"comparisonName":"Injected"},"local_index":i})).collect();
    let matched = match_members(
        &query,
        SearchPath::Local,
        &prepared,
        &rows,
        &BTreeSet::new(),
        &MatchFacts {
            query_is_maybe_self: false,
            can_match_beyond_word_boundaries: false,
            passthrough_remote_fuzzy_filtering: false,
        },
        |_| true,
    );
    assert_eq!(matched.len(), 22);
    let names: Vec<_> = (0..22).map(|i| format!("Name{i:02}")).collect();
    let state = json!({"selfId":"U00000999"});
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    let context = RankInputs {
        state: &state,
        history: &history,
        model: &model,
        now_ms: 1.0,
    };
    let plan =
        ObservationPlan::capture("fixture", &query, SearchPath::Local, &matched, &names).unwrap();
    let reply = RendererReply {
        generation: "fixture".into(),
        locale: "en-US".into(),
        collation_locale: "en-US".into(),
        lowered: plan
            .normalization_fields()
            .iter()
            .map(|s| s.to_lowercase())
            .collect(),
        classes: (0..plan.collation_names().len()).map(|i| (i, i)).collect(),
        demote_passthrough: false,
    };
    let observations = plan.decode("en-US", reply).unwrap();
    let order = rank_members(
        &query,
        SearchPath::Local,
        &matched,
        &names,
        &context,
        &observations,
        &mut SimilarityCache::default(),
    )
    .unwrap();
    assert_eq!(order, (0..20).collect::<Vec<_>>());
    assert_eq!(rows[0]["facts"]["comparisonName"], "Injected");
    assert!(
        rank_members(
            &query,
            SearchPath::Local,
            &matched,
            &names[..21],
            &context,
            &observations,
            &mut SimilarityCache::default(),
        )
        .is_err()
    );
}

#[test]
fn malformed_profile_only_flags_refuse_before_any_normalized_upsert_rows() {
    for flag in [json!("true"), json!(1), json!({}), json!([]), json!(null)] {
        let raw = json!({"results":[
            {"id":"U00000001","is_stranger":true},
            {"id":"U00000002","is_stranger":true,"is_profile_only_user":flag}
        ]});
        let mut pages = Pages::default();
        pages.append(Page::decode(raw.clone()).unwrap()).unwrap();
        for normalize in [false, true] {
            assert!(pages.normalized_rows(normalize).is_err());
        }
        assert_eq!(pages.pages()[0].raw(), &raw);
        assert_eq!(pages.rows().unwrap(), raw["results"].as_array().unwrap());
    }
}

#[test]
fn original_query_rejects_each_changed_authentication_identity_before_building_request() {
    let original = owner();
    let query = Query::capture(&original, "@Ada").unwrap();
    let client = SlackClient::default();
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &original,
        &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/T00000001"}),
        now,
    )
    .unwrap();
    for changed in 0..5 {
        let mut replacement = original.clone();
        match changed {
            0 => replacement.team_id = "T00000002".into(),
            1 => replacement.user_id = "U00000002".into(),
            2 => replacement.enterprise_id = Some("E00000001".into()),
            3 => replacement.token = "synthetic-replacement".into(),
            _ => replacement.url = "https://replacement.test".into(),
        }
        assert!(!query.current(&replacement));
        assert!(
            query
                .request(&client, &replacement, &endpoint, now, None)
                .is_err()
        );
    }
    // Owner-currentness is not a source grant to trim the user's query.
    assert_eq!(
        Query::capture(&original, " @Ada ")
            .unwrap()
            .text(SearchPath::Remote),
        " @Ada "
    );
    assert!(Query::capture(&original, "a\0b").is_err());
    assert!(Query::capture(&original, &"x".repeat(4097)).is_err());
}

#[test]
fn bounded_incomplete_paging_refuses_atomically_but_accepts_a_final_fiftieth_page() {
    let mut pages = Pages::default();
    for n in 0..49 {
        pages
            .append(Page::decode(json!({"results":[],"next_marker":format!("page-{n}")})).unwrap())
            .unwrap();
    }
    assert!(!pages.complete());
    assert_eq!(pages.pages().len(), 49);
    assert_eq!(pages.marker(), Some("page-48"));
    assert!(
        pages
            .append(Page::decode(json!({"results":[],"next_marker":"page-49"})).unwrap())
            .is_err()
    );
    assert_eq!(pages.pages().len(), 49);
    assert_eq!(pages.marker(), Some("page-48"));
    assert!(pages.rows().is_err());
    pages
        .append(
            Page::decode(json!({"results":[{"id":"U00000001","opaque":{"retained":true}}]}))
                .unwrap(),
        )
        .unwrap();
    assert!(pages.complete());
    assert_eq!(pages.pages().len(), 50);
    assert_eq!(pages.rows().unwrap()[0]["opaque"], json!({"retained":true}));
    assert!(
        pages
            .append(Page::decode(json!({"results":[]})).unwrap())
            .is_err()
    );
    assert_eq!(pages.rows().unwrap().len(), 1);
}

#[test]
fn source_overflow_marker_and_duplicate_order_do_not_become_deduplicated_display_rows() {
    let first: Vec<_> = (0..19)
        .map(|n| json!({"id":"U00000001","occurrence":n}))
        .collect();
    let tail = json!({"results":[
        {"id":"U00000002","opaque":"included"},
        {"id":"U00000003","opaque":"first omitted"},
        {"id":"U00000004","opaque":"also omitted"}],"next_marker":"server-marker"});
    let mut pages = Pages::default();
    pages
        .append(Page::decode(json!({"results":first,"next_marker":"continue"})).unwrap())
        .unwrap();
    pages.append(Page::decode(tail.clone()).unwrap()).unwrap();
    assert_eq!(pages.rows().unwrap().len(), LIMIT);
    assert_eq!(pages.rows().unwrap()[18]["occurrence"], 18);
    assert_eq!(pages.rows().unwrap()[19]["id"], "U00000002");
    assert_eq!(pages.marker(), Some("U00000003"));
    assert_eq!(pages.pages()[1].raw(), &tail);
}
