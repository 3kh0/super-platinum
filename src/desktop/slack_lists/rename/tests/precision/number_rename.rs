use super::*;
fn begin_name(s: &mut ShellState) -> (Guard, Intent) {
    let visit = generation(s).unwrap();
    assert!(open_shell(s, "C_NUMBER", visit));
    let e = entry(s).unwrap();
    assert_eq!(e.target, Target::Column("C_NUMBER".into()));
    with_registry(|r| {
        r.entries.get_mut(&Key::current(s).unwrap()).unwrap().input =
            Input::Name("Planned effort".into())
    });
    prepare(s, e.serial).unwrap()
}
fn name_metadata(s: &ShellState, renamed: bool) -> network::Metadata {
    let mut m = number_metadata(s, 2);
    if renamed {
        m.document.raw["list_metadata"]["schema"][2]["name"] = json!("Planned effort");
    }
    m.document = network::document(json!({"ok":true,"file":m.document.raw}), "FL1").unwrap();
    m
}
fn shell_two() -> (ShellState, Cleanup) {
    let (mut s, c) = number_shell();
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .document = number_metadata(&s, 2).document;
    rebuild_rows(&mut s);
    (s, c)
}
#[test]
fn number_name_and_precision_share_slot_in_both_directions_and_reject_wrong_input() {
    for name_first in [true, false] {
        let (mut s, _c) = shell_two();
        let visit = generation(&s).unwrap();
        assert!(supports_column(&s, "C_NUMBER"));
        assert!(precision::supports(&s, "C_NUMBER"));
        if name_first {
            assert!(open_shell(&mut s, "C_NUMBER", visit));
            assert!(!precision::open_shell(&mut s, "C_NUMBER", visit));
            let e = entry(&s).unwrap();
            assert!(presentation(&s).unwrap().precision.is_none());
            assert!(!precision::set_input(&s, e.serial, visit, 3));
            with_registry(|r| {
                r.entries.get_mut(&Key::current(&s).unwrap()).unwrap().input = Input::Precision(3)
            });
            assert!(prepare(&mut s, e.serial).is_none());
        } else {
            assert!(precision::open_shell(&mut s, "C_NUMBER", visit));
            assert!(!open_shell(&mut s, "C_NUMBER", visit));
        }
    }
}
#[test]
fn name_ack_is_not_observation_and_full_readback_preserves_precision_and_population() {
    let (mut s, _c) = shell_two();
    s.slack_lists.view.as_mut().unwrap().layout.selected = None;
    s.slack_lists.view.as_mut().unwrap().layout.manual = true;
    rebuild_rows(&mut s);
    let visit = generation(&s).unwrap();
    assert!(super::super::super::super::toggle_expanded_shell(
        &mut s, "row1", visit
    ));
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .clone();
    let (g, i) = begin_name(&mut s);
    let p = admitted(&s, &g, &i, &name_metadata(&s, false)).unwrap();
    let RequestBody::Form(fields) = p.request(&s.core.client, &fixture::session()).body else {
        panic!()
    };
    assert_eq!(
        fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("token", "fixture-not-a-token"),
            ("list_id", "FL1"),
            ("key", "estimate"),
            ("name", "Planned effort"),
            ("options", "{\"precision\":2}"),
            ("backfill_missing_cells_with_default", "true"),
            ("_x_reason", "update-column")
        ]
    );
    assert!(admitted(&s, &g, &i, &name_metadata(&s, false)).is_none());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document,
        before.document
    );
    assert!(prepare(&mut s, g.serial).is_none());
    let read = checking(&s);
    let stale = name_metadata(&s, false);
    assert!(checked(&mut s, &read, Ok(stale)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let read = checking(&s);
    let fresh = name_metadata(&s, true);
    assert!(checked(&mut s, &read, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let v = s.slack_lists.view.as_ref().unwrap();
    let after = v.loaded.as_ref().unwrap();
    let mut expected = before.document.raw.clone();
    expected["list_metadata"]["schema"][2]["name"] = json!("Planned effort");
    assert_eq!(after.document.raw, expected);
    assert_eq!(after.records.raw, before.records.raw);
    assert!(v.expanded.contains("row1"));
    assert!(v.rows.iter().any(|r| r.id == "child" && r.depth == 1));
    assert_eq!(
        v.rows
            .iter()
            .find(|r| r.id == "row1")
            .unwrap()
            .cells
            .iter()
            .find(|c| c.column_id == "C_NUMBER")
            .unwrap()
            .text,
        "12.50"
    );
    s.slack_lists.view.as_mut().unwrap().layout.selected = Some("V_BOARD".into());
    rebuild_rows(&mut s);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.loaded.as_ref().unwrap().records.raw, before.records.raw);
    assert_eq!(
        v.layout
            .lanes
            .iter()
            .flat_map(|l| l.rows.iter())
            .find(|r| r.id == "row1")
            .unwrap()
            .cells
            .iter()
            .find(|c| c.column_id == "C_NUMBER")
            .unwrap()
            .text,
        "12.50"
    );
    assert_eq!(
        v.loaded
            .as_ref()
            .unwrap()
            .document
            .column("C_NUMBER")
            .unwrap()
            .name,
        "Planned effort"
    );
}
#[test]
fn number_name_preflight_refuses_schema_permission_and_stale_owner() {
    for kind in [
        "schema",
        "permission",
        "epoch",
        "visit",
        "read",
        "selection",
    ] {
        let (mut s, _c) = shell_two();
        let (g, i) = begin_name(&mut s);
        let mut m = name_metadata(&s, false);
        match kind {
            "schema" => {
                m.document.raw["list_metadata"]["schema"][2]["options"]["precision"] = json!(3);
                m.document = Document::parse(m.document.raw).unwrap();
            }
            "permission" => m.allow = None,
            "epoch" => s.core.account_epoch += 1,
            "visit" => s.slack_lists.view.as_mut().unwrap().generation = next_id(),
            "read" => s.slack_lists.view.as_mut().unwrap().read = next_id(),
            "selection" => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            _ => unreachable!(),
        }
        assert!(admitted(&s, &g, &i, &m).is_none(), "{kind}");
    }
}
#[test]
fn stale_number_name_read_cannot_observe_or_replace_current_document() {
    let (mut s, _c) = shell_two();
    let (g, i) = begin_name(&mut s);
    assert!(admitted(&s, &g, &i, &name_metadata(&s, false)).is_some());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let read = checking(&s);
    let fresh = name_metadata(&s, true);
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document
        .clone();
    s.slack_lists.view.as_mut().unwrap().read = next_id();
    assert!(!checked(&mut s, &read, Ok(fresh)));
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document,
        before
    );
    assert_ne!(entry(&s).unwrap().stage, Stage::Observed);
}
#[test]
fn late_number_name_result_retains_name_options_and_owner_cooldown_without_replay() {
    let (mut s, _c) = shell_two();
    let (g, i) = begin_name(&mut s);
    assert!(admitted(&s, &g, &i, &name_metadata(&s, false)).is_some());
    let original_account = s.core.active_account.clone();
    super::super::super::super::close_shell(&mut s);
    let (mut other, _other) = shell_two();
    completed(
        &mut other,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::RateLimited {
            retry_after_secs: Some(30),
        })),
    );
    assert!(registry::blocked(&g.key.owner));
    assert!(!registry::blocked(&Key::current(&other).unwrap().owner));
    other.core.active_account = original_account;
    let file =
        serde_json::from_value(with_registry(|r| r.entries[&g.key].original.raw.clone())).unwrap();
    assert!(super::super::super::super::open_shell(&mut other, file));
    let e = entry(&other).unwrap();
    assert_eq!(e.input, Input::Name("Planned effort".into()));
    assert_eq!(e.stage, Stage::Uncertain);
    assert_eq!(
        e.original.column("C_NUMBER").unwrap().raw["options"],
        json!({"precision":2})
    );
    assert!(!presentation(&other).unwrap().can_check);
    assert!(prepare(&mut other, g.serial).is_none());
    assert!(!precision::can_open(&other, "C_NUMBER"));
    assert!(!can_open(&other, "C_NUMBER"));
}
#[test]
fn named_number_rename_seeds_are_disconnected_and_keep_hierarchy_cursor_and_format() {
    for name in [
        "slack-list-number-rename",
        "slack-list-number-rename-narrow",
    ] {
        let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let _cleanup = Cleanup(Key::current(&s).unwrap());
        assert!(s.core.session.is_none() && s.core.transport.is_none());
        assert!(!fixture::enabled(&s)); // Static review must not acquire any HTTP checker capability.
        let records = &s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .raw;
        assert_eq!(records["records"][5]["fields"][2]["number"], json!([12.5]));
        assert_eq!(records["records"][11]["parent_record_id"], "R_LIST_010");
        assert_eq!(
            records["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
        assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
        let p = presentation(&s).unwrap();
        assert_eq!(p.stage, Stage::Editing);
        assert!(p.precision.is_none());
        assert_eq!(p.name, "Estimate");
        assert_eq!(entry(&s).unwrap().target, Target::Column("C_NUMBER".into()));
    }
}
