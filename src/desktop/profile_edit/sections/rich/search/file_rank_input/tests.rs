use super::*;
use super_platinum_core::slack::models::Channel;

fn selectors() -> Value {
    json!({"offline":{"channel":{},"mpim":{}},"entityCtr":{},"ims":{},"remoteDm":{}})
}
#[test]
fn observed_cache_defaults_and_null_vs_missing_dm_are_preserved() {
    let mut ids = Ids::default();
    ids.add(&json!({"user":"UCREATOR01","channels":["CFILE01"],"groups":["GFILE01"]}))
        .unwrap();
    let offline = Offline::default();
    let mut input = selectors();
    augment(
        &mut input,
        &offline,
        &ids,
        &json!({"UCREATOR01":{"id":"DCREATOR01"}}),
    )
    .unwrap();
    assert_eq!(input["entityCtr"]["file"], offline.other_ctr());
    assert_eq!(
        input["offline"]["member"]["UCREATOR01"]["user_ctr_30dd"],
        offline.entity_ctr(Entity::Member)
    );
    assert_eq!(input["remoteDm"].get("UCREATOR01"), Some(&Value::Null));
    assert_eq!(input["ims"]["UCREATOR01"]["id"], "DCREATOR01");
    // A completed empty map has undefined lookup, not null. Use actual decoder.
    let observed = Offline::decode(&json!({"ok":true,"users_has_dmed":{}})).unwrap();
    let mut input = selectors();
    augment(&mut input, &observed, &ids, &json!({})).unwrap();
    assert!(input["remoteDm"].get("UCREATOR01").is_none());
    assert!(input["ims"].get("UCREATOR01").is_none());
}
#[test]
fn source_ids_do_not_create_typed_members_and_malformed_inputs_refuse() {
    let mut ids = Ids::default();
    for raw in [json!({}), json!({"user":null}), json!({"user":""})] {
        ids.add(&raw).unwrap();
    }
    assert!(ids.creators.is_empty());
    assert!(ids.add(&json!({"user":{"id":"UFAKE"}})).is_err());
    assert!(ids.add(&json!({"channels":[42]})).is_err());
    assert!(treatment(&json!({})).is_err());
    assert!(!treatment(&json!({"ok":true,"assignments":{}})).unwrap());
    assert!(!treatment(&json!({"ok":true,"assignments":{"ml_autocomplete_ignore_file_fuzzy_score":{"group":"on"}}})).unwrap());
    assert!(treatment(&json!({"ok":true,"assignments":{"ml_autocomplete_ignore_file_fuzzy_score":{"group":"treatment"}}})).unwrap());
    assert!(
        treatment(&json!({"ok":true,"assignments":{"ml_autocomplete_ignore_file_fuzzy_score":{}}}))
            .is_err()
    );
}
#[test]
fn creator_im_native_witness_detects_addition_mutation_and_ambiguous_duplicates() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.workspaces.keys().next().unwrap().clone();
    let creators = BTreeSet::from(["UCREATOR01".into()]);
    let absent = native(&shell, &team, &creators).unwrap();
    let dm = |id: &str| {
        serde_json::from_value::<Channel>(json!({"id":id,"is_im":true,"user":"UCREATOR01"}))
            .unwrap()
    };
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert("DFILE01".into(), dm("DFILE01"));
    let present = native(&shell, &team, &creators).unwrap();
    assert_ne!(absent, present);
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut("DFILE01")
        .unwrap()
        .last_read = Some("99.0".parse().unwrap());
    assert_ne!(present, native(&shell, &team, &creators).unwrap());
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert("DFILE02".into(), dm("DFILE02"));
    assert!(native(&shell, &team, &creators).is_err());
}
