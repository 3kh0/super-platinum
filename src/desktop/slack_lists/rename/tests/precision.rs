use super::super::precision;
use super::*;
use super_platinum_core::slack::client::RequestBody;

struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn number_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let v = s.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    let mut raw = l.document.raw.clone();
    raw["filetype"] = json!("list");
    raw["list_limits"] = json!({"over_view_maximum":false});
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":0},"opaque":{"keep":true}}));
    raw["list_metadata"]["views"] = json!([
        {"id":"V_PRECISION","name":"Estimates","type":"table","columns":[{"id":"name","key":"name","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true}],"grouping":{},"filters":[],"sorts":[],"opaque":{"keep":true}},
        {"id":"V_BOARD","name":"Board","type":"board","columns":[{"id":"C_NUMBER","key":"estimate","visible":true}],"grouping":{"group_by":"done","group_by_column_id":"done"},"filters":[],"sorts":[]}]);
    raw["list_metadata"]["default_view"] = json!("V_PRECISION");
    l.document = Document::parse(raw).unwrap();
    let mut raw = l.records.raw.clone();
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"estimate","number":[12.5],"opaque":{"keep":true}}));
    raw["records"].as_array_mut().unwrap().push(json!({"id":"child","parent_record_id":"row1","position":2,"fields":[{"key":"name","text":"Child"},{"key":"estimate","number":[1.25]}]}));
    l.records = RecordsPage::parse(raw).unwrap();
    v.layout = Default::default();
    rebuild_rows(&mut s);
    let cleanup = Cleanup(Key::current(&s).unwrap());
    (s, cleanup)
}
fn open_number(s: &mut ShellState) -> u64 {
    let visit = generation(s).unwrap();
    assert!(precision::open_shell(s, "C_NUMBER", visit));
    entry(s).unwrap().serial
}
fn begin_number(s: &mut ShellState, value: u8) -> (Guard, Intent) {
    let serial = open_number(s);
    assert!(precision::set_input(
        s,
        serial,
        generation(s).unwrap(),
        value
    ));
    prepare(s, serial).unwrap()
}
fn number_metadata(s: &ShellState, value: u8) -> network::Metadata {
    let mut m = metadata(s, None);
    m.document.raw["list_metadata"]["schema"][2]["options"]["precision"] = json!(value);
    m.document = Document::parse(m.document.raw).unwrap();
    // Exercise the actual positive files.info parser, not a forged typed model.
    m.document =
        network::document(json!({"ok":true,"file":m.document.raw}), &m.document.id).unwrap();
    m
}
fn sent_number(s: &mut ShellState) -> Guard {
    let (g, i) = begin_number(s, 2);
    let p = admitted(s, &g, &i, &number_metadata(s, 0)).unwrap();
    assert!(p.is_precision() && !p.is_creation() && !p.is_view());
    g
}
#[test]
fn typed_precision_admission_builds_all_six_values_once_with_key_not_id() {
    for desired in 0..=5 {
        let (mut s, _c) = number_shell();
        let v = s.slack_lists.view.as_mut().unwrap();
        let l = v.loaded.as_mut().unwrap();
        l.document.raw["list_metadata"]["schema"][2]["options"]["precision"] =
            json!((desired + 1) % 6);
        l.document = Document::parse(l.document.raw.clone()).unwrap();
        let (g, i) = begin_number(&mut s, desired);
        assert_eq!(entry(&s).unwrap().input, Input::Precision(desired));
        assert!(presentation(&s).unwrap().name.is_empty());
        let permit = admitted(&s, &g, &i, &number_metadata(&s, (desired + 1) % 6)).unwrap();
        let request = permit.request(&s.core.client, &fixture::session());
        let RequestBody::Form(fields) = request.body else {
            panic!()
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-not-a-token".into()),
                ("list_id".into(), "FL1".into()),
                ("key".into(), "estimate".into()),
                ("options".into(), format!("{{\"precision\":{desired}}}")),
                ("backfill_missing_cells_with_default".into(), "true".into()),
                ("_x_reason".into(), "update-column".into())
            ]
        );
        assert!(admitted(&s, &g, &i, &number_metadata(&s, (desired + 1) % 6)).is_none());
        assert!(prepare(&mut s, g.serial).is_none());
    }
}
#[test]
fn typed_input_refuses_wrong_target_stale_callback_invalid_or_unchanged_value() {
    let (mut s, _c) = number_shell();
    let visit = generation(&s).unwrap();
    let serial = open_number(&mut s);
    assert!(!precision::set_input(&s, serial + 1, visit, 2));
    assert!(!precision::set_input(&s, serial, visit + 1, 2));
    assert!(!precision::set_input(&s, serial, visit, 6));
    assert!(prepare(&mut s, serial).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Editing);
    assert!(precision::set_input(&s, serial, visit, 2));
    let key = Key::current(&s).unwrap();
    with_registry(|r| r.entries.get_mut(&key).unwrap().input = Input::Name("2".into()));
    assert!(prepare(&mut s, serial).is_none());
    assert!(entry(&s).unwrap().intent.is_none());
    with_registry(|r| {
        r.entries.remove(&key);
    });
    assert!(open_shell(&mut s, "name", visit));
    assert!(!precision::set_input(
        &s,
        entry(&s).unwrap().serial,
        visit,
        2
    ));
}
#[test]
fn precision_and_rename_view_create_share_one_slot_in_both_directions() {
    for other in [
        Target::Column("name".into()),
        Target::View("V_PRECISION".into()),
        Target::NewView(super_platinum_core::slack_lists::ViewLayout::Table, None),
    ] {
        for precision_first in [true, false] {
            let (mut s, _c) = number_shell();
            let visit = generation(&s).unwrap();
            if precision_first {
                assert!(precision::open_shell(&mut s, "C_NUMBER", visit));
                assert!(!open_target(&mut s, other.clone(), visit));
            } else {
                assert!(open_target(&mut s, other.clone(), visit));
                assert!(!precision::open_shell(&mut s, "C_NUMBER", visit));
            }
            assert!(blocks_mutation(&s));
            assert!(
                super::super::super::editor::begin_shell(&mut s, "row1", "C_NUMBER", visit)
                    .is_err()
            );
        }
    }
}
#[test]
fn preflight_shape_permission_and_owner_fences_refuse_before_dispatch() {
    for kind in [
        "schema",
        "permission",
        "personal",
        "epoch",
        "cookie",
        "visit",
        "read",
        "revision",
        "selection",
    ] {
        let (mut s, _c) = number_shell();
        let (g, i) = begin_number(&mut s, 2);
        let mut m = number_metadata(&s, 0);
        match kind {
            "schema" => {
                m.document.raw["list_metadata"]["schema"][2]["options"]["future"] = json!(true);
                m.document = Document::parse(m.document.raw).unwrap();
            }
            "permission" => m.allow = None,
            "personal" => {
                m.personal = PersonalTaskLists::from_response(
                    &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":"FL1"}}),
                )
                .unwrap()
            }
            "epoch" => s.core.account_epoch += 1,
            "cookie" => s.core.session.as_mut().unwrap().d_cookie.push('x'),
            "visit" => s.slack_lists.view.as_mut().unwrap().generation = next_id(),
            "read" => s.slack_lists.view.as_mut().unwrap().read = next_id(),
            "revision" => s.slack_lists.view.as_mut().unwrap().revision = next_id(),
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
        assert_eq!(with_registry(|r| r.entries[&g.key].stage), Stage::Preparing);
    }
}
#[test]
fn ambiguous_ack_needs_explicit_current_full_readback_and_preserves_all_records() {
    let (mut s, _c) = number_shell();
    let g = sent_number(&mut s);
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .clone();
    let selected = selected_view(&s).map(str::to_owned);
    completed(
        &mut s,
        &g,
        Ok(json!({"ok":true,"column":{"id":"C_NUMBER","options":{"precision":2}}})),
    );
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
    let stale = number_metadata(&s, 0);
    let read = checking(&s);
    assert!(checked(&mut s, &read, Ok(stale)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert_eq!(presentation(&s).unwrap().precision.unwrap().desired, 2);
    let desired = number_metadata(&s, 2);
    let read = checking(&s);
    assert!(checked(&mut s, &read, Ok(desired)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let after = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(after.records.raw, before.records.raw);
    let mut expected = before.document.raw;
    expected["list_metadata"]["schema"][2]["options"]["precision"] = json!(2);
    assert_eq!(after.document.raw, expected);
    assert_eq!(selected_view(&s), selected.as_deref());
    let number = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .find(|r| r.id == "row1")
        .unwrap()
        .cells
        .iter()
        .find(|c| c.column_id == "C_NUMBER")
        .unwrap();
    assert_eq!(number.text, "12.50");
    assert_eq!(
        presentation(&s).unwrap().precision.unwrap().current,
        Some(2)
    );
    assert!(prepare(&mut s, g.serial).is_none());
}
#[test]
fn stale_read_job_and_wrong_incarnation_never_observe_or_project() {
    for kind in [
        "visit",
        "read",
        "revision",
        "epoch",
        "job",
        "incarnation",
        "partial",
    ] {
        let (mut s, _c) = number_shell();
        let g = sent_number(&mut s);
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let read = checking(&s);
        let mut m = number_metadata(&s, 2);
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
        match kind {
            "visit" => s.slack_lists.view.as_mut().unwrap().generation = next_id(),
            "read" => s.slack_lists.view.as_mut().unwrap().read = next_id(),
            "revision" => s.slack_lists.view.as_mut().unwrap().revision = next_id(),
            "epoch" => s.core.account_epoch += 1,
            "job" => with_registry(|r| r.entries.get_mut(&g.key).unwrap().job = Some(next_id())),
            "incarnation" => {
                m.document.raw["created"] = json!(999);
                m.document = Document::parse(m.document.raw).unwrap();
            }
            "partial" => {
                m.document.raw["list_metadata"]
                    .as_object_mut()
                    .unwrap()
                    .remove("opaque");
                m.document = Document::parse(m.document.raw).unwrap();
            }
            _ => unreachable!(),
        }
        let applied = checked(&mut s, &read, Ok(m));
        assert_ne!(
            with_registry(|r| r.entries[&g.key].stage),
            Stage::Observed,
            "{kind}"
        );
        if kind == "partial" {
            assert!(applied);
            assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        } else {
            assert!(!applied);
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
        }
    }
}
#[test]
fn late_result_and_owner_cooldown_retained_without_reopen_replay() {
    let (mut s, _c) = number_shell();
    let g = sent_number(&mut s);
    let original_account = s.core.active_account.clone();
    super::super::super::close_shell(&mut s);
    let (mut other, _other) = number_shell();
    completed(
        &mut other,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::RateLimited {
            retry_after_secs: Some(30),
        })),
    );
    assert!(!blocks_mutation(&other));
    assert!(registry::blocked(&g.key.owner));
    assert!(!registry::blocked(&Key::current(&other).unwrap().owner));
    other.core.active_account = original_account;
    let file =
        serde_json::from_value(with_registry(|r| r.entries[&g.key].original.raw.clone())).unwrap();
    assert!(super::super::super::open_shell(&mut other, file));
    assert_eq!(entry(&other).unwrap().input, Input::Precision(2));
    assert_eq!(entry(&other).unwrap().stage, Stage::Uncertain);
    assert!(!presentation(&other).unwrap().can_check);
    assert!(prepare(&mut other, g.serial).is_none());
    assert!(!precision::can_open(&other, "C_NUMBER"));
}
#[test]
fn precision_readback_parser_rejects_missing_wrong_owner_and_malformed_positive_results() {
    let (s, _c) = number_shell();
    let doc = number_metadata(&s, 2).document;
    for raw in [
        json!({"ok":true}),
        json!({"ok":false,"file":doc.raw}),
        json!({"ok":true,"error":"denied","file":doc.raw}),
        json!({"ok":null,"file":doc.raw}),
    ] {
        assert!(network::document(raw, "FL1").is_err());
    }
    assert!(network::document(json!({"ok":true,"file":doc.raw}), "OTHER").is_err());
}

#[test]
fn observed_format_reprojects_board_and_keeps_loaded_hierarchy_expansion() {
    let (mut s, _c) = number_shell();
    let v = s.slack_lists.view.as_mut().unwrap();
    v.layout.selected = None;
    v.layout.manual = true;
    rebuild_rows(&mut s);
    let visit = generation(&s).unwrap();
    assert!(super::super::super::toggle_expanded_shell(
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
        .records
        .raw
        .clone();
    let g = sent_number(&mut s);
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let read = checking(&s);
    let fresh = number_metadata(&s, 2);
    assert!(checked(&mut s, &read, Ok(fresh)));
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(v.expanded.contains("row1"));
    assert!(v.rows.iter().any(|r| r.id == "child" && r.depth == 1));
    assert_eq!(v.loaded.as_ref().unwrap().records.raw, before);
    // Pure projector probe: no navigation, editor action or server write implied.
    let v = s.slack_lists.view.as_mut().unwrap();
    v.layout.selected = Some("V_BOARD".into());
    rebuild_rows(&mut s);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(matches!(
        v.layout.kind,
        super_platinum_core::slack_lists::saved_views::ProjectionLayout::Board
    ));
    let parent = v
        .layout
        .lanes
        .iter()
        .flat_map(|lane| lane.rows.iter())
        .find(|r| r.id == "row1")
        .unwrap();
    assert_eq!(
        parent
            .cells
            .iter()
            .find(|c| c.column_id == "C_NUMBER")
            .unwrap()
            .text,
        "12.50"
    );
    assert_eq!(v.loaded.as_ref().unwrap().records.raw, before);
}

mod number_rename;
