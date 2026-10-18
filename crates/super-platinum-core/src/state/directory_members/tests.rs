use super::*;
use crate::slack::models::User;
use serde_json::json;
use std::collections::HashMap;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST1234".into(),
        user_id: "USELF1234".into(),
        enterprise_id: None,
        name: "Test".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    }
}
fn flags() -> Flags {
    Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap()
}
fn row(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"profile":{"display_name":name}})
}
fn receipt(p: &Pending) -> RendererReply {
    RendererReply {
        generation: p.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: p.fields().to_vec(),
        lowered: p.fields().iter().map(|s| s.to_lowercase()).collect(),
    }
}
fn settle(s: &mut Store, native: &mut HashMap<String, User>) {
    let p = s.prepare("A", &owner(), 7, "Q", flags(), native).unwrap();
    let response = receipt(&p);
    let commit = s
        .adopt(p, response, &flags(), "A", &owner(), 7, "Q", native)
        .unwrap();
    let (_, staged) = commit.into_parts();
    *native = staged;
}
fn seeded() -> (Store, HashMap<String, User>) {
    let mut s = Store::default();
    let mut native = HashMap::new();
    s.begin("A", &owner(), &BTreeSet::new()).unwrap();
    s.enqueue(
        "A",
        &owner(),
        Origin::Directory,
        vec![row("UZLAST123", "Z"), row("UAFIRST12", "A")],
        json!({"UZLAST123":true,"UAFIRST12":true}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    (s, native)
}
#[test]
fn observed_order_updates_duplicates_and_tombstones_never_sort() {
    let (mut s, mut native) = seeded();
    s.enqueue(
        "A",
        &owner(),
        Origin::Directory,
        vec![
            row("UNEW12345", "first"),
            row("UZLAST123", "newZ"),
            row("UNEW12345", "last"),
        ],
        json!({"UNEW12345":true,"UZLAST123":true}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    let ready = s.ready("A", &owner(), 7, "Q", &native).unwrap();
    assert_eq!(
        ready
            .rows()
            .iter()
            .map(|r| r.source["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["UZLAST123", "UAFIRST12", "UNEW12345"]
    );
    let mut tombstone = row("UZLAST123", "newZ");
    tombstone["deleted"] = json!(true);
    s.enqueue(
        "A",
        &owner(),
        Origin::DirectUpsert,
        vec![tombstone],
        json!({}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    assert_eq!(
        s.ready("A", &owner(), 7, "Q", &native).unwrap().rows()[0].source["id"],
        "UZLAST123"
    );
    assert!(!ready.current(&s, "A", &owner(), 7, "Q", &native));
}
#[test]
fn physical_removal_then_reinsert_appends_but_omission_preserves() {
    let (mut s, mut native) = seeded();
    let id = "UZLAST123";
    s.remove("A", &owner(), &BTreeSet::from([id.into()]))
        .unwrap();
    native.remove(id);
    s.enqueue(
        "A",
        &owner(),
        Origin::DirectUpsert,
        vec![row(id, "again")],
        json!({}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    assert_eq!(
        s.ready("A", &owner(), 7, "Q", &native).unwrap().rows()[1].source["id"],
        id
    );
}
#[test]
fn typed_changes_with_same_ids_cannot_reuse_source_snapshot() {
    let (s, mut native) = seeded();
    let ready = s.ready("A", &owner(), 7, "Q", &native).unwrap();
    native.get_mut("UZLAST123").unwrap().name = Some("changed".into());
    assert!(!ready.current(&s, "A", &owner(), 7, "Q", &native));
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_err());
    assert_eq!(
        s.required_observations(&native),
        BTreeSet::from(["UZLAST123".into()])
    );
}
#[test]
fn pending_binds_native_prior_credentials_epoch_and_store_branch() {
    for reason in [
        "token",
        "account",
        "epoch",
        "generation",
        "native",
        "queue",
        "clone",
    ] {
        let (mut s, mut native) = seeded();
        s.enqueue(
            "A",
            &owner(),
            Origin::Directory,
            vec![row("UZLAST123", "new")],
            json!({}),
        )
        .unwrap();
        let p = s.prepare("A", &owner(), 7, "Q", flags(), &native).unwrap();
        let response = receipt(&p);
        let mut session = owner();
        if reason == "token" {
            session.token = "other".into();
        }
        if reason == "native" {
            native.get_mut("UZLAST123").unwrap().deleted = true;
        }
        if reason == "queue" {
            s.enqueue("A", &owner(), Origin::Directory, vec![], json!({}))
                .unwrap();
        }
        if reason == "clone" {
            let mut branch = s.clone();
            branch.invalidate("UAFIRST12");
            s.invalidate("UZLAST123");
            assert!(!branch.current(&p.stamp, "A", &owner(), 7, "Q"));
        }
        let before = s.cached();
        assert!(
            s.adopt(
                p,
                response,
                &flags(),
                if reason == "account" { "B" } else { "A" },
                &session,
                if reason == "epoch" { 8 } else { 7 },
                if reason == "generation" { "R" } else { "Q" },
                &native
            )
            .is_err()
        );
        assert_eq!(s.cached(), before);
    }
}
#[test]
fn overflow_retains_accepted_order_and_requires_explicit_reacquisition() {
    let (mut s, native) = seeded();
    for _ in 0..MAX_PENDING_BATCHES {
        s.enqueue("A", &owner(), Origin::Directory, vec![], json!({}))
            .unwrap();
    }
    let original = s.saved.as_ref().unwrap().records.clone();
    assert!(
        s.enqueue("A", &owner(), Origin::Directory, vec![], json!({}))
            .is_err()
    );
    assert!(s.saved.as_ref().unwrap().records == original);
    assert_eq!(s.saved.as_ref().unwrap().pending.len(), MAX_PENDING_BATCHES);
    assert!(s.prepare("A", &owner(), 7, "Q", flags(), &native).is_err());
    s.begin("A", &owner(), &native.keys().cloned().collect())
        .unwrap();
    assert_eq!(
        s.saved.as_ref().unwrap().provenance,
        Provenance::ReacquiredNativeHistory
    );
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_err());
}
#[test]
fn warm_cache_retains_exact_order_and_rejects_other_account_identity() {
    let (s, native) = seeded();
    let payload = s.cached().unwrap();
    let restored = Store::restore(Some(&payload), &owner());
    assert_eq!(restored.cached(), Some(payload.clone()));
    assert_eq!(
        restored
            .ready("A", &owner(), 9, "new", &native)
            .unwrap()
            .rows()[0]
            .source["id"],
        "UZLAST123"
    );
    assert!(restored.ready("B", &owner(), 9, "new", &native).is_err());
    let mut other = owner();
    other.user_id = "UOTHER123".into();
    assert!(Store::restore(Some(&payload), &other).saved.is_none());
    other = owner();
    other.team_id = "TOTHER123".into();
    assert!(Store::restore(Some(&payload), &other).saved.is_none());
    assert!(!payload.to_string().contains("synthetic"));
}
#[test]
fn legacy_and_malformed_cache_are_unknown_not_empty_source_inventory() {
    for raw in [
        None,
        Some(json!({})),
        Some(json!(null)),
        Some(json!({"version":99})),
    ] {
        let s = Store::restore(raw.as_ref(), &owner());
        assert!(s.ready("A", &owner(), 7, "Q", &HashMap::new()).is_err());
    }
    let (s, _) = seeded();
    let mut bad = s.cached().unwrap();
    bad["records"][1] = bad["records"][0].clone();
    assert!(Store::restore(Some(&bad), &owner()).saved.is_none());
}
#[test]
fn partial_boot_does_not_certify_legacy_full_native_membership() {
    let mut s = Store::default();
    let mut native = HashMap::new();
    let old: User = serde_json::from_value(row("UOLD12345", "old")).unwrap();
    native.insert(old.id.clone(), old);
    s.observe_boot(
        "A",
        &owner(),
        &native.keys().cloned().collect(),
        &json!({"ok":true,"users":[row("UNEW12345","new")]}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_err());
    assert!(s.invalidated().contains("UOLD12345"));
    s.enqueue(
        "A",
        &owner(),
        Origin::Directory,
        vec![row("UOLD12345", "fresh")],
        json!({"UOLD12345":true}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    let ready = s.ready("A", &owner(), 7, "Q", &native).unwrap();
    assert_eq!(ready.provenance(), Provenance::ReacquiredNativeHistory);
    assert_eq!(ready.rows()[0].source["id"], "UNEW12345");
}
#[test]
fn typed_invalidation_is_row_scoped_and_raw_reacquisition_settles_it() {
    let (mut s, mut native) = seeded();
    s.invalidate("UZLAST123");
    assert_eq!(s.invalidated(), BTreeSet::from(["UZLAST123".into()]));
    s.enqueue(
        "A",
        &owner(),
        Origin::Directory,
        vec![row("UZLAST123", "fresh")],
        json!({}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_ok());
}

#[test]
fn commit_stages_exact_checked_native_changes_and_preserves_other_fields() {
    let (mut s, mut native) = seeded();
    native
        .get_mut("UZLAST123")
        .unwrap()
        .extra
        .insert("native_only".into(), json!(7));
    s.enqueue(
        "A",
        &owner(),
        Origin::Directory,
        vec![
            row("UZLAST123", "Updated"),
            json!({"id":"UZLAST123","profile":{"title":"Retained Name"}}),
        ],
        json!({}),
    )
    .unwrap();
    let pending = s.prepare("A", &owner(), 7, "Q", flags(), &native).unwrap();
    let response = receipt(&pending);
    let (applied, staged) = s
        .adopt(pending, response, &flags(), "A", &owner(), 7, "Q", &native)
        .unwrap()
        .into_parts();
    assert_eq!(native["UZLAST123"].name.as_deref(), Some("Z"));
    assert_eq!(staged["UZLAST123"].name.as_deref(), Some("Updated"));
    assert_eq!(staged["UZLAST123"].extra["native_only"], 7);
    assert!(applied.rows()[0].source["profile"]["display_name"].is_null());
    assert_eq!(
        staged["UZLAST123"]
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Updated")
    );
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_err());
    assert!(s.ready("A", &owner(), 7, "Q", &staged).is_ok());
}

#[test]
fn invalid_native_patch_never_consumes_pending_or_changes_store() {
    let (mut s, native) = seeded();
    let mut wire = row("UZLAST123", "new");
    wire["is_bot"] = json!("malformed");
    s.enqueue("A", &owner(), Origin::Directory, vec![wire], json!({}))
        .unwrap();
    let pending = s.prepare("A", &owner(), 7, "Q", flags(), &native).unwrap();
    let response = receipt(&pending);
    let before = s.cached();
    assert!(
        s.adopt(pending, response, &flags(), "A", &owner(), 7, "Q", &native)
            .is_err()
    );
    assert_eq!(s.cached(), before);
    assert_eq!(native["UZLAST123"].name.as_deref(), Some("Z"));
}

#[test]
fn queue_state_distinguishes_empty_from_unknown_and_never_grants_ready() {
    let mut s = Store::default();
    assert!(s.has_pending("A", &owner()).is_err());
    s.begin("A", &owner(), &BTreeSet::new()).unwrap();
    assert!(!s.has_pending("A", &owner()).unwrap());
    assert!(s.has_pending("OTHER", &owner()).is_err());
    let mut other = owner();
    other.team_id = "TOTHER123".into();
    assert!(s.has_pending("A", &other).is_err());
    let mut native = HashMap::new();
    s.enqueue(
        "A",
        &owner(),
        Origin::Boot,
        vec![row("UNEW12345", "New")],
        json!({}),
    )
    .unwrap();
    assert!(s.has_pending("A", &owner()).unwrap());
    settle(&mut s, &mut native);
    assert!(!s.has_pending("A", &owner()).unwrap());
    s.invalidate("UNEW12345");
    assert!(!s.has_pending("A", &owner()).unwrap());
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_err());
    for _ in 0..MAX_PENDING_BATCHES {
        s.enqueue("A", &owner(), Origin::Boot, vec![], json!({}))
            .unwrap();
    }
    assert!(s.has_pending("A", &owner()).unwrap());
    assert!(
        s.enqueue("A", &owner(), Origin::Boot, vec![], json!({}))
            .is_err()
    );
    assert!(s.has_pending("A", &owner()).is_err());
}

#[test]
fn indexed_adoption_retains_last_unskipped_wire_and_untouched_witness() {
    let (mut s, mut native) = seeded();
    let mut prior = row("UZLAST123", "original");
    prior["updated"] = json!(10);
    s.enqueue("A", &owner(), Origin::Boot, vec![prior.clone()], json!({}))
        .unwrap();
    settle(&mut s, &mut native);
    let untouched = s.saved.as_ref().unwrap().records[1].clone();
    let mut first = row("UZLAST123", "first");
    first["updated"] = json!(11);
    let mut last = row("UZLAST123", "last");
    last["updated"] = json!(12);
    s.enqueue(
        "A",
        &owner(),
        Origin::Directory,
        vec![first, last.clone(), prior],
        json!({}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    let records = &s.saved.as_ref().unwrap().records;
    assert_eq!(records[0].source["name"], "last");
    assert_eq!(records[0].wire, last);
    assert!(records[1] == untouched);
    assert_eq!(native["UZLAST123"].name.as_deref(), Some("last"));
    let ready = s.ready("A", &owner(), 7, "Q", &native).unwrap();
    assert_eq!(
        ready
            .rows()
            .iter()
            .map(|row| row.source["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["UZLAST123", "UAFIRST12"]
    );
}

#[test]
fn member_info_ingress_preserves_projected_stranger_and_uses_scqy_skip() {
    let (mut s, mut native) = seeded();
    let mut wire = row("UZLAST123", "unavailable");
    wire["updated"] = json!(17);
    wire["is_stranger"] = json!(true);
    s.enqueue_member_info(
        "A",
        &owner(),
        vec![wire.clone()],
        json!({"UZLAST123":false}),
        vec!["UZLAST123".into()],
        vec![],
    )
    .unwrap();
    let restored_pending = Store::restore(s.cached().as_ref(), &owner());
    assert_eq!(restored_pending.cached(), s.cached());
    assert!(restored_pending.has_pending("A", &owner()).unwrap());
    s = restored_pending;
    let normalize=Flags::decode(&json!({"ok":true,"assignments":{"search_normalize_visible_external_users":{"group":"on"}}})).unwrap();
    let p = s
        .prepare("A", &owner(), 7, "Q", normalize.clone(), &native)
        .unwrap();
    let reply = receipt(&p);
    let (_, staged) = s
        .adopt(p, reply, &normalize, "A", &owner(), 7, "Q", &native)
        .unwrap()
        .into_parts();
    native = staged;
    let ready = s.ready("A", &owner(), 7, "Q", &native).unwrap();
    assert_eq!(ready.rows()[0].source["is_stranger"], true);
    assert_eq!(ready.rows()[0].interaction, Some(false));
    wire["name"] = json!("same version ignored");
    s.enqueue_member_info(
        "A",
        &owner(),
        vec![wire],
        json!({"UZLAST123":false}),
        vec!["UZLAST123".into()],
        vec![],
    )
    .unwrap();
    let p = s
        .prepare("A", &owner(), 7, "Q", normalize.clone(), &native)
        .unwrap();
    assert!(p.fields().is_empty());
    let reply = receipt(&p);
    let (_, staged) = s
        .adopt(p, reply, &normalize, "A", &owner(), 7, "Q", &native)
        .unwrap()
        .into_parts();
    native = staged;
    assert_eq!(
        s.ready("A", &owner(), 7, "Q", &native).unwrap().rows()[0].source["name"],
        "unavailable"
    );
    let restored = Store::restore(s.cached().as_ref(), &owner());
    assert!(restored.ready("A", &owner(), 7, "Q", &native).is_ok());
}

#[test]
fn partial_observations_keep_unknown_distinct_and_bind_source_scope() {
    let (mut s, native) = seeded();
    s.invalidate("UZLAST123");
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_err());
    let observed = s.observations("A", &owner(), 7, "Q").unwrap();
    assert_eq!(observed.interactions().get("UZLAST123"), Some(&Some(true)));
    assert!(!observed.interactions().contains_key("UUNKNOWN1"));
    assert!(!observed.disconnected().contains_key("UZLAST123"));
    assert!(observed.current(&s, "A", &owner(), 7, "Q"));
    assert!(!observed.current(&s, "OTHER", &owner(), 7, "Q"));
    assert!(!observed.current(&s, "A", &owner(), 8, "Q"));
    s.enqueue("A", &owner(), Origin::Boot, vec![], json!({}))
        .unwrap();
    assert!(!observed.current(&s, "A", &owner(), 7, "Q"));
    assert!(s.observations("A", &owner(), 7, "Q").is_err());
    let mut legacy = Store::default();
    legacy
        .begin("A", &owner(), &BTreeSet::from(["ULEGACY12".into()]))
        .unwrap();
    let observed = legacy.observations("A", &owner(), 7, "Q").unwrap();
    assert!(observed.interactions().is_empty());
    assert!(observed.disconnected().is_empty());
}

#[test]
fn retained_acquisition_rows_survive_native_drift_without_granting_subset_availability() {
    let (mut store, mut native) = seeded();
    let ids = BTreeSet::from(["UZLAST123".into()]);
    let original = store.subset("A", &owner(), 7, "Q", &ids, &native).unwrap();
    let raw = original.rows()[0].source.clone();
    native.get_mut("UZLAST123").unwrap().name = Some("typed-newer".into());
    assert!(store.subset("A", &owner(), 7, "Q", &ids, &native).is_err());
    assert!(!original.current(&store, "A", &owner(), 7, "Q", &native));
    let observed = store.observations("A", &owner(), 7, "Q").unwrap();
    let rows = observed.rows(&store, &["UZLAST123".into()]).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source, raw);
    assert_eq!(rows[0].interaction, Some(true));
    let reversed = observed
        .rows(&store, &["UAFIRST12".into(), "UZLAST123".into()])
        .unwrap();
    assert_eq!(reversed[0].source["id"], "UAFIRST12");
    assert_eq!(reversed[1].source["id"], "UZLAST123");
    assert!(observed.rows(&store, &["UUNKNOWN1".into()]).is_err());
    assert!(observed.rows(&store, &vec!["UZLAST123".into(); 2]).is_err());
    assert!(observed.rows(&store, &[]).is_err());
    assert!(
        observed
            .rows(
                &store,
                &(0..101).map(|n| format!("U{n:08}")).collect::<Vec<_>>()
            )
            .is_err()
    );
    assert!(observed.current(&store, "A", &owner(), 7, "Q"));
    assert!(!observed.current(&store, "B", &owner(), 7, "Q"));
    assert!(!observed.current(&store, "A", &owner(), 8, "Q"));
    assert!(!observed.current(&store, "A", &owner(), 7, "successor"));
    let mut successor = owner();
    successor.token = "replacement-synthetic".into();
    assert!(!observed.current(&store, "A", &successor, 7, "Q"));
    store.invalidate("UZLAST123");
    assert!(!observed.current(&store, "A", &owner(), 7, "Q"));
    assert!(observed.rows(&store, &["UZLAST123".into()]).is_err());
    let invalidated = store.observations("A", &owner(), 7, "Q").unwrap();
    assert_eq!(
        invalidated.rows(&store, &["UZLAST123".into()]).unwrap()[0].source,
        raw
    );
    assert!(store.subset("A", &owner(), 7, "Q", &ids, &native).is_err());
    store
        .enqueue("A", &owner(), Origin::Boot, vec![], json!({}))
        .unwrap();
    assert!(!invalidated.current(&store, "A", &owner(), 7, "Q"));
    assert!(invalidated.rows(&store, &["UZLAST123".into()]).is_err());
    assert!(store.observations("A", &owner(), 7, "Q").is_err());
}

#[test]
fn info_disconnection_effects_settle_atomically_and_survive_cache_without_removing_members() {
    let (mut s, mut native) = seeded();
    s.enqueue_member_info(
        "A",
        &owner(),
        vec![row("UZLAST123", "new")],
        json!({"UZLAST123":true}),
        vec!["UZLAST123".into(), "UAFIRST12".into()],
        vec!["UZLAST123".into()],
    )
    .unwrap();
    let saved = s.cached();
    s = Store::restore(saved.as_ref(), &owner());
    assert_eq!(s.cached(), saved);
    settle(&mut s, &mut native);
    let observed = s.observations("A", &owner(), 7, "Q").unwrap();
    assert_eq!(observed.disconnected().get("UZLAST123"), Some(&true));
    assert_eq!(observed.disconnected().get("UAFIRST12"), Some(&false));
    assert!(s.ready("A", &owner(), 7, "Q", &native).is_ok());
    assert_eq!(native.len(), 2);
    s.enqueue_member_info(
        "A",
        &owner(),
        vec![],
        json!({}),
        vec!["UZLAST123".into()],
        vec![],
    )
    .unwrap();
    settle(&mut s, &mut native);
    let restored = Store::restore(s.cached().as_ref(), &owner());
    assert_eq!(
        restored
            .observations("A", &owner(), 7, "Q")
            .unwrap()
            .disconnected()
            .get("UZLAST123"),
        Some(&false)
    );
    assert_eq!(
        restored
            .ready("A", &owner(), 7, "Q", &native)
            .unwrap()
            .rows()
            .len(),
        2
    );
}

#[test]
fn failed_native_stage_preserves_disconnection_effect_custody() {
    let (mut s, native) = seeded();
    let mut wire = row("UZLAST123", "bad");
    wire["is_bot"] = json!("invalid");
    s.enqueue_member_info(
        "A",
        &owner(),
        vec![wire],
        json!({}),
        vec!["UZLAST123".into()],
        vec!["UZLAST123".into()],
    )
    .unwrap();
    let p = s.prepare("A", &owner(), 7, "Q", flags(), &native).unwrap();
    let reply = receipt(&p);
    let before = s.cached();
    assert!(
        s.adopt(p, reply, &flags(), "A", &owner(), 7, "Q", &native)
            .is_err()
    );
    assert_eq!(s.cached(), before);
    assert!(s.saved.as_ref().unwrap().disconnected.is_empty());
    assert!(s.has_pending("A", &owner()).unwrap());
}

#[test]
fn legacy_metadata_absence_is_unknown_and_malformed_effects_never_restore() {
    let (s, _) = seeded();
    let mut old = s.cached().unwrap();
    old.as_object_mut().unwrap().remove("disconnected");
    let restored = Store::restore(Some(&old), &owner());
    assert!(
        restored
            .observations("A", &owner(), 7, "Q")
            .unwrap()
            .disconnected()
            .is_empty()
    );
    old["disconnected"] = json!({"bad-id":true});
    assert!(
        Store::restore(Some(&old), &owner())
            .observations("A", &owner(), 7, "Q")
            .is_err()
    );
    let mut s = restored;
    assert!(
        s.enqueue("A", &owner(), Origin::MemberInfo, vec![], json!({}))
            .is_err()
    );
    assert!(
        s.enqueue_member_info(
            "A",
            &owner(),
            vec![],
            json!({}),
            vec!["UZLAST123".into()],
            vec!["UAFIRST12".into()]
        )
        .is_err()
    );
}

#[test]
fn recovery_state_never_treats_wrong_identity_as_unknown_or_overflow() {
    let mut store = Store::default();
    assert_eq!(
        store.recovery_state("A", &owner()).unwrap(),
        RecoveryState::Unknown
    );
    assert!(store.recovery_state("", &owner()).is_err());
    store.begin("A", &owner(), &BTreeSet::new()).unwrap();
    assert_eq!(
        store.recovery_state("A", &owner()).unwrap(),
        RecoveryState::CurrentIdentity
    );
    for _ in 0..MAX_PENDING_BATCHES {
        store
            .enqueue("A", &owner(), Origin::Boot, vec![], json!({}))
            .unwrap();
    }
    assert!(
        store
            .enqueue("A", &owner(), Origin::Boot, vec![], json!({}))
            .is_err()
    );
    assert_eq!(
        store.recovery_state("A", &owner()).unwrap(),
        RecoveryState::Overflowed
    );
    assert!(store.recovery_state("B", &owner()).is_err());
    let mut other = owner();
    other.user_id = "UOTHER123".into();
    assert!(store.recovery_state("A", &other).is_err());
    assert_eq!(
        store.recovery_state("A", &owner()).unwrap(),
        RecoveryState::Overflowed
    );
}

#[test]
fn unknown_completion_uses_source_presence_and_never_native_membership() {
    use crate::block_actions::directory_endpoint::member_info::unknown::{
        Capture, NetworkTerminal, PlaceholderAsset,
    };
    let (s, _native) = seeded();
    let observed = s.observations("A", &owner(), 7, "Q").unwrap();
    assert_eq!(
        observed.source_ids().collect::<BTreeSet<_>>(),
        BTreeSet::from(["UZLAST123", "UAFIRST12"])
    );
    let captured =
        Capture::network_only(observed, &["UZLAST123".into(), "UNEW00001".into()]).unwrap();
    assert_eq!(captured.ids(), &["UNEW00001"]);
    let asset = PlaceholderAsset::observed("https://source-fixture.invalid/assets/").unwrap();
    assert!(
        captured
            .complete(
                &s,
                "A",
                &owner(),
                7,
                "Q",
                NetworkTerminal::Complete(vec![json!({"id":"UZLAST123","is_stranger":false})]),
                &asset
            )
            .is_err()
    );
    assert_eq!(
        s.ready("A", &owner(), 7, "Q", &_native).unwrap().rows()[0].source["name"],
        "Z"
    );
}
