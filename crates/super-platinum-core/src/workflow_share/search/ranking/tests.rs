use super::*;
fn member(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"real_name":name,"team_id":"T1","profile":{"real_name":name,"display_name":name},"_real_name_normalized_lc":name,"_display_name_normalized_lc":name,"_email_normalized_lc":""})
}
fn channel(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"is_channel":true,"is_member":true})
}
fn facts() -> MatchFacts {
    MatchFacts {
        query_is_maybe_self: false,
        can_match_beyond_word_boundaries: false,
        passthrough_remote_fuzzy_filtering: false,
    }
}
fn state(rows: &Rows) -> Value {
    // Explicit test observations of absent histories/metadata, never host defaults.
    let mut s = json!({"selfId":"U00000999","self":{"facts":{"team":"T1"}},"teamId":"T1","enterpriseId":null,"enterprise":false,"offline":null,"ims":{},"metadata":{},"navigationHistory":{},"experiments":{"autocomplete_affinity_score":"off","autocomplete_affinity_ml_model":"off"}});
    for i in rows.identities() {
        if i.kind == Kind::Members {
            s["ims"][&i.id] = Value::Null;
        } else {
            s["metadata"][&i.id] = Value::Null;
            s["navigationHistory"][&i.id] = Value::Null;
        }
    }
    s
}
fn observed(query: &str, rows: Rows, demote: bool) -> RankObservations {
    let plan = ObservationPlan::capture("original", query, rows).unwrap();
    let mut indices: Vec<_> = (0..plan.collation_names().len()).collect();
    // Captured en-US numeric ordering for the synthetic ASCII fixture names.
    indices.sort_by_key(|i| {
        let s = &plan.collation_names()[*i];
        (
            s.split(" item ").next().unwrap().to_owned(),
            s.split(" item ")
                .nth(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0),
        )
    });
    let reply = RendererReply {
        generation: "original".into(),
        locale: "en-US".into(),
        collation_locale: "en-US".into(),
        lowered: plan
            .normalization_fields()
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect(),
        classes: indices
            .into_iter()
            .enumerate()
            .map(|(rank, index)| (index, rank))
            .collect(),
        demote_passthrough: demote,
    };
    plan.decode("en-US", reply).unwrap()
}
fn execute(
    rows: &Rows,
    query: &str,
    s: &Value,
    model: &Model,
    demote: bool,
) -> Result<ResultSet, Error> {
    let history = History::from_wire("{}").unwrap();
    rank(
        query,
        rows,
        &RankInputs {
            state: s,
            history: &history,
            model,
            now_ms: 1_760_000_000_000.0,
        },
        &observed(query, rows.clone(), demote),
        &mut SimilarityCache::default(),
    )
}
#[test]
fn share_ranking_exact_retained_84_to_72_to_24_mixed_vector() {
    let oracle: Value = serde_json::from_str(include_str!("combined-oracle.json")).unwrap();
    let names: Vec<_> = (0..42)
        .map(|i| {
            if i % 3 == 0 {
                "review".into()
            } else {
                format!("review item {i}")
            }
        })
        .collect();
    let members: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, n)| member(&format!("U{i:08}"), n))
        .collect();
    let channels: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, n)| channel(&format!("C{i:08}"), n))
        .collect();
    let ms: Vec<_> = members
        .iter()
        .enumerate()
        .map(|(i, m)| local::Scored {
            item: m,
            id: m["id"].as_str().unwrap(),
            score: (i % 4) as u32,
            score_details: None,
            is_exact_match: Some(false),
            exact_match_details: None,
            is_self: false,
            is_flannel_fuzzy_passthrough: false,
        })
        .collect();
    let cs: Vec<_> = channels
        .iter()
        .enumerate()
        .map(|(i, c)| local::ChannelScored {
            item: c,
            id: c["id"].as_str().unwrap(),
            score: Some((i % 4) as u32),
            is_exact_match: Some(i % 3 == 0),
            previous_name_scores: None,
            best_previous_name_match: None,
            is_flannel_fuzzy_passthrough: false,
        })
        .collect();
    let rows = Rows::new(&ms, &names, &cs, &names).unwrap();
    let result = execute(&rows, "review", &state(&rows), &Model::fallback(), false).unwrap();
    assert_eq!(INITIAL_LIMIT, 72);
    assert_eq!(result.rows.len(), 24);
    assert_eq!(
        json!(
            result
                .rows
                .iter()
                .map(|r| &r.identity.id)
                .collect::<Vec<_>>()
        ),
        oracle["final24"]
    );
    for r in result.rows {
        let expected = oracle["sorted"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == r.identity.id)
            .unwrap();
        let score = expected["modelScore"].as_f64().unwrap();
        assert!(
            (r.model_score - score).abs() < 1e-10 * (1.0 + score.abs()),
            "{}: {} vs {}",
            r.identity.id,
            r.model_score,
            score
        );
        assert_eq!(
            r.identity.id[1..].parse::<usize>().unwrap(),
            r.identity.index
        );
    }
}
#[test]
fn share_ranking_remote_passthrough_requires_independent_gate_and_eligibility() {
    let members = vec![member("U00000001", "unrelated")];
    let remote = BTreeSet::from(["U00000001".into()]);
    let q = local::prepare_query("zzzzzzzz", str::to_owned);
    let mut f = facts();
    assert!(match_members("zzzzzzzz", &q, &members, &remote, &f, |_| true).is_empty());
    f.passthrough_remote_fuzzy_filtering = true;
    let m = match_members("zzzzzzzz", &q, &members, &remote, &f, |_| true);
    assert_eq!(m.len(), 1);
    assert!(m[0].is_flannel_fuzzy_passthrough);
    assert_eq!(m[0].score, 2);
    assert!(match_members("zzzzzzzz", &q, &members, &remote, &f, |_| false).is_empty());
}
#[test]
fn share_ranking_observation_coverage_and_original_rows_are_required() {
    let members = vec![member("U00000001", "review")];
    let q = local::prepare_query("review", str::to_owned);
    let m = match_members("review", &q, &members, &BTreeSet::new(), &facts(), |_| true);
    let rows = Rows::new(&m, &["review".into()], &[], &[]).unwrap();
    let mut s = state(&rows);
    s["ims"].as_object_mut().unwrap().clear();
    assert!(matches!(
        execute(&rows, "review", &s, &Model::fallback(), false),
        Err(Error::MissingObservation(_))
    ));
    let proof = observed("review", rows.clone(), false);
    assert!(proof.validate("other", &rows).is_err());
    assert!(proof.normalize("unobserved").is_err());
    let history = History::from_wire("{}").unwrap();
    let s = state(&rows);
    let model = Model::fallback();
    let invalid = RankInputs {
        state: &s,
        history: &history,
        model: &model,
        now_ms: f64::NAN,
    };
    assert!(matches!(
        rank(
            "review",
            &rows,
            &invalid,
            &proof,
            &mut SimilarityCache::default()
        ),
        Err(Error::InvalidClock)
    ));
}
#[test]
fn share_ranking_projects_facts_and_rejects_duplicate_identity() {
    let mut raw = member("U00000001", "review");
    raw["facts"] = json!({"member":false,"bestName":"forged"});
    let members = vec![raw];
    let q = local::prepare_query("review", str::to_owned);
    let m = match_members("review", &q, &members, &BTreeSet::new(), &facts(), |_| true);
    let rows = Rows::new(&m, &["review".into()], &[], &[]).unwrap();
    assert_eq!(
        rows.groups()["members"][0]["item"]["facts"]["bestName"],
        "review"
    );
    let dup = vec![m[0].clone(), m[0].clone()];
    assert!(Rows::new(&dup, &["review".into(), "review".into()], &[], &[]).is_err());
}
#[test]
fn share_ranking_write_coefficients_are_not_read_coefficients() {
    let members = vec![member("U00000001", "review")];
    let q = local::prepare_query("review", str::to_owned);
    let m = match_members("review", &q, &members, &BTreeSet::new(), &facts(), |_| true);
    let rows = Rows::new(&m, &["review".into()], &[], &[]).unwrap();
    let model = Model::from_weights(std::collections::BTreeMap::from([
        ("USER_IS_ACTIVE".into(), 2.0),
        ("READ_USER_IS_ACTIVE".into(), 1000.0),
        ("WRITE_USER_IS_ACTIVE".into(), 3.0),
    ]))
    .unwrap();
    let r = execute(&rows, "review", &state(&rows), &model, false).unwrap();
    assert_eq!(r.rows[0].model_score, 5.0);
}
#[test]
fn share_ranking_renderer_reply_rejects_stale_generation_and_incomplete_classes() {
    let rows = Rows::new(&[], &[], &[], &[]).unwrap();
    let p = ObservationPlan::capture("original", "review", rows.clone()).unwrap();
    let reply = RendererReply {
        generation: "new".into(),
        locale: "en-US".into(),
        collation_locale: "en-US".into(),
        lowered: p
            .normalization_fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        classes: vec![],
        demote_passthrough: false,
    };
    assert!(p.decode("en-US", reply).is_err());
    let members = vec![member("U00000001", "review")];
    let q = local::prepare_query("review", str::to_owned);
    let m = match_members("review", &q, &members, &BTreeSet::new(), &facts(), |_| true);
    let p = ObservationPlan::capture(
        "original",
        "review",
        Rows::new(&m, &["review".into()], &[], &[]).unwrap(),
    )
    .unwrap();
    let reply = RendererReply {
        generation: "original".into(),
        locale: "en-US".into(),
        collation_locale: "en-US".into(),
        lowered: p
            .normalization_fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        classes: vec![],
        demote_passthrough: false,
    };
    assert!(p.decode("en-US", reply).is_err());
}

#[test]
fn share_ranking_channel_matching_does_not_enable_previous_names() {
    let mut c = channel("C00000001", "unrelated");
    c["previous_names"] = json!(["review"]);
    let channels = vec![c];
    let q = local::prepare_query("review", str::to_owned);
    let matched = match_channels(
        "review",
        &q,
        &channels,
        &BTreeSet::new(),
        &facts(),
        |_| true,
        |v| v["name"].as_str().unwrap().into(),
        |_| panic!("Share does not query previous names"),
    );
    assert!(matched.is_empty());
}
#[test]
fn share_ranking_demote_observation_does_not_admit_missing_matches() {
    let members = vec![member("U00000001", "zulu")];
    let q = local::prepare_query("review", str::to_owned);
    let matched = match_members(
        "review",
        &q,
        &members,
        &BTreeSet::from(["U00000001".into()]),
        &facts(),
        |_| true,
    );
    assert!(matched.is_empty());
    let rows = Rows::new(&matched, &[], &[], &[]).unwrap();
    assert!(
        execute(&rows, "review", &state(&rows), &Model::fallback(), true)
            .unwrap()
            .rows
            .is_empty()
    );
}
