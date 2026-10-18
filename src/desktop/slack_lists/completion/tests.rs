use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s, "slack-list-todo-completion");
    s
}
fn begin_first(s: &mut ShellState) -> Task {
    let detail = record_detail::presentation(s.slack_lists.view.as_ref().unwrap()).unwrap();
    begin(
        s,
        "R_LIST_000",
        "C_TODO_DONE",
        EditAuthority::RecordDetail(detail.generation),
    )
    .unwrap()
}
fn start(s: &mut ShellState) -> u64 {
    let t = begin_first(s);
    let serial = t.serial;
    fixture::start(s, t);
    serial
}
fn check(s: &mut ShellState, serial: u64) {
    let t = task(s, serial).unwrap();
    fixture::check(s, t, false);
}
#[test]
fn both_named_fixtures_enable_actual_personal_checkbox_without_any_write() {
    for name in [
        "slack-list-todo-completion",
        "slack-list-todo-completion-narrow",
    ] {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install(&mut s, name);
        let p = super::super::presentation(&s).unwrap();
        let detail = p.detail.unwrap();
        let cell = detail
            .row
            .cells
            .iter()
            .find(|c| c.column_id == "C_TODO_DONE")
            .unwrap();
        assert_eq!(cell.checkbox, Some(false));
        assert!(cell.editable);
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records
                .records
                .len(),
            73
        );
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records
                .raw["response_metadata"]["next_cursor"],
            "fixture-unloaded-page"
        );
        assert!(s.slack_lists.completion.fixture.forms.is_empty());
    }
}
#[test]
fn retired_preflight_sends_nothing_and_keeps_scoped_completion_status() {
    let mut s = shell();
    let t = begin_first(&mut s);
    s.overlay = None;
    fixture::start(&mut s, t.clone());
    assert!(s.slack_lists.completion.fixture.forms.is_empty());
    assert!(s.slack_lists.completion.entries[&t.serial].phase == Phase::Stopped);
    assert!(
        s.slack_lists.completion.entries[&t.serial]
            .running
            .is_none()
    );
    assert!(presentation(&s).is_some()); // pure presentation remains owner-scoped; parent dialog is closed
}
#[test]
fn two_actual_forms_and_separate_lost_reply_observations_preserve_all_loaded_metadata() {
    let mut s = shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let serial = start(&mut s);
    assert!(!fixture::complete(&mut s, true, true));
    assert!(s.slack_lists.completion.entries[&serial].phase == Phase::PrimaryUnknown);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before.records
    );
    check(&mut s, serial);
    assert!(s.slack_lists.completion.entries[&serial].phase == Phase::ReferenceReady);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1); // Check never sends the second step
    let next = dispatch_task(&mut s, serial).unwrap();
    fixture::reference(&mut s, next);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 2);
    assert!(!fixture::complete(&mut s, true, true));
    assert!(!fixture::complete(&mut s, true, true)); // same dispatched attempt consumed once
    check(&mut s, serial);
    assert!(s.slack_lists.completion.entries.is_empty());
    let after = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let mut expected = before.records.raw.clone();
    let field = expected["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["key"] == "todo_completed")
        .unwrap();
    field["checkbox"] = json!(true);
    assert_eq!(after.records.raw, expected);
    assert_eq!(after.document.raw, before.document.raw);
    for (form, (list, row)) in s.slack_lists.completion.fixture.forms.iter().zip([
        ("F_LIST_FIXTURE", "R_LIST_000"),
        ("F_TODO_SOURCE", "R_SOURCE"),
    ]) {
        assert_eq!(form["method"], "lists.cells.update");
        let fields = form["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0], json!(["list_id", list]));
        assert_eq!(fields[1][0], "cells");
        let cells: Value = serde_json::from_str(fields[1][1].as_str().unwrap()).unwrap();
        assert_eq!(
            cells,
            json!([{"row_id":row,"column_id":"C_TODO_DONE","checkbox":true}])
        );
        assert_eq!(fields[2], json!(["_x_reason", "bulk-update-cells"]));
    }
}
#[test]
fn changed_primary_reference_does_not_retarget_or_send_after_partial_success() {
    let mut s = shell();
    let serial = start(&mut s);
    fixture::complete(&mut s, true, true);
    check(&mut s, serial);
    super::super::fixture::edit_backend_for_test(&mut s, |_, records| {
        let field = records["records"][0]["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|f| f["key"] == "todo_ref")
            .unwrap();
        field["reference"][0]["list_record"]["row_id"] = json!("NEW_TARGET");
    });
    let next = dispatch_task(&mut s, serial).unwrap();
    fixture::reference(&mut s, next);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1);
    assert!(s.slack_lists.completion.entries[&serial].phase == Phase::Stopped);
    assert!(s.slack_lists.completion.entries[&serial].primary_observed);
}
#[test]
fn late_response_retains_receipt_but_cannot_project_or_adopt_replacement_account() {
    let mut s = shell();
    let serial = start(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    s.core.account_epoch += 1;
    fixture::complete(&mut s, false, true);
    assert!(s.slack_lists.completion.entries[&serial].phase == Phase::PrimaryUnknown);
    assert!(task(&mut s, serial).is_none());
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before.records
    );
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1);
}
#[test]
fn retired_read_cleanup_cannot_release_new_token_or_repeat_write() {
    let mut s = shell();
    let serial = start(&mut s);
    fixture::complete(&mut s, true, true);
    let old = task(&mut s, serial).unwrap();
    failed(&mut s, &old, "read retired", None);
    let new = task(&mut s, serial).unwrap();
    failed(
        &mut s,
        &old,
        "late old read",
        Some(&Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    assert_eq!(
        s.slack_lists.completion.entries[&serial].running,
        Some(new.token)
    );
    assert_eq!(s.slack_lists.completion.entries[&serial].retry_at, 0);
    fixture::check(&mut s, new, false);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1);
}
#[test]
fn retry_after_is_retained_with_uncertain_step_and_prevents_queued_new_reads() {
    let mut s = shell();
    let serial = start(&mut s);
    let (pending, _, _) = s.slack_lists.completion.fixture.pending.take().unwrap();
    assert!(!settled(
        &mut s,
        &pending,
        false,
        Err(Error::RateLimited {
            retry_after_secs: Some(30)
        })
    ));
    assert!(task(&mut s, serial).is_none());
    assert!(!presentation(&s).unwrap().can_check);
    assert!(poll(&mut s));
    s.slack_lists
        .completion
        .entries
        .get_mut(&serial)
        .unwrap()
        .retry_at = 0;
    assert!(poll(&mut s));
    assert!(presentation(&s).unwrap().can_check);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 1);
}
#[test]
fn foreign_mirror_readback_stays_uncertain_even_when_primary_changes() {
    let mut s = shell();
    let serial = start(&mut s);
    fixture::complete(&mut s, true, true);
    check(&mut s, serial);
    let next = dispatch_task(&mut s, serial).unwrap();
    fixture::reference(&mut s, next);
    fixture::complete(&mut s, true, true);
    super::super::fixture::completion_observe(&mut s, "R_LIST_000", false);
    s.slack_lists
        .completion
        .fixture
        .reference
        .as_mut()
        .unwrap()
        .records
        .records[0]
        .raw["list_id"] = json!("F_FOREIGN");
    check(&mut s, serial);
    assert!(s.slack_lists.completion.entries[&serial].phase == Phase::ReferenceUnknown);
    assert!(!presentation(&s).unwrap().can_dismiss);
    assert!(presentation(&s).unwrap().can_check);
    assert_eq!(s.slack_lists.completion.fixture.forms.len(), 2);
}

#[test]
fn absent_page_target_uses_exact_info_without_losing_page_cursor_or_inventing_absence() {
    use super_platinum_core::slack_lists::ListRecordReadback;
    let s = shell();
    let mut loaded = super::super::fixture::deletion_loaded(&s).unwrap();
    let before = loaded.records.raw.clone();
    let mut raw = loaded.records.records[0].raw.clone();
    raw["id"] = json!("R_OFF_PAGE");
    let info = ListRecordReadback::from_response(
        "F_LIST_FIXTURE",
        "R_OFF_PAGE",
        &json!({"ok":true,"record":raw}),
    )
    .unwrap();
    network::extend_target(&mut loaded, &info).unwrap();
    assert_eq!(loaded.records.records.len(), 74);
    assert_eq!(
        loaded.records.raw["response_metadata"],
        before["response_metadata"]
    );
    assert_eq!(
        &loaded.records.raw["records"].as_array().unwrap()[..73],
        before["records"].as_array().unwrap().as_slice()
    );
    assert!(network::extend_target(&mut loaded, &info).is_err());
    let deleted = ListRecordReadback::from_response(
        "F_LIST_FIXTURE",
        "R_DELETED",
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap();
    assert!(network::extend_target(&mut loaded, &deleted).is_err());
    let foreign = ListRecordReadback::from_response(
        "F_FOREIGN",
        "R_X",
        &json!({"ok":true,"record":{"id":"R_X","fields":[]}}),
    )
    .unwrap();
    assert!(network::extend_target(&mut loaded, &foreign).is_err());
}
