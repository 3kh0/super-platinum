use super::*;
use serde_json::json;

fn fixture(view_patch: Value) -> (Document, RecordsPage, SavedView) {
    let mut view = json!({"id":"V1","name":"Saved table","type":"table","position":1,"columns":[
        {"key":"name","id":"C_NAME","visible":true},{"key":"done","id":"C_DONE","visible":true},
        {"key":"n","id":"C_NUMBER","visible":true},{"key":"status","id":"C_SELECT","visible":true}],"filters":[],"sorts":[]});
    view.as_object_mut()
        .unwrap()
        .extend(view_patch.as_object().unwrap().clone());
    let doc=Document::parse(json!({"id":"F1","mode":"list","list_metadata":{"schema":[
        {"id":"C_NAME","key":"name","name":"Name","type":"text"},
        {"id":"C_DONE","key":"done","name":"Done","type":"checkbox"},
        {"id":"C_NUMBER","key":"n","name":"Number","type":"number"},
        {"id":"C_SELECT","key":"status","name":"Status","type":"select","options":{"format":"multi_select","choices":[{"value":"z","label":"Zebra"},{"value":"a","label":"Alpha"}]}},
        {"id":"C_USER","key":"owner","name":"Owner","type":"user","options":{"format":"multi_entity"}},
        {"id":"C_DATE","key":"date","name":"Date","type":"date"}
    ],"views":[view]}})).unwrap();
    let page=RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R1","position":3,"date_created":1,"fields":[{"key":"name","text":"Bravo"},{"key":"done","checkbox":false},{"key":"n","number":[0]},{"key":"status","select":["z","a"]},{"key":"owner","user":["U1","U2"]}]},
        {"id":"R2","position":1,"date_created":2,"fields":[{"key":"name","text":"Alpha"},{"key":"done","checkbox":true},{"key":"n","number":[2]},{"key":"status","select":["a"]},{"key":"owner","user":["U2"]}]},
        {"id":"R3","position":2,"date_created":3,"fields":[{"key":"name","text":"Charlie"},{"key":"status","select":["removed"]}]}
    ],"response_metadata":{"next_cursor":"unloaded"}})).unwrap();
    let view = SavedView::parse(doc.views[0].clone()).unwrap();
    (doc, page, view)
}
fn ids(projection: &Projection) -> Vec<&str> {
    projection
        .groups
        .iter()
        .flat_map(|g| g.rows.iter().map(|r| r.record_id.as_str()))
        .collect()
}
fn run(patch: Value) -> Projection {
    let (doc, page, view) = fixture(patch);
    project(&doc, &page, &view, &ProjectionContext::default()).unwrap()
}

#[test]
fn exact_columns_keep_grouping_visible_but_never_append_omitted_schema() {
    let projection = run(
        json!({"columns":[{"key":"status","visible":false},{"key":"deleted","visible":true},{"key":"name","visible":true}],"grouping":{"group_by":"status","group_by_column_id":"C_SELECT"}}),
    );
    assert_eq!(projection.column_ids, vec!["C_SELECT", "C_NAME"]);
    assert!(projection.incomplete);
    let (doc, page, view) =
        fixture(json!({"columns":[{"key":"name","id":"REPLACED","visible":true}]}));
    assert!(project(&doc, &page, &view, &ProjectionContext::default()).is_err());
}
#[test]
fn checkbox_false_and_missing_are_empty_but_zero_is_not() {
    assert_eq!(
        ids(&run(
            json!({"filters":[{"key":"done","operator":"is_empty","values":[]}]})
        )),
        vec!["R3", "R1"]
    );
    assert_eq!(
        ids(&run(
            json!({"filters":[{"key":"n","operator":"is_not_empty","values":[]}]})
        )),
        vec!["R2", "R1"]
    );
    assert_eq!(
        ids(&run(
            json!({"filters":[{"key":"n","operator":"equal","values":[0]}]})
        )),
        vec!["R1"]
    );
    assert_eq!(
        ids(&run(
            json!({"filters":[{"key":"done","operator":"is_empty","values":[]}],"info_column_filters":[{"key":"n","operator":"equal","values":[0]}]})
        )),
        vec!["R1"]
    );
}
#[test]
fn typed_select_membership_option_sort_and_stable_ties() {
    assert_eq!(
        ids(&run(
            json!({"filters":[{"key":"status","operator":"contains","values":["a"]}]})
        )),
        vec!["R2", "R1"]
    );
    assert_eq!(
        ids(&run(json!({"sorts":[{"key":"status","ascending":true}]}))),
        vec!["R1", "R2", "R3"]
    );
    assert_eq!(
        ids(&run(json!({"sorts":[{"key":"status","ascending":false}]}))),
        vec!["R2", "R1", "R3"]
    );
    let (doc, mut page, view) = fixture(json!({"sorts":[{"key":"n","ascending":true}]}));
    page.records[0].fields.get_mut("n").unwrap()["number"] = json!([2]);
    assert_eq!(
        ids(&project(&doc, &page, &view, &ProjectionContext::default()).unwrap()),
        vec!["R2", "R1", "R3"]
    );
    let (doc, mut page, view) =
        fixture(json!({"sorts":[{"key":"done","ascending":true},{"key":"n","ascending":false}]}));
    page.records[0].raw["view_positions"] = json!({"v1":0.5});
    assert_eq!(
        ids(&project(&doc, &page, &view, &ProjectionContext::default()).unwrap()),
        vec!["R1", "R3", "R2"]
    );
}
#[test]
fn multigroup_instances_are_distinct_and_collapse_preserves_counts() {
    let (doc, page, view) = fixture(
        json!({"grouping":{"group_by":"status","group_by_column_id":"C_SELECT","order":[{"select":["a"]}]}}),
    );
    let mut context = ProjectionContext::default();
    let projection = project(&doc, &page, &view, &context).unwrap();
    assert_eq!(
        projection
            .groups
            .iter()
            .map(|g| g.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Zebra", "Alpha", "Ungrouped"]
    );
    assert_eq!(projection.unique_record_count, 3);
    assert_eq!(ids(&projection), vec!["R1", "R2", "R1", "R3"]);
    assert_ne!(
        projection.groups[0].rows[0].instance_id,
        projection.groups[1].rows[1].instance_id
    );
    context.collapsed_groups.insert("a".into());
    let collapsed = project(&doc, &page, &view, &context).unwrap();
    assert_eq!(collapsed.groups[1].count, 2);
    assert!(collapsed.groups[1].rows.is_empty());
    assert!(collapsed.groups[1].collapsed);
    let checkbox = run(json!({"grouping":{"group_by":"done","group_by_column_id":"C_DONE"}}));
    assert_eq!(checkbox.groups[0].label, "Checked");
    assert_eq!(checkbox.groups[1].count, 2);
}
#[test]
fn viewer_filter_requires_context_and_membership_honors_multi_entity() {
    let (doc, page, view) =
        fixture(json!({"filters":[{"key":"owner","operator":"contains","values":["viewer"]}]}));
    assert!(project(&doc, &page, &view, &ProjectionContext::default()).is_err());
    let context = ProjectionContext {
        viewer_id: Some("U2".into()),
        ..Default::default()
    };
    assert_eq!(
        ids(&project(&doc, &page, &view, &context).unwrap()),
        vec!["R2", "R1"]
    );
}
#[test]
fn unsupported_rules_never_return_an_unfiltered_saved_view() {
    for patch in [
        json!({"type":"future_layout"}),
        json!({"filters":[{"key":"date","operator":"equal","values":["today"]}]}),
        json!({"filters":[{"key":"n","operator":"future_operator","values":[1]}]}),
        json!({"sorts":[{"key":"removed","ascending":true}]}),
        json!({"filter_set":{"match":"any","groups":[]}}),
    ] {
        let (doc, page, view) = fixture(patch);
        assert!(project(&doc, &page, &view, &ProjectionContext::default()).is_err());
        let fallback = all_loaded(&doc, &page);
        assert_eq!(fallback.title, "All loaded items");
        assert_eq!(fallback.view_id, None);
        assert!(fallback.incomplete);
    }
    let (mut doc, page, view) = fixture(json!({}));
    doc.views.clear();
    assert!(project(&doc, &page, &view, &ProjectionContext::default()).is_err());
}
#[test]
fn rich_sort_is_bounded_and_uses_first_meaningful_token_not_display_fallback() {
    let (doc, mut page, view) = fixture(json!({"sorts":[{"key":"name","ascending":true}]}));
    page.records[0].fields.get_mut("name").unwrap()["rich_text"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"user","user_id":"U1"},{"type":"text","text":"later"}]}]}]);
    let context = ProjectionContext {
        user_display_names: BTreeMap::from([("U1".into(), "Aardvark".into())]),
        ..Default::default()
    };
    assert_eq!(
        ids(&project(&doc, &page, &view, &context).unwrap()),
        vec!["R1", "R2", "R3"]
    );
    page.records[0].fields.get_mut("name").unwrap()["rich_text"] =
        json!([{"type":"rich_text","elements":[{"type":"text","text":"x".repeat(65537)}]}]);
    assert!(project(&doc, &page, &view, &context).is_err());
    page.records[0].fields.get_mut("name").unwrap()["rich_text"] =
        json!([{"type":"future_opaque","secret_semantics":true}]);
    assert!(project(&doc, &page, &view, &context).is_err());
}
#[test]
fn refreshed_projection_excludes_filtered_or_parented_records_without_losing_canonical_owner() {
    let (doc, mut page, view) =
        fixture(json!({"filters":[{"key":"n","operator":"equal","values":[0]}]}));
    assert_eq!(
        ids(&project(&doc, &page, &view, &ProjectionContext::default()).unwrap()),
        vec!["R1"]
    );
    page.records[0].fields.get_mut("n").unwrap()["number"] = json!([1]);
    assert!(ids(&project(&doc, &page, &view, &ProjectionContext::default()).unwrap()).is_empty());
    assert!(page.record("R1").is_some());
    page.records[0].raw["parent_record_id"] = json!("R2");
    page.records[1].raw["suggested"] = json!(true);
    assert_eq!(ids(&all_loaded(&doc, &page)), vec!["R3"]);
}

#[test]
fn live_shaped_default_views_require_explicit_task_completion_policy() {
    for (key, show, expected) in [
        ("all_items", true, vec!["R2", "R3", "R1"]),
        ("incomplete_items", false, vec!["R3", "R1"]),
        ("completed_items", true, vec!["R2"]),
    ] {
        let (mut doc, page, view) =
            fixture(json!({"default_view_key":key,"show_completed_items":show}));
        doc.columns
            .iter_mut()
            .find(|c| c.key == "done")
            .unwrap()
            .kind = "todo_completed".into();
        let mut context = ProjectionContext {
            todo_mode: true,
            ..Default::default()
        };
        assert!(project(&doc, &page, &view, &context).is_err());
        context.apply_todo_completion_rules = Some(true);
        assert_eq!(
            ids(&project(&doc, &page, &view, &context).unwrap()),
            expected
        );
    }
}

#[test]
fn catalog_preserves_raw_default_metadata_and_sorts_positions_stably() {
    let (mut doc, _, _) = fixture(
        json!({"position":"2","default_view_key":"all_items","is_all_items_view":true,"is_template_initial_view":true,"unknown_future":{"retained":true}}),
    );
    let mut second = doc.views[0].clone();
    second["id"] = json!("V0");
    second["position"] = json!("1");
    doc.views.push(second);
    let views = catalog(&doc).unwrap();
    assert_eq!(views[0].id, "V0");
    assert_eq!(views[1].default_view_key.as_deref(), Some("all_items"));
    assert!(views[1].is_all_items_view && views[1].is_template_initial_view);
    assert_eq!(views[1].raw["unknown_future"]["retained"], true);
}

#[test]
fn writable_empty_select_groups_match_live_table_and_filter_policy() {
    let grouping = json!({"group_by":"status","group_by_column_id":"C_SELECT"});
    for (filters, labels) in [
        (json!([]), vec!["Zebra", "Alpha", "Ungrouped"]),
        (
            json!([{"key":"status","operator":"contains","values":["a"]}]),
            vec!["Alpha"],
        ),
        (
            json!([{"key":"status","operator":"does_not_contain","values":["a"]}]),
            vec!["Zebra", "Ungrouped"],
        ),
        (
            json!([{"key":"n","operator":"equal","values":[999]}]),
            vec![],
        ),
    ] {
        let (doc, mut page, view) = fixture(json!({"grouping":grouping,"filters":filters}));
        for record in &mut page.records {
            record.fields.get_mut("status").unwrap()["select"] = json!([]);
        }
        let writable = ProjectionContext {
            editable: true,
            ..Default::default()
        };
        let projection = project(&doc, &page, &view, &writable).unwrap();
        assert_eq!(
            projection
                .groups
                .iter()
                .map(|g| g.label.as_str())
                .collect::<Vec<_>>(),
            labels
        );
        for group in &projection.groups {
            if group.label != "Ungrouped" {
                assert_eq!(group.count, 0);
                assert!(group.rows.is_empty());
            }
        }
        let readonly = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
        assert!(readonly.groups.iter().all(|group| group.count > 0));
    }
}

#[test]
fn is_me_uses_exact_identity_not_a_prefix_of_another_user() {
    for (operator, expected) in [("is_me", vec!["R1"]), ("is_not_me", vec!["R2", "R3"])] {
        let (doc, mut page, view) =
            fixture(json!({"filters":[{"key":"owner","operator":operator,"values":[]}]}));
        page.records[0].fields.get_mut("owner").unwrap()["user"] = json!(["U10", "U1"]);
        page.records[1].fields.get_mut("owner").unwrap()["user"] = json!(["U10"]);
        let context = ProjectionContext {
            viewer_id: Some("U1".into()),
            ..Default::default()
        };
        assert_eq!(
            ids(&project(&doc, &page, &view, &context).unwrap()),
            expected
        );
    }
}

#[test]
fn verified_personal_visibility_overrides_saved_false_without_ignoring_completed_view() {
    for (key, expected) in [
        ("all_items", vec!["R2", "R3", "R1"]),
        ("completed_items", vec!["R2"]),
    ] {
        let (mut doc, page, view) =
            fixture(json!({"default_view_key":key,"show_completed_items":false}));
        doc.columns
            .iter_mut()
            .find(|c| c.key == "done")
            .unwrap()
            .kind = "todo_completed".into();
        let policy =
            CompletionPolicy::resolve(true, Some(true), Some(true), Some(false), Some(false), None)
                .unwrap();
        let context = ProjectionContext {
            todo_mode: true,
            apply_todo_completion_rules: Some(policy.apply_rules),
            effective_show_completed: Some(policy.effective_show_completed),
            ..Default::default()
        };
        assert_eq!(
            ids(&project(&doc, &page, &view, &context).unwrap()),
            expected
        );
    }
}

#[test]
fn mixed_catalog_positions_are_explicitly_unsupported_instead_of_nontransitive() {
    let (mut doc, _, _) = fixture(json!({"position":"2"}));
    for (id, position) in [("V2", json!("10")), ("V3", json!(3))] {
        let mut view = doc.views[0].clone();
        view["id"] = json!(id);
        view["position"] = position;
        doc.views.push(view);
    }
    assert!(
        catalog(&doc)
            .unwrap_err()
            .reason
            .contains("mix string and numeric")
    );
    doc.views[2]["position"] = json!("3");
    assert_eq!(
        catalog(&doc)
            .unwrap()
            .iter()
            .map(|v| v.id.as_str())
            .collect::<Vec<_>>(),
        vec!["V2", "V1", "V3"]
    );
}

#[test]
fn catalog_rejects_duplicate_or_empty_ids_before_picker_selection() {
    let (mut doc, _, _) = fixture(json!({}));
    let mut duplicate = doc.views[0].clone();
    duplicate["name"] = json!("Conflicting view");
    duplicate["position"] = json!(2);
    doc.views.push(duplicate);
    assert!(catalog(&doc).unwrap_err().reason.contains("ambiguous"));
    assert!(DefaultSelection::new(&doc, Some("V1"), false, None).is_err());
    doc.views[1]["id"] = json!("");
    assert!(catalog(&doc).is_err());
    doc.views[1]["id"] = json!("V2");
    assert_eq!(
        catalog(&doc)
            .unwrap()
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>(),
        vec!["V1", "V2"]
    );
}

#[test]
fn grouped_instance_identity_survives_sort_label_and_collapse_changes() {
    let (mut doc, page, mut view) = fixture(json!({
        "grouping":{"group_by":"status","group_by_column_id":"C_SELECT"}
    }));
    let original_page = page.clone();
    let instances = |projection: &Projection| {
        projection
            .groups
            .iter()
            .flat_map(|group| {
                group.rows.iter().map(|row| {
                    (
                        (group.key.clone(), row.record_id.clone()),
                        row.instance_id.clone(),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    let original_instances = instances(&before);
    assert_eq!(original_instances.len(), 4);
    assert_eq!(
        original_instances.values().collect::<BTreeSet<_>>().len(),
        4
    );

    // A label rename and different display ordering must not remount the same
    // group/record pair or conflate the two instances of R1.
    doc.columns
        .iter_mut()
        .find(|c| c.key == "status")
        .unwrap()
        .raw["options"]["choices"][0]["label"] = json!("Renamed group");
    doc.views[0]["sorts"] = json!([{"key":"status","ascending":true}]);
    view = SavedView::parse(doc.views[0].clone()).unwrap();
    let reordered = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    assert_eq!(instances(&reordered), original_instances);
    assert_eq!(
        reordered
            .groups
            .iter()
            .map(|group| group.key.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("a"), Some("z"), Some("")]
    );
    assert_eq!(
        reordered.groups[0]
            .rows
            .iter()
            .map(|row| row.record_id.as_str())
            .collect::<Vec<_>>(),
        vec!["R1", "R2"]
    );

    let context = ProjectionContext {
        collapsed_groups: ["z", "a", ""].map(str::to_owned).into(),
        ..Default::default()
    };
    let collapsed = project(&doc, &page, &view, &context).unwrap();
    assert!(ids(&collapsed).is_empty());
    assert!(collapsed.groups.iter().all(|g| g.collapsed));
    assert_eq!(collapsed.groups.iter().map(|g| g.count).sum::<usize>(), 4);
    assert_eq!(collapsed.unique_record_count, 3);
    assert!(collapsed.incomplete);
    let reopened = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    assert_eq!(instances(&reopened), original_instances);
    assert_eq!(page, original_page);
}

#[test]
fn empty_filtered_or_unloaded_pages_do_not_claim_complete_results() {
    let (doc, mut page, view) = fixture(json!({
        "grouping":{"group_by":"status","group_by_column_id":"C_SELECT"},
        "filters":[{"key":"n","operator":"equal","values":[999]}]
    }));
    let projection = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    assert_eq!(projection.unique_record_count, 0);
    assert!(projection.groups.is_empty());
    assert!(projection.incomplete);
    assert_eq!(page.records.len(), 3);

    page.records.clear();
    let empty = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    assert!(empty.incomplete);
    assert_eq!(empty.unique_record_count, 0);
    assert!(all_loaded(&doc, &page).incomplete);
    page.next_cursor = None;
    let complete = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    assert!(!complete.incomplete);
    assert!(complete.groups.is_empty());
    assert!(!all_loaded(&doc, &page).incomplete);
}

#[test]
fn unsupported_rules_are_rejected_even_without_any_loaded_rows() {
    for patch in [
        json!({"filters":[{"key":"date","operator":"equal","values":["today"]}]}),
        json!({"filters":[{"key":"n","operator":"future_operator","values":[1]}]}),
        json!({"filters":[{"key":"n","operator":"equal","values":["not a number"]}]}),
        json!({"filters":[{"key":"owner","operator":"is_me","values":[]}]}),
        json!({"sorts":[{"key":"n","column_id":"REPLACED","ascending":true}]}),
        json!({"grouping":{"group_by":"date","group_by_column_id":"C_DATE"}}),
        json!({"grouping":{"group_by":"status","group_by_column_id":"REPLACED"}}),
    ] {
        let (doc, mut page, view) = fixture(patch.clone());
        page.records.clear();
        assert!(
            project(&doc, &page, &view, &ProjectionContext::default()).is_err(),
            "unsupported rule must not masquerade as an empty supported view: {patch}"
        );
        let fallback = all_loaded(&doc, &page);
        assert_eq!(fallback.view_id, None);
        assert_eq!(fallback.title, "All loaded items");
        assert!(fallback.incomplete);
        assert_eq!(fallback.unique_record_count, 0);
    }
}

#[test]
fn unknown_task_completion_policy_rejects_empty_and_filtered_empty_pages() {
    for no_loaded_rows in [false, true] {
        let (mut doc, mut page, view) = fixture(json!({
            "filters":[{"key":"n","operator":"equal","values":[999]}]
        }));
        doc.columns
            .iter_mut()
            .find(|c| c.key == "done")
            .unwrap()
            .kind = "todo_completed".into();
        if no_loaded_rows {
            page.records.clear();
        }
        let mut context = ProjectionContext {
            todo_mode: true,
            ..Default::default()
        };
        assert!(
            project(&doc, &page, &view, &context)
                .unwrap_err()
                .reason
                .contains("verified completion-visibility policy")
        );
        for known_policy in [false, true] {
            context.apply_todo_completion_rules = Some(known_policy);
            let projection = project(&doc, &page, &view, &context).unwrap();
            assert_eq!(projection.unique_record_count, 0);
            assert!(ids(&projection).is_empty());
            assert!(projection.incomplete);
        }
        // Ordinary Lists do not require a task-specific completion policy.
        context.todo_mode = false;
        context.apply_todo_completion_rules = None;
        assert!(project(&doc, &page, &view, &context).is_ok());
    }
}

#[test]
fn null_completed_visibility_defaults_false_but_other_nonbooleans_are_rejected() {
    let (mut doc, page, mut view) = fixture(json!({"show_completed_items":null}));
    doc.columns
        .iter_mut()
        .find(|c| c.key == "done")
        .unwrap()
        .kind = "todo_completed".into();
    let context = ProjectionContext {
        todo_mode: true,
        apply_todo_completion_rules: Some(true),
        ..Default::default()
    };
    assert_eq!(
        ids(&project(&doc, &page, &view, &context).unwrap()),
        vec!["R3", "R1"]
    );
    view.raw
        .as_object_mut()
        .unwrap()
        .remove("show_completed_items");
    doc.views[0] = view.raw.clone();
    assert_eq!(
        ids(&project(&doc, &page, &view, &context).unwrap()),
        vec!["R3", "R1"]
    );
    for malformed in [json!(0), json!("false"), json!([]), json!({})] {
        view.raw["show_completed_items"] = malformed;
        doc.views[0] = view.raw.clone();
        assert_eq!(
            project(&doc, &page, &view, &context).unwrap_err().reason,
            "Completed-item visibility is malformed."
        );
    }
    view.raw["show_completed_items"] = json!(true);
    doc.views[0] = view.raw.clone();
    assert_eq!(
        ids(&project(&doc, &page, &view, &context).unwrap()),
        vec!["R2", "R3", "R1"]
    );
}
