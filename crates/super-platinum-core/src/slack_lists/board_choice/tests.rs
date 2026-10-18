use super::*;
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn doc() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","created":1,"user":"U1","opaque_file":[1,null],"list_metadata":{
        "opaque":{"preserve":true},"schema":[{"id":"C1","key":"status","name":"Status","type":"select","opaque_column":7,
        "options":{"format":"multi_select","opaque":{"keep":[true,null]},"default_value":"legacy","default_value_typed":{"select":["legacy"]},"choices":[
            {"value":"legacy","label":"Original","color":"blue","opaque":{"choice":[1,2]}},{"value":"other","label":"Other"}]}}],
        "views":[{"id":"V1","name":"Board","type":"board","grouping":{"group_by":"status","opaque":7},"columns":[],"sorts":[],"filters":[]},
        {"id":"R","type":"record","opaque":true},{"id":"I","type":"canvas_inline"}]}})).unwrap()
}
fn capture(action: BoardChoiceAction) -> BoardChoiceChange {
    BoardChoiceChange::capture(&doc(), "V1", action, Some("full"), Some(&personal())).unwrap()
}
fn rename(label: &str) -> BoardChoiceAction {
    BoardChoiceAction::Rename {
        value: "legacy".into(),
        label: label.into(),
    }
}
fn observed(intent: &BoardChoiceChange) -> Document {
    let mut raw = intent.original().raw.clone();
    raw["list_metadata"]["schema"][0]["options"] = intent.options().clone();
    Document::parse(raw).unwrap()
}
#[test]
fn add_keeps_raw_spacing_and_all_unknown_options_choices_and_views() {
    let action = BoardChoiceAction::Add {
        value: "OptABCDEFG1".into(),
        label: "  New lane  ".into(),
        color: "cyan".into(),
    };
    let intent = capture(action);
    assert_eq!(
        intent.options()["choices"][2],
        json!({"value":"OptABCDEFG1","label":"  New lane  ","color":"cyan"})
    );
    let mut after = observed(&intent).raw;
    after["list_metadata"]["schema"][0]["options"]["choices"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(after, intent.original().raw);
    assert!(intent.observes_desired(&observed(&intent)));
}
#[test]
fn rename_uses_ecmascript_trim_empty_is_valid_and_color_preserves_every_other_member() {
    for (input, want) in [
        ("\u{feff} Renamed \u{a0}", "Renamed"),
        ("   ", ""),
        ("\u{0085}name\u{0085}", "\u{0085}name\u{0085}"),
    ] {
        // U+0085 is a Rust control but not ECMAScript trim whitespace: local one-line policy refuses it.
        if input.contains('\u{0085}') {
            assert!(
                BoardChoiceChange::capture(
                    &doc(),
                    "V1",
                    rename(input),
                    Some("full"),
                    Some(&personal())
                )
                .is_err()
            );
            continue;
        }
        let intent = capture(rename(input));
        assert_eq!(intent.options()["choices"][0]["label"], want);
        assert_eq!(
            intent.options()["choices"][0]["opaque"],
            json!({"choice":[1,2]})
        );
    }
    let intent = capture(BoardChoiceAction::Color {
        value: "legacy".into(),
        color: "purple".into(),
    });
    let mut expected = doc().columns[0].raw["options"].clone();
    expected["choices"][0]["color"] = json!("purple");
    assert_eq!(intent.options(), &expected);
}
#[test]
fn lossy_default_or_output_shapes_and_ambiguous_choices_refuse_instead_of_normalizing() {
    for change in 0..6 {
        let mut raw = doc().raw;
        let opts = &mut raw["list_metadata"]["schema"][0]["options"];
        match change {
            0 => opts["emoji_url"] = json!("output"),
            1 => opts["default_value_typed"] = Value::Null,
            2 => opts["default_value_typed"]["rating"] = json!([2]),
            3 => opts["default_value_typed"]["future"] = json!(true),
            4 => opts["choices"][1]["value"] = json!("legacy"),
            _ => opts["choices"][0]["label"] = json!(7),
        };
        assert!(
            BoardChoiceChange::capture(
                &Document::parse(raw).unwrap(),
                "V1",
                rename("Next"),
                Some("full"),
                Some(&personal())
            )
            .is_err()
        );
    }
}
#[test]
fn one_use_full_raw_preflight_and_metadata_readback_not_ack() {
    let original = doc();
    let intent = capture(rename("Next"));
    let expected = observed(&intent);
    let mut attempt = BoardChoiceAttempt::new(intent.clone());
    assert!(!attempt.observe(&expected));
    let _permit = attempt
        .admit(&original, Some("full"), Some(&personal()))
        .unwrap();
    assert!(
        attempt
            .admit(&original, Some("full"), Some(&personal()))
            .is_err()
    );
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), BoardChoicePhase::Unconfirmed);
    assert!(!attempt.observe(&original));
    let mut changed = expected.raw.clone();
    changed["list_metadata"]["opaque"] = json!({});
    assert!(!attempt.observe(&Document::parse(changed).unwrap()));
    assert!(attempt.observe(&expected));
    assert!(!attempt.finish(false));
    let mut changed = original.raw.clone();
    changed["opaque_file"] = json!([]);
    assert!(
        intent
            .revalidate(
                &Document::parse(changed).unwrap(),
                Some("full"),
                Some(&personal())
            )
            .is_err()
    );
}
#[test]
fn read_only_missing_preferences_stale_view_or_wrong_column_never_admit() {
    for change in 0..5 {
        let mut raw = doc().raw;
        match change {
            0 => raw["access"] = json!("read"),
            1 => raw["list_metadata"]["views"][0]["type"] = json!("table"),
            2 => {
                raw["list_metadata"]["views"][0]["grouping"]["group_by_column_id"] = json!("WRONG")
            }
            3 => {
                raw["list_metadata"]["views"][1]["id"] = json!("V1");
                // Duplicate raw view identities are rejected before capture.
                assert_eq!(
                    Document::parse(raw).unwrap_err(),
                    ListError("List views have ambiguous identities.")
                );
                continue;
            }
            _ => raw["list_metadata"]["schema"][0]["type"] = json!("todo_completed"),
        };
        assert!(
            BoardChoiceChange::capture(
                &Document::parse(raw).unwrap(),
                "V1",
                rename("Next"),
                Some("full"),
                Some(&personal())
            )
            .is_err()
        );
    }
    assert!(BoardChoiceChange::capture(&doc(), "V1", rename("Next"), Some("full"), None).is_err());
    assert!(
        BoardChoiceChange::capture(
            &doc(),
            "V1",
            rename("Original"),
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}
#[test]
fn generated_add_is_action_owned_and_local_resource_bounds_are_explicit() {
    let action = BoardChoiceAction::generate_add("New", "green").unwrap();
    let BoardChoiceAction::Add { value, .. } = &action else {
        panic!()
    };
    assert!(value.starts_with("Opt"));
    assert_eq!(value.len(), 11);
    assert_eq!(capture(action.clone()).action(), &action);
    assert!(BoardChoiceAction::generate_add("   ", "green").is_err());
    assert!(BoardChoiceAction::generate_add("New", "red").is_err());
    assert!(BoardChoiceAction::generate_add(&"\u{1f600}".repeat(129), "green").is_err());
}
