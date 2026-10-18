use super::*;
use serde_json::json;

#[test]
fn explicit_unchanged_action_refreshes_alias_without_replacing_canonical_rows() {
    let (mut store, mut native) = seeded();
    initialize(&mut store, &native);
    (store, native) = store
        .remove("A", &owner(), &BTreeSet::from(["DLOW".into()]), &native)
        .unwrap()
        .into_parts();
    let missing_alias = initialize(&mut store, &native);
    assert!(missing_alias.for_member("UONE").unwrap().is_none());
    let before = store.cached().unwrap();
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    let (commit, missing) = store
        .process_unchanged_channels(
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &["DWIN".into(), "CUNKNOWN".into()],
            &[],
        )
        .unwrap()
        .into_parts();
    assert_eq!(missing, vec!["CUNKNOWN"]);
    (store, native) = commit.into_parts();
    assert_eq!(store.cached().unwrap(), before);
    assert!(!missing_alias.current(&store, "A", &owner(), 1, "Q", &native));
    assert_eq!(
        initialize(&mut store, &native)
            .for_member("UONE")
            .unwrap()
            .unwrap()["id"],
        "DWIN"
    );
    // The original response observation cannot be replayed after its own action.
    assert!(
        store
            .process_unchanged_channels(
                &observed,
                "A",
                &owner(),
                1,
                "Q",
                &native,
                &["DWIN".into()],
                &[]
            )
            .is_err()
    );
}

#[test]
fn unchanged_action_never_recertifies_native_drift_or_notifies_returned_rows() {
    let (mut store, mut native) = seeded();
    initialize(&mut store, &native);
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    let (commit, missing) = store
        .process_unchanged_channels(
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &["DWIN".into()],
            &["DWIN".into()],
        )
        .unwrap()
        .into_parts();
    assert!(missing.is_empty());
    (store, native) = commit.into_parts();
    assert!(observed.current(&store, "A", &owner(), 1, "Q", &native));
    native.get_mut("DWIN").unwrap().name = Some("drift".into());
    let drift = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    assert!(
        store
            .process_unchanged_channels(
                &drift,
                "A",
                &owner(),
                1,
                "Q",
                &native,
                &["DWIN".into()],
                &[]
            )
            .is_err()
    );
}

fn source_map(index: &Index) -> Value {
    let mut reverse = serde_json::Map::new();
    for (id, keys) in &index.reverse {
        let mut object = serde_json::Map::new();
        for (kind, key) in keys {
            let kind = match kind {
                Kind::Im => "ims",
                Kind::Channel => "channelsAndGroups",
                Kind::Mpim => "mpims",
            };
            object
                .entry(kind)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(key.clone().into());
        }
        reverse.insert(id.clone(), Value::Object(object));
    }
    json!({"ims":index.maps.get(&Kind::Im).cloned().unwrap_or_default(),
        "channelsAndGroups":index.maps.get(&Kind::Channel).cloned().unwrap_or_default(),
        "mpims":index.maps.get(&Kind::Mpim).cloned().unwrap_or_default(),"reverseMap":reverse})
}
#[test]
fn actual_emxr_alias_update_remove_and_member_lookup_vectors() {
    let cases: Value =
        serde_json::from_str(include_str!("../name-index-source-vectors.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 13);
    for case in cases.as_array().unwrap() {
        let mut index = Index::default();
        for trace in case["traces"].as_array().unwrap() {
            let step = &trace["step"];
            match step["kind"].as_str().unwrap() {
                "initial" | "update" if step["kind"] == "initial" || truth(&step["reason"]) => {
                    let lower = serde_json::from_value(trace["lowercase"].clone()).unwrap();
                    index
                        .update(step["rows"].as_array().unwrap(), &lower)
                        .unwrap();
                }
                "remove" => {
                    for id in step["ids"].as_array().unwrap() {
                        index.remove(id.as_str().unwrap());
                    }
                }
                _ => (),
            }
            assert_eq!(source_map(&index), trace["map"], "{}", case["label"]);
            for (id, wanted) in trace["memberLookups"].as_object().unwrap() {
                assert_eq!(
                    json!(index.member(id).unwrap()),
                    *wanted,
                    "{} {id}",
                    case["label"]
                );
            }
        }
    }
}
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "USELF".into(),
        enterprise_id: None,
        name: "fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn facts() -> Facts {
    Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::from([
            (
                "UONE".into(),
                json!({"id":"UONE","name":"first","team_id":"TONE"}),
            ),
            (
                "UTWO".into(),
                json!({"id":"UTWO","name":"uone","team_id":"TONE"}),
            ),
        ]),
    }
}
fn settle(
    store: &mut Store,
    native: &mut BTreeMap<String, Channel>,
    rows: Vec<Value>,
    facts: Facts,
) {
    store
        .enqueue("A", &owner(), 1, Origin::Direct, rows)
        .unwrap();
    let plan = store
        .prepare("A", &owner(), 1, "Q", native, facts.clone())
        .unwrap();
    assert!(plan.fields().iter().all(|s| s.is_ascii()));
    let reply = RendererReply {
        generation: plan.generation().into(),
        fields: plan.fields().to_vec(),
        values: plan
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
    };
    (*store, *native) = store
        .finish(plan, &facts, reply, "A", &owner(), 1, "Q", native)
        .unwrap()
        .into_parts();
}
fn seeded() -> (Store, BTreeMap<String, Channel>) {
    let mut store = Store::default();
    let mut native = BTreeMap::new();
    let lease = store.begin_full_boot("A", &owner(), 1).unwrap();
    assert!(store.finish_full_boot(lease, "A", &owner(), 1));
    store.begin("A", &owner(), &native).unwrap();
    settle(
        &mut store,
        &mut native,
        vec![
            json!({"id":"DLOW","name":"raw","is_im":true,"user":"UONE","updated":1}),
            json!({"id":"DWIN","name":"raw","is_im":true,"user":"UONE","updated":1}),
        ],
        facts(),
    );
    (store, native)
}
fn initialize(store: &mut Store, native: &BTreeMap<String, Channel>) -> NameSnapshot {
    let ready = store.ready("A", &owner(), 1, "Q", native).unwrap();
    if let Some(plan) = store
        .prepare_names(&ready, "A", &owner(), 1, "Q", native)
        .unwrap()
    {
        let reply = RendererReply {
            generation: plan.generation().into(),
            fields: plan.fields().to_vec(),
            values: plan
                .fields()
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect(),
        };
        store
            .finish_names(plan, reply, "A", &owner(), 1, "Q", native)
            .unwrap()
    } else {
        store.names_snapshot("A", &owner(), 1, "Q", native).unwrap()
    }
}
#[test]
fn lazy_index_survives_updates_and_source_removal_but_never_persists() {
    let (mut store, mut native) = seeded();
    assert!(store.names.is_none());
    let first = initialize(&mut store, &native);
    assert_eq!(first.for_member("UONE").unwrap().unwrap()["id"], "DWIN");
    let mut changed = facts();
    changed.members.get_mut("UONE").unwrap()["name"] = "renamed".into();
    settle(
        &mut store,
        &mut native,
        vec![json!({"id":"DLOW","updated":2})],
        changed,
    );
    assert!(!first.current(&store, "A", &owner(), 1, "Q", &native));
    assert_eq!(
        store.names.as_ref().unwrap().maps[&Kind::Im]["first"],
        "DWIN"
    );
    // DLOW's retained reverse peer key is deleted even though DWIN won it.
    (store, native) = store
        .remove("A", &owner(), &BTreeSet::from(["DLOW".into()]), &native)
        .unwrap()
        .into_parts();
    let next = initialize(&mut store, &native);
    assert!(next.for_member("UONE").unwrap().is_none());
    let saved = store.cached().unwrap();
    assert!(saved.get("names").is_none());
    let mut restored = Store::restore(Some(&saved), &owner());
    assert!(restored.names.is_none());
    // Reload mirrors source nonpersistence: rebuild from current actual order.
    let rebuilt = initialize(&mut restored, &native);
    assert_eq!(rebuilt.for_member("UONE").unwrap().unwrap()["id"], "DWIN");
}
#[test]
fn renderer_plan_rejects_native_drift_and_nonmember_lookup_shape() {
    let (mut store, mut native) = seeded();
    let ready = store.ready("A", &owner(), 1, "Q", &native).unwrap();
    let plan = store
        .prepare_names(&ready, "A", &owner(), 1, "Q", &native)
        .unwrap()
        .unwrap();
    let reply = RendererReply {
        generation: plan.generation().into(),
        fields: plan.fields().to_vec(),
        values: plan
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
    };
    native.get_mut("DLOW").unwrap().name = Some("drift".into());
    assert!(
        store
            .finish_names(plan, reply, "A", &owner(), 1, "Q", &native)
            .is_err()
    );
    assert!(Index::default().member("#name").is_err());
}

#[test]
fn eh_membership_action_republishes_unchanged_names_into_live_alias_index() {
    let (mut store, mut native) = seeded();
    // IMs normally start as members; retain an actual update before lazy init.
    settle(
        &mut store,
        &mut native,
        vec![json!({"id":"DWIN","is_member":false,"updated":2})],
        facts(),
    );
    initialize(&mut store, &native);
    (store, native) = store
        .remove("A", &owner(), &BTreeSet::from(["DLOW".into()]), &native)
        .unwrap()
        .into_parts();
    let absent = initialize(&mut store, &native);
    assert!(absent.for_member("UONE").unwrap().is_none());
    let ready = store.ready("A", &owner(), 1, "Q", &native).unwrap();
    assert_eq!(
        native["DWIN"].extra.get("is_member"),
        Some(&Value::Bool(false))
    );
    (store, native) = store
        .unchanged_memberships(
            &ready,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &[],
            &["DWIN".into()],
            true,
        )
        .unwrap()
        .into_parts();
    assert_eq!(
        native["DWIN"].extra.get("is_member"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        initialize(&mut store, &native)
            .for_member("UONE")
            .unwrap()
            .unwrap()["id"],
        "DWIN"
    );
}

#[test]
fn unchanged_receipt_does_not_resurrect_removed_peer_alias() {
    let (mut store, mut native) = seeded();
    initialize(&mut store, &native);
    (store, native) = store
        .remove("A", &owner(), &BTreeSet::from(["DLOW".into()]), &native)
        .unwrap()
        .into_parts();
    let before = initialize(&mut store, &native);
    assert!(before.for_member("UONE").unwrap().is_none());
    let row = store.ready("A", &owner(), 1, "Q", &native).unwrap().rows()[0].clone();
    settle(&mut store, &mut native, vec![row], facts());
    assert!(
        initialize(&mut store, &native)
            .for_member("UONE")
            .unwrap()
            .is_none()
    );
}

#[test]
fn actual_changed_actions_retain_nested_reference_and_duplicate_semantics() {
    let cases: Value =
        serde_json::from_str(include_str!("../name-action-source-vectors.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 11);
    let members = BTreeMap::new();
    for case in cases.as_array().unwrap() {
        let lower: BTreeMap<String, String> =
            serde_json::from_value(case["lowercase"].clone()).unwrap();
        let lowercase = |raw: &str| lower.get(raw).cloned();
        let context = projection::Context {
            self_id: "USELF",
            current_team: "TONE",
            omit_team: "TONE",
            boot_in_flight: false,
            retain_public_membership_on_archive: false,
            members: &members,
            lowercase: &lowercase,
        };
        let processed = projection::batch(
            case["before"].as_array().unwrap(),
            case["incoming"].as_array().unwrap(),
            &context,
        )
        .unwrap();
        assert_eq!(
            processed.updates(),
            case["updates"].as_array().unwrap(),
            "observed {}",
            case["name"]
        );
        assert_eq!(
            processed.actions(),
            case["actionUpdates"].as_array().unwrap(),
            "actions {}",
            case["name"]
        );
        assert_eq!(
            processed.rows(),
            case["rows"].as_array().unwrap(),
            "canonical {}",
            case["name"]
        );
    }
}
