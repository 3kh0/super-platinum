use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","created":1,"user":"U1","access":"write",
        "list_limits":{"over_column_maximum":false},"list_metadata":{
        "schema":[{"id":"C1","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true},
            {"id":"C2","key":"count","name":"Count","type":"number","options":{"precision":2},"opaque":{"keep":1}}],
        "views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"C1","key":"NAME","visible":true},{"id":"C2","key":"count","visible":true}],"filters":[],"sorts":[{"key":"count","ascending":true}]},
            {"id":"V2","name":"Other","type":"table","columns":[],"opaque":{"keep":3}}],
        "default_view":"V1","opaque":{"keep":2}}})).unwrap()
}
fn records() -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R1","fields":[{"key":"NAME","text":"Parent"},{"key":"count","number":[12.5]}],"opaque":{"keep":4}},
        {"id":"R2","parent_record_id":"R1","fields":[]}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(d: &Document, p: &RecordsPage) -> ColumnCreate {
    ColumnCreate::capture(d, p, "V1", " Notes ", "U1", Some("full"), Some(&ordinary())).unwrap()
}
fn added(d: &Document) -> Document {
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":"SERVER_NEW","key":"server_key","name":"Notes","type":"text","options":{"format":"text"},"opaque":{"server":42}}));
    raw["list_metadata"]["views"][0]["columns"].as_array_mut().unwrap().insert(1,json!({"id":"SERVER_NEW","key":"server_key","visible":true,"position":13,"opaque":{"width":99}}));
    // One optional reference in another existing view is allowed, not required.
    raw["list_metadata"]["views"][1]["columns"] =
        json!([{"id":"SERVER_NEW","key":"server_key","visible":false}]);
    Document::parse(raw).unwrap()
}
fn mutation(d: &Document, f: impl FnOnce(&mut Value)) -> Document {
    let mut raw = d.raw.clone();
    f(&mut raw);
    Document::parse(raw).unwrap()
}
#[test]
fn isolated_observed_identity_preserves_unknowns_order_records_cursor_and_last_capacity() {
    let d = document();
    let p = records();
    let i = capture(&d, &p);
    let fresh = mutation(&added(&d), |v| {
        v["list_limits"]["over_column_maximum"] = json!(true)
    });
    let column = i.matching_addition(&fresh, &p).unwrap();
    assert_eq!(
        (&column.id, &column.key),
        (&"SERVER_NEW".to_owned(), &"server_key".to_owned())
    );
    assert_eq!(column.raw["opaque"], json!({"server":42}));
    assert_eq!(i.original(), &d);
    assert_eq!(i.original_records(), &p);
    assert_eq!(p.next_cursor.as_deref(), Some("NEXT"));
    assert_eq!(fresh.columns[1].id, "SERVER_NEW"); // Fixture placement, not a server promise.
}
#[test]
fn membership_is_observed_not_fabricated_and_visibility_requires_real_projection() {
    use super::super::saved_views::{ProjectionContext, catalog, project};
    let d = document();
    let p = records();
    let i = capture(&d, &p);
    // The retained Number sort requires the source-backed singleton-array cell.
    // Unknown members on the new view reference remain opaque and preserved.
    let baseline_views = catalog(&d).unwrap();
    assert!(project(&d, &p, &baseline_views[0], &ProjectionContext::default()).is_ok());
    let observed = added(&d);
    assert_eq!(observed.views[0]["columns"][1]["position"], json!(13));
    assert_eq!(
        observed.views[0]["columns"][1]["opaque"],
        json!({"width":99})
    );
    // Core's isolated metadata observation is not permission to call the new
    // field visible when the actual retained population cannot be projected.
    let mut malformed_raw = p.raw.clone();
    malformed_raw["records"][0]["fields"][1]["number"] = json!(12.5);
    let malformed = RecordsPage::parse(malformed_raw).unwrap();
    let malformed_intent = capture(&d, &malformed);
    assert!(
        malformed_intent
            .matching_addition(&observed, &malformed)
            .is_some()
    );
    let observed_views = catalog(&observed).unwrap();
    assert_eq!(
        project(
            &observed,
            &malformed,
            &observed_views[0],
            &ProjectionContext::default()
        )
        .unwrap_err()
        .reason,
        "Saved-view field array is malformed."
    );
    for visible in [false, true] {
        let fresh = mutation(&added(&d), |v| {
            v["list_metadata"]["views"][0]["columns"][1]["visible"] = json!(visible)
        });
        let column = i.matching_addition(&fresh, &p).unwrap();
        let views = catalog(&fresh).unwrap();
        let projection = project(
            &fresh,
            &p,
            views.iter().find(|v| v.id == "V1").unwrap(),
            &ProjectionContext::default(),
        )
        .unwrap();
        assert_eq!(projection.column_ids.contains(&column.id), visible);
    }
    let no_refs = mutation(&added(&d), |v| {
        v["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        v["list_metadata"]["views"][1]["columns"] = json!([]);
    });
    assert!(i.matching_addition(&no_refs, &p).is_some());
    let views = catalog(&no_refs).unwrap();
    let projection = project(&no_refs, &p, &views[0], &ProjectionContext::default()).unwrap();
    assert!(!projection.column_ids.contains(&"SERVER_NEW".into()));
}
#[test]
fn owner_capacity_custom_table_and_fresh_policy_are_required() {
    let d = document();
    let p = records();
    let i = capture(&d, &p);
    for value in [Value::Null, json!(true), json!("false")] {
        let changed = mutation(&d, |v| v["list_limits"]["over_column_maximum"] = value);
        assert!(
            ColumnCreate::capture(
                &changed,
                &p,
                "V1",
                "Notes",
                "U1",
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
        assert!(
            i.revalidate(&changed, &p, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    let absent = mutation(&d, |v| {
        v["list_limits"]
            .as_object_mut()
            .unwrap()
            .remove("over_column_maximum");
    });
    assert!(!ColumnCreate::supports(&absent, "V1", "U1"));
    for user in ["", "U2"] {
        assert!(
            ColumnCreate::capture(&d, &p, "V1", "Notes", user, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    let owner = mutation(&d, |v| v["access"] = json!("owner"));
    assert!(
        ColumnCreate::capture(
            &owner,
            &p,
            "V1",
            "Notes",
            "U2",
            Some("full"),
            Some(&ordinary())
        )
        .is_ok()
    );
    for allow in [None, Some("readonly")] {
        assert!(
            i.revalidate(&d, &p, "U1", allow, Some(&ordinary()))
                .is_err()
        );
    }
    assert!(i.revalidate(&d, &p, "U1", Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F1\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        i.revalidate(&d, &p, "U1", Some("full"), Some(&personal))
            .is_err()
    );
    for (key, value) in [("is_locked", json!(true)), ("access", json!("read"))] {
        let changed = mutation(&d, |v| v[key] = value);
        assert!(
            i.revalidate(&changed, &p, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    let board = mutation(&d, |v| {
        v["list_metadata"]["views"][0]["type"] = json!("board")
    });
    assert!(!ColumnCreate::supports(&board, "V1", "U1"));
    let default = mutation(&d, |v| {
        v["list_metadata"]["views"][0]["is_all_items_view"] = json!(true)
    });
    assert!(!ColumnCreate::supports(&default, "V1", "U1"));
}
#[test]
fn public_typed_disagreement_names_stale_user_and_population_refuse() {
    let d = document();
    let p = records();
    let i = capture(&d, &p);
    let mut fake = d.clone();
    fake.columns[0].name = "Forged".into();
    assert!(
        ColumnCreate::capture(
            &fake,
            &p,
            "V1",
            "Notes",
            "U1",
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    let mut fake_page = p.clone();
    fake_page.next_cursor = None;
    assert!(
        ColumnCreate::capture(
            &d,
            &fake_page,
            "V1",
            "Notes",
            "U1",
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    assert!(i.matching_addition(&added(&d), &fake_page).is_none());
    for name in [
        "".to_owned(),
        " \t".into(),
        "bad\nname".into(),
        "x".repeat(257),
        "😀".repeat(129),
    ] {
        assert!(
            ColumnCreate::capture(&d, &p, "V1", &name, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    assert_eq!(i.desired_name(), "Notes");
    assert!(
        i.revalidate(&d, &p, "U2", Some("full"), Some(&ordinary()))
            .is_err()
    );
    for edit in [0, 1, 2] {
        let mut raw = p.raw.clone();
        match edit {
            0 => raw["records"][0]["fields"][1]["number"] = json!([13]),
            1 => raw["response_metadata"]["next_cursor"] = json!("OTHER"),
            _ => raw["records"][1]["parent_record_id"] = json!("OTHER"),
        };
        let changed = RecordsPage::parse(raw).unwrap();
        assert!(
            i.revalidate(&d, &changed, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert!(i.matching_addition(&added(&d), &changed).is_none());
    }
}
#[test]
fn old_schema_order_views_and_unknown_metadata_cannot_change() {
    let d = document();
    let p = records();
    let i = capture(&d, &p);
    let fresh = added(&d);
    for edit in 0..9 {
        let changed = mutation(&fresh, |v| match edit {
            0 => v["list_metadata"]["schema"][0]["name"] = json!("Changed"),
            1 => v["list_metadata"]["schema"]
                .as_array_mut()
                .unwrap()
                .swap(0, 2),
            2 => v["list_metadata"]["views"]
                .as_array_mut()
                .unwrap()
                .swap(0, 1),
            3 => v["list_metadata"]["views"][0]["sorts"] = json!([]),
            4 => v["list_metadata"]["views"][0]["filters"] = json!([{"unknown":true}]),
            5 => v["list_metadata"]["opaque"]["keep"] = json!(3),
            6 => v["list_metadata"]["schema"][2]["opaque"]["keep"] = json!(2),
            7 => v["list_metadata"]["views"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"V3","name":"Extra","type":"table"})),
            _ => v["list_metadata"]["views"][0]["columns"][0]["visible"] = json!(false),
        });
        assert!(i.matching_addition(&changed, &p).is_none(), "edit {edit}");
    }
    for key in ["user", "created"] {
        let changed = mutation(&fresh, |v| v[key] = json!("changed"));
        assert!(i.matching_addition(&changed, &p).is_none());
    }
}
#[test]
fn exact_new_field_shape_unique_identity_and_primary_semantics() {
    let d = document();
    let p = records();
    let i = capture(&d, &p);
    let fresh = added(&d);
    for (key, value) in [
        ("name", json!("Different")),
        ("type", json!("rich_text")),
        ("options", json!({"format":"text","extra":true})),
        ("is_primary_column", json!(true)),
        ("is_primary_column", Value::Null),
        ("key", json!("name")),
    ] {
        let changed = mutation(&fresh, |v| v["list_metadata"]["schema"][1][key] = value);
        assert!(i.matching_addition(&changed, &p).is_none());
    }
    let explicit_false = mutation(&fresh, |v| {
        v["list_metadata"]["schema"][1]["is_primary_column"] = json!(false)
    });
    assert!(i.matching_addition(&explicit_false, &p).is_some());
    for key in ["id", "key"] {
        let mut raw = fresh.raw.clone();
        raw["list_metadata"]["schema"][1][key] = raw["list_metadata"]["schema"][0][key].clone();
        assert!(Document::parse(raw).is_err());
    }
    let two = mutation(&fresh, |v| {
        v["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"ANOTHER","key":"another","name":"Notes","type":"text","options":{"format":"text"}}))
    });
    assert!(i.matching_addition(&two, &p).is_none());
    let no_primary = mutation(&d, |v| {
        v["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        v["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .remove(0);
    });
    assert!(
        capture(&no_primary, &p)
            .matching_addition(&added(&no_primary), &p)
            .is_none()
    );
}
#[test]
fn prior_orphan_string_and_property_keys_never_revive_as_a_new_field() {
    for marker in ["SERVER_NEW", "server_key"] {
        for property in [false, true] {
            let d = mutation(&document(), |v| {
                v["list_metadata"]["opaque"]["nested"] = if property {
                    json!({(marker):42})
                } else {
                    json!([{"value":marker}])
                }
            });
            let p = records();
            assert!(capture(&d, &p).matching_addition(&added(&d), &p).is_none());
            let d = document();
            let mut raw = records().raw;
            raw["records"][0]["opaque"] = if property {
                json!({(marker):42})
            } else {
                json!([{"value":marker}])
            };
            let p = RecordsPage::parse(raw).unwrap();
            assert!(capture(&d, &p).matching_addition(&added(&d), &p).is_none());
        }
        let d = mutation(&document(), |v| {
            v["list_metadata"]["views"][0]["columns"]
                .as_array_mut()
                .unwrap()
                .push(json!({"key":marker,"visible":true}))
        });
        let p = records();
        assert!(capture(&d, &p).matching_addition(&added(&d), &p).is_none());
    }
    assert!(!unreferenced(
        &json!(vec![Value::Null; 100_001]),
        "new",
        "key"
    ));
    assert!(!unreferenced(
        &json!("x".repeat(8 * 1024 * 1024 + 1)),
        "new",
        "key"
    ));
}
#[test]
fn only_one_exact_id_key_reference_per_existing_view_may_be_stripped() {
    let d = document();
    let p = records();
    let i = capture(&d, &p);
    let fresh = added(&d);
    for edit in 0..5 {
        let changed = mutation(&fresh, |v| match edit {
            0 => {
                v["list_metadata"]["views"][0]["columns"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("id");
            }
            1 => {
                v["list_metadata"]["views"][0]["columns"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("key");
            }
            2 => v["list_metadata"]["views"][0]["columns"][1]["id"] = json!("C2"),
            3 => {
                let entry = v["list_metadata"]["views"][0]["columns"][1].clone();
                v["list_metadata"]["views"][0]["columns"]
                    .as_array_mut()
                    .unwrap()
                    .push(entry);
            }
            _ => v["list_metadata"]["views"][0]["columns"][1]["key"] = json!("count"),
        });
        assert!(i.matching_addition(&changed, &p).is_none(), "edit {edit}");
    }
    let absent = mutation(&d, |v| {
        v["list_metadata"]["views"][1]
            .as_object_mut()
            .unwrap()
            .remove("columns");
    });
    let mut raw = added(&d).raw;
    raw["list_metadata"]["views"][1]["columns"] = json!([{"id":"SERVER_NEW","key":"server_key"}]);
    assert!(
        capture(&absent, &p)
            .matching_addition(&Document::parse(raw).unwrap(), &p)
            .is_none()
    );
}
#[test]
fn failed_admission_does_not_consume_but_every_admitted_outcome_is_nonreplayable() {
    let d = document();
    let p = records();
    for not_requested in [false, true] {
        let mut a = ColumnCreateAttempt::new(capture(&d, &p));
        assert!(!a.finish(false));
        assert!(
            a.admit(&d, &p, "U2", Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert_eq!(a.phase(), ColumnCreatePhase::Reviewing);
        let _permit = a
            .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
            .unwrap();
        assert_eq!(a.phase(), ColumnCreatePhase::InFlight);
        assert!(
            a.admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert!(a.finish(not_requested));
        assert!(!a.finish(not_requested));
        assert_eq!(
            a.phase(),
            if not_requested {
                ColumnCreatePhase::NotRequested
            } else {
                ColumnCreatePhase::Unconfirmed
            }
        );
        assert!(
            a.admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert!(a.intent().matching_addition(&added(&d), &p).is_some());
        // Observation never silently promotes phase or rearms the write.
        assert_ne!(a.phase(), ColumnCreatePhase::Reviewing);
    }
}
