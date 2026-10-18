use super::*;
use serde_json::json;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST1234".into(),
        enterprise_id: None,
        user_id: "USELF1234".into(),
        name: "Test".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    }
}
fn flags() -> Flags {
    Flags::decode(&json!({"ok":true,"assignments":{"search_normalize_visible_external_users":{"group":"on"},"preserve_omitted_user_change_member_fields":{"group":"on"}}})).unwrap()
}
fn current(id: &str) -> Current {
    let raw = json!({"id":id,"name":"Old","updated":1,"is_bot":true,"unknown":{"keep":1},"profile":{"real_name":"Old Real","display_name":"Old Display","email":"old@example.invalid","first_name":"OldFirst","last_name":"OldLast","fields":{"X":"keep"},"unknown_profile":3}});
    let source = derive_member_names(&raw, str::to_lowercase).unwrap();
    Current {
        native: raw,
        source,
        interaction: Some(true),
    }
}
fn plan(rows: Vec<Current>, incoming: Vec<Value>, f: Flags) -> Plan {
    let interactions: serde_json::Map<String, Value> = incoming
        .iter()
        .map(|row| (row["id"].as_str().unwrap().to_owned(), json!(true)))
        .collect();
    Plan::capture(
        Scope::capture("A", &owner(), 7, "Q").unwrap(),
        rows,
        incoming,
        IncomingInteractions::decode(&Value::Object(interactions)).unwrap(),
        f,
    )
    .unwrap()
}
fn reply(p: &Plan) -> RendererReply {
    RendererReply {
        generation: "Q".into(),
        operation: "ordinary-toLowerCase".into(),
        fields: p.fields.clone(),
        lowered: p.fields.iter().map(|s| s.to_lowercase()).collect(),
    }
}
fn finish(p: Plan, rows: &[Current], f: &Flags) -> Result<Applied, Error> {
    let receipt = reply(&p);
    p.finish("A", &owner(), 7, "Q", rows, f, receipt)
}
#[test]
fn native_absence_preservation_never_supplies_source_names() {
    let before = vec![current("UOTHER123")];
    let f = flags();
    let result = finish(
        plan(
            before.clone(),
            vec![json!({"id":"UOTHER123","updated":2,"profile":{"display_name":"New"}})],
            f.clone(),
        ),
        &before,
        &f,
    )
    .unwrap();
    let row = &result.rows()[0];
    assert_eq!(row.native["profile"]["real_name"], "Old Real");
    assert_eq!(row.native["unknown"]["keep"], 1);
    assert_eq!(row.native["profile"]["unknown_profile"], 3);
    assert!(row.source["profile"]["real_name"].is_null());
    assert_eq!(row.source["_real_name_normalized_lc"], "");
    assert_eq!(row.source["_display_name_normalized_lc"], "new");
    assert_eq!(row.source["profile"]["fields"], json!({"X":"keep"}));
    assert_eq!(row.source["profile"]["first_name"], "");
}
#[test]
fn self_email_flag_and_explicit_null_custom_fields_follow_actual_source() {
    let before = vec![current("USELF1234")];
    let f = flags();
    let wire = json!({"id":"USELF1234","profile":{"email":null,"fields":null}});
    let out = finish(
        plan(before.clone(), vec![wire.clone()], f.clone()),
        &before,
        &f,
    )
    .unwrap();
    assert_eq!(
        out.rows()[0].source["profile"]["email"],
        "old@example.invalid"
    );
    assert_eq!(
        out.rows()[0].native["profile"]["email"],
        "old@example.invalid"
    );
    let off = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let out = finish(plan(before.clone(), vec![wire], off.clone()), &before, &off).unwrap();
    assert!(out.rows()[0].source["profile"]["email"].is_null());
    let wire = json!({"id":"USELF1234","profile":{"email":"","fields":{}}});
    let out = finish(plan(before.clone(), vec![wire], f.clone()), &before, &f).unwrap();
    assert_eq!(out.rows()[0].source["profile"]["email"], "");
    assert_eq!(out.rows()[0].source["profile"]["fields"], json!({}));
}
#[test]
fn full_order_and_duplicate_returned_ids_preserve_batch_semantics() {
    let before = vec![current("UZLAST123"), current("UAFIRST12")];
    let f = flags();
    let incoming = vec![
        json!({"id":"UNEW12345","name":"First","extra":1,"profile":{"display_name":"First"}}),
        json!({"id":"UNEW12345","profile":{"display_name":"Final"}}),
    ];
    let out = finish(plan(before.clone(), incoming, f.clone()), &before, &f).unwrap();
    assert_eq!(
        out.rows()
            .iter()
            .map(|r| r.source["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["UZLAST123", "UAFIRST12", "UNEW12345"]
    );
    assert_eq!(out.rows()[0].source, before[0].source);
    assert_eq!(out.rows()[2].native["extra"], 1);
    assert_eq!(out.returned().len(), 2);
    assert_eq!(out.returned()[0], out.returned()[1]);
    assert_eq!(out.returned()[0]["_display_name_normalized_lc"], "final");
}
#[test]
fn same_version_needs_observed_interaction_and_returns_prior_projection() {
    let before = vec![current("UOTHER123")];
    let f = flags();
    let wire = json!({"id":"UOTHER123","updated":1,"profile":{"display_name":"Ignored"}});
    let out = finish(
        plan(before.clone(), vec![wire.clone()], f.clone()),
        &before,
        &f,
    )
    .unwrap();
    assert!(out.observations()[0].skipped_same_version);
    assert_eq!(out.rows()[0].source, before[0].source);
    let mut unknown = before;
    unknown[0].interaction = None;
    let out = finish(plan(unknown.clone(), vec![wire], f.clone()), &unknown, &f).unwrap();
    assert!(!out.observations()[0].skipped_same_version);
    assert_eq!(out.rows()[0].source["profile"]["display_name"], "Ignored");
}
#[test]
fn malformed_profile_only_flag_refuses_before_any_projection() {
    for v in [json!("yes"), json!(1), json!(null), json!({})] {
        let raw = json!({"id":"UOTHER123","profile":{},"is_profile_only_user":v});
        assert!(
            Plan::capture(
                Scope::capture("A", &owner(), 1, "Q").unwrap(),
                vec![],
                vec![raw],
                IncomingInteractions::decode(&json!({})).unwrap(),
                flags()
            )
            .is_err()
        );
    }
    let f = flags();
    let p = plan(
        vec![],
        vec![json!({"id":"UOTHER123","profile":{},"is_profile_only_user":true,"is_stranger":true})],
        f.clone(),
    );
    let out = finish(p, &[], &f).unwrap();
    assert_eq!(out.rows()[0].interaction, Some(false));
}
#[test]
fn all_names_use_complete_renderer_observation_without_locale_substitution() {
    let f = flags();
    let p = plan(
        vec![],
        vec![
            json!({"id":"UOTHER123","name":"USER","profile":{"first_name":"FIRST","last_name":"LAST","real_name":"ÉLODIE","display_name":"İSİM","email":"UPPER@EXAMPLE.INVALID"}}),
        ],
        f.clone(),
    );
    assert_eq!(
        p.fields(),
        [
            "USER",
            "FIRST",
            "LAST",
            "ÉLODIE",
            "İSİM",
            "UPPER@EXAMPLE.INVALID"
        ]
    );
    let out = finish(p, &[], &f).unwrap();
    assert_eq!(out.rows()[0].source["_first_name_lc"], "first");
    assert_eq!(out.rows()[0].source["_real_name_normalized_lc"], "elodie");
    assert_eq!(
        out.rows()[0].source["profile"]["real_name_normalized"],
        "ELODIE"
    );
}
#[test]
fn changed_owner_epoch_prior_or_flags_never_adopts_any_rows() {
    let before = vec![current("UOTHER123")];
    let f = flags();
    for reason in ["account", "token", "epoch", "generation", "prior", "flags"] {
        let p = plan(
            before.clone(),
            vec![json!({"id":"UOTHER123","profile":{}})],
            f.clone(),
        );
        let receipt = reply(&p);
        let mut w = owner();
        let mut rows = before.clone();
        let mut account = "A";
        let mut epoch = 7;
        let mut generation = "Q";
        let mut actual = f.clone();
        match reason {
            "account" => account = "B",
            "token" => w.token = "other".into(),
            "epoch" => epoch = 8,
            "generation" => generation = "R",
            "prior" => rows[0].native["unknown"] = json!("changed"),
            "flags" => actual = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            p.finish(account, &w, epoch, generation, &rows, &actual, receipt)
                .is_err(),
            "{reason}"
        );
    }
}
#[test]
fn partial_reordered_or_wrong_operation_renderer_receipt_is_refused() {
    let f = flags();
    for reason in ["missing", "order", "operation", "generation"] {
        let p = plan(
            vec![],
            vec![json!({"id":"UOTHER123","name":"A","profile":{"display_name":"B"}})],
            f.clone(),
        );
        let mut receipt = reply(&p);
        match reason {
            "missing" => {
                receipt.lowered.pop();
            }
            "order" => receipt.fields.reverse(),
            "operation" => receipt.operation = "toLocaleLowerCase".into(),
            "generation" => receipt.generation = "old".into(),
            _ => unreachable!(),
        };
        assert!(p.finish("A", &owner(), 7, "Q", &[], &f, receipt).is_err());
    }
}
#[test]
fn contradictory_experiment_envelope_is_not_observed_off() {
    for error in [json!(false), json!(""), json!({})] {
        assert!(Flags::decode(&json!({"ok":true,"error":error,"assignments":{}})).is_err());
    }
    assert!(Flags::decode(&json!({"ok":true})).is_err());
}
#[test]
fn actual_retained_profile_name_and_normalization_vectors() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let raw = case["before"].clone();
        let prior = Current {
            native: raw.clone(),
            source: derive_member_names(&raw, str::to_lowercase).unwrap(),
            interaction: None,
        };
        let before = vec![prior];
        let f=Flags::decode(&json!({"ok":true,"assignments":{
            "search_normalize_visible_external_users":{"group":if case["normalize"]==true{"on"}else{"off"}},
            "preserve_omitted_user_change_member_fields":{"group":if case["preserve"]==true{"on"}else{"off"}}
        }})).unwrap();
        let p = plan(before.clone(), vec![case["input"].clone()], f.clone());
        let fields: Vec<String> =
            serde_json::from_value(case["renderer"]["fields"].clone()).unwrap();
        assert_eq!(p.fields(), fields, "{}", case["label"]);
        let receipt = RendererReply {
            generation: "Q".into(),
            operation: "ordinary-toLowerCase".into(),
            fields,
            lowered: serde_json::from_value(case["renderer"]["lowered"].clone()).unwrap(),
        };
        let out = p
            .finish("A", &owner(), 7, "Q", &before, &f, receipt)
            .unwrap();
        assert_eq!(out.observations()[0].normalized_wire, case["normalized"]);
        assert_eq!(
            out.rows()[0].source["profile"],
            case["profile"],
            "{}",
            case["label"]
        );
        for (k, v) in case["derived"].as_object().unwrap() {
            assert_eq!(&out.rows()[0].source[k], v, "{} {k}", case["label"]);
        }
    }
}

#[test]
fn duplicate_omission_preserves_accumulated_explicit_interaction() {
    let f = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    for prior in [None, Some(false), Some(true)] {
        for stranger in [false, true] {
            let mut before = vec![];
            if let Some(value) = prior {
                let mut row = current("UDUP12345");
                row.interaction = Some(value);
                before.push(row);
            }
            let p = Plan::capture(
                Scope::capture("A", &owner(), 7, "Q").unwrap(),
                before.clone(),
                vec![
                    json!({"id":"UDUP12345","is_stranger":stranger,"profile":{}}),
                    json!({"id":"UDUP12345","profile":{}}),
                ],
                IncomingInteractions::decode(&json!({})).unwrap(),
                f.clone(),
            )
            .unwrap();
            let out = finish(p, &before, &f).unwrap();
            assert_eq!(out.rows()[0].interaction, Some(!stranger));
        }
    }
}
#[test]
fn incoming_map_is_observed_before_explicit_rows_but_after_version_partition() {
    let f = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    for prior in [None, Some(false), Some(true)] {
        for observed in [None, Some(false), Some(true)] {
            for explicit in [None, Some(false), Some(true)] {
                let mut old = current("UOTHER123");
                old.interaction = prior;
                let before = vec![old];
                let mut wire =
                    json!({"id":"UOTHER123","updated":1,"profile":{"display_name":"New"}});
                if let Some(flag) = explicit {
                    wire["is_stranger"] = json!(flag);
                }
                let map = observed.map_or_else(|| json!({}), |v| json!({"UOTHER123":v}));
                let p = Plan::capture(
                    Scope::capture("A", &owner(), 7, "Q").unwrap(),
                    before.clone(),
                    vec![wire],
                    IncomingInteractions::decode(&map).unwrap(),
                    f.clone(),
                )
                .unwrap();
                let out = finish(p, &before, &f).unwrap();
                let skip = observed.is_none_or(|v| prior == Some(v));
                assert_eq!(out.observations()[0].skipped_same_version, skip);
                let expected = if skip {
                    observed.or(prior)
                } else {
                    explicit.map(|v| !v).or(observed).or(prior)
                };
                assert_eq!(out.rows()[0].interaction, expected);
            }
        }
    }
    let p = Plan::capture(
        Scope::capture("A", &owner(), 7, "Q").unwrap(),
        vec![],
        vec![json!({"id":"UNEW12345","profile":{}})],
        IncomingInteractions::decode(&json!({"UNEW12345":true})).unwrap(),
        f.clone(),
    )
    .unwrap();
    assert_eq!(
        finish(p, &[], &f).unwrap().rows()[0].interaction,
        Some(true)
    );
}
#[test]
fn malformed_or_unowned_interaction_map_is_not_silently_discarded() {
    for raw in [
        json!(null),
        json!([]),
        json!({"UOTHER123":null}),
        json!({"UOTHER123":"true"}),
        json!({"invalid":true}),
    ] {
        assert!(IncomingInteractions::decode(&raw).is_err());
    }
    assert!(
        Plan::capture(
            Scope::capture("A", &owner(), 7, "Q").unwrap(),
            vec![],
            vec![],
            IncomingInteractions::decode(&json!({"UOTHER123":true})).unwrap(),
            flags()
        )
        .is_err()
    );
}

#[test]
fn direct_upsert_keeps_same_version_updates_and_observed_stranger() {
    let mut old = current("UOTHER123");
    old.interaction = Some(true);
    let before = vec![old];
    let f = flags();
    let p = Plan::capture_direct(Scope::capture("A", &owner(), 7, "Q").unwrap(),
        before.clone(), vec![json!({"id":"UOTHER123","updated":1,"is_stranger":true,"profile":{"display_name":"Direct"}})], f.clone()).unwrap();
    let out = finish(p, &before, &f).unwrap();
    assert!(!out.observations()[0].skipped_same_version);
    assert_eq!(
        out.rows()[0].source["_display_name_normalized_lc"],
        "direct"
    );
    assert_eq!(out.rows()[0].interaction, Some(false));
    assert_eq!(out.observations()[0].normalized_wire["is_stranger"], true);
}

#[test]
fn native_updates_use_actual_cache_before_without_supplying_source_names() {
    let f = flags();
    let p = plan(
        vec![],
        vec![
            json!({"id":"UNEW12345","profile":{"display_name":"first"}}),
            json!({"id":"UNEW12345","profile":{"title":"last"}}),
        ],
        f.clone(),
    );
    let out = finish(p, &[], &f).unwrap();
    let before = BTreeMap::from([(
        "UNEW12345".into(),
        json!({"id":"UNEW12345","native_extra":9,"profile":{"real_name":"native old"}}),
    )]);
    let updates = out.native_updates(&before).unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["native_extra"], 9);
    assert_eq!(updates[0]["profile"]["real_name"], "native old");
    assert_eq!(updates[0]["profile"]["display_name"], "first");
    assert!(out.rows()[0].source["profile"]["real_name"].is_null());
    assert!(out.rows()[0].source["profile"]["display_name"].is_null());
}
