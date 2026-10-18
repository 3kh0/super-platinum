use super::*;
use serde_json::json;

#[test]
fn actual_retained_fuzzy_oracle_vectors() {
    let vectors: Vec<Value> = serde_json::from_str(include_str!("fuzzy-cases.json")).unwrap();
    assert_eq!(vectors.len(), 550);
    for v in vectors {
        let options = FuzzyOptions {
            limit: v["limit"].as_u64().unwrap() as u32,
            jump_limit: v["jump_limit"].as_u64().unwrap() as usize,
            cjk: v["cjk"].as_bool().unwrap(),
            forward_only: v["forward_only"].as_bool().unwrap(),
            substrings_only: v["substrings_only"].as_bool().unwrap(),
            can_match_beyond_word_boundaries: v["beyond"].as_bool().unwrap(),
        };
        let actual = FuzzyMatcher::new(v["query"].as_str().unwrap(), options)
            .score(v["text"].as_str().unwrap());
        assert_eq!(actual, v["score"].as_u64().map(|n| n as u32), "{v}");
    }
}
#[test]
fn actual_normalization_vectors_preserve_member_whitespace_and_query_locale_seam() {
    let vectors: Vec<Value> =
        serde_json::from_str(include_str!("normalization-cases.json")).unwrap();
    for v in vectors {
        assert_eq!(
            normalize_lowered_query(v["lowered"].as_str().unwrap()),
            v["query"].as_str().unwrap()
        );
        let raw = json!({"id":"U1","profile":{"real_name":v["raw"]}});
        let derived = derive_member_names(&raw, |s| s.to_lowercase()).unwrap();
        // Member producer is explicitly non-locale lowercase; these vectors pin
        // actual retained source results. Query uses the oracle's locale output.
        assert_eq!(derived["_real_name_normalized_lc"], v["member"]);
        assert_eq!(derived["profile"], raw["profile"]);
    }
}
#[test]
fn removed_combining_marks_can_expose_space_for_second_normalization() {
    let query = prepare_query("\u{0301} A", |s| s.to_lowercase());
    assert_eq!(query.normalized, " a");
    assert_eq!(query.matcher_normalized, "a");
    let raw = json!({"id":"U1","real_name":"top ignored","profile":{"real_name":" A  B "}});
    let user = derive_member_names(&raw, |s| s.to_lowercase()).unwrap();
    assert_eq!(user["_real_name_normalized_lc"], " a  b ");
}
fn peers() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "U1".into(),
            json!({"id":"U1","_real_name_normalized_lc":"alice doe","deleted":true}),
        ),
        (
            "U2".into(),
            json!({"id":"U2","_real_name_normalized_lc":"zebra"}),
        ),
        ("U3".into(), json!({"id":"U3","isUnknown":true})),
        (
            "U4".into(),
            json!({"id":"U4","is_self":true,"_real_name_normalized_lc":"self"}),
        ),
    ])
}
fn ims() -> Vec<Value> {
    vec![
        json!({"id":"D1","is_im":true,"user":"U1","unknown":7}),
        json!({"id":"D2","is_im":true,"user":"U2"}),
        json!({"id":"D3","is_im":true,"user":"U3"}),
        json!({"id":"D4","is_im":true,"user":"U4"}),
        json!({"id":"D5","is_im":true,"user":"missing"}),
    ]
}
#[test]
fn dm_scoring_resolves_actual_peer_and_preserves_original_d_id_and_item() {
    let ims = ims();
    let peers = peers();
    let options = Options::workflow("ad", "ad", "ad", false);
    let out = filter_ims(&ims, &peers, &options, &BTreeSet::new(), |_| true);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, "D1");
    assert!(std::ptr::eq(out[0].item, &ims[0]));
    assert_eq!(out[0].score, 1);
    // A deleted peer still follows the supplied IM predicate, not ordinary
    // member eligibility. hHTo rejects only unknown/nonexistent peers here.
    assert_eq!(out[0].item["unknown"], 7);
}
#[test]
fn remote_passthrough_is_keyed_by_peer_member_and_retains_d_identity() {
    let ims = ims();
    let peers = peers();
    let mut options = Options::workflow("unrelated", "unrelated", "unrelated", false);
    options.passthrough_remote_fuzzy_filtering = true;
    let members = BTreeSet::from(["U2".into()]);
    let out = filter_ims(&ims, &peers, &options, &members, |_| true);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, "D2");
    assert_eq!(out[0].score, 2);
    assert!(out[0].is_flannel_fuzzy_passthrough);
    assert!(
        filter_ims(
            &ims,
            &peers,
            &options,
            &BTreeSet::from(["D2".into()]),
            |_| true
        )
        .is_empty()
    );
}
#[test]
fn exact_raw_d_fallback_is_case_sensitive_and_cannot_bypass_peer_or_filter() {
    let ims = ims();
    let peers = peers();
    let o = Options::workflow("D2", "d2", "d2", false);
    let out = filter_ims(&ims, &peers, &o, &BTreeSet::new(), |_| true);
    assert_eq!(out[0].id, "D2");
    assert_eq!(out[0].score, 0);
    assert!(out[0].score_details.is_none());
    assert!(filter_ims(&ims, &peers, &o, &BTreeSet::new(), |_| false).is_empty());
    for (raw, normalized) in [("d2", "d2"), ("D3", "d3"), ("D5", "d5")] {
        let o = Options::workflow(raw, normalized, normalized, false);
        assert!(filter_ims(&ims, &peers, &o, &BTreeSet::new(), |_| true).is_empty());
    }
}
#[test]
fn empty_query_and_self_exact_metadata_paths_match_source() {
    let ims = ims();
    let peers = peers();
    let mut o = Options::workflow("", "", "", false);
    o.allow_empty_query = true;
    o.self_first = true;
    let out = filter_ims(&ims, &peers, &o, &BTreeSet::new(), |_| true);
    assert_eq!(
        out.iter().map(|x| x.id).collect::<Vec<_>>(),
        vec!["D1", "D2", "D4"]
    );
    assert!(out.iter().all(|x| !x.is_self && x.score_details.is_none()));
    let members: Vec<_> = peers.values().cloned().collect();
    assert!(
        filter_members(&members, &o, &BTreeSet::new(), |_| true)
            .iter()
            .find(|x| x.id == "U4")
            .unwrap()
            .is_self
    );
    let o = Options::workflow("me", "me", "me", true);
    assert_eq!(
        filter_ims(&ims, &peers, &o, &BTreeSet::new(), |_| true)[0].id,
        "D4"
    );
    let mut o = Options::workflow("alice doe", "alice doe", "alice doe", false);
    o.avoid_exact_match_for_members = false;
    let out = filter_ims(&ims, &peers, &o, &BTreeSet::new(), |_| true);
    assert_eq!(out[0].is_exact_match, Some(true));
    assert_eq!(
        out[0].exact_match_details.as_ref().unwrap()["real_name"],
        true
    );
}
#[test]
fn display_name_email_and_server_member_passthrough_follow_explicit_options() {
    let members = vec![
        json!({"id":"U1","_display_name_normalized_lc":"alice","_email_normalized_lc":"user@example.test"}),
    ];
    let o = Options::workflow("alice", "alice", "alice", false);
    assert_eq!(
        filter_members(&members, &o, &BTreeSet::new(), |_| true)[0].score,
        0
    );
    let mut o = Options::workflow("user", "user", "user", false);
    assert!(filter_members(&members, &o, &BTreeSet::new(), |_| true).is_empty());
    o.match_email = true;
    assert_eq!(
        filter_members(&members, &o, &BTreeSet::new(), |_| true)[0]
            .score_details
            .as_ref()
            .unwrap()["email"],
        0
    );
    let mut o = Options::workflow("absent", "absent", "absent", false);
    o.passthrough_remote_fuzzy_filtering = true;
    let remote = BTreeSet::from(["U1".into()]);
    assert_eq!(filter_members(&members, &o, &remote, |_| true)[0].score, 2);
    assert!(filter_members(&members, &o, &remote, |_| false).is_empty());
}
#[test]
fn no_raw_profile_fallback_before_actual_derived_name_acquisition() {
    let members = vec![json!({"id":"U1","profile":{"real_name":"Alice"}})];
    let o = Options::workflow("alice", "alice", "alice", false);
    assert!(filter_members(&members, &o, &BTreeSet::new(), |_| true).is_empty());
    let acquired = vec![derive_member_names(&members[0], |s| s.to_lowercase()).unwrap()];
    assert_eq!(
        filter_members(&acquired, &o, &BTreeSet::new(), |_| true)[0].id,
        "U1"
    );
}
