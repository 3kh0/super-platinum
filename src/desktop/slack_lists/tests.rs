use super::*;
mod checkbox_missing;
use serde_json::json;
use std::sync::Arc;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
};

pub(super) fn file() -> File {
    serde_json::from_value(json!({"id":"FL1","mode":"list","title":"Plan","access":"write","is_locked":false,
    "list_metadata":{"schema":[{"id":"name","key":"name","name":"Item","type":"rich_text"},{"id":"done","key":"done","name":"Done","type":"checkbox"}]}})).unwrap()
}
pub(super) fn loaded(checked: bool) -> Loaded {
    Loaded {document:Document::from_file(&file()).unwrap(),records:RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"row1","date_created":100,"created_by":"U1","updated_timestamp":"100.0","fields":[
            {"key":"name","text":"Ship","rich_text":[]},{"key":"done","column_id":"done","checkbox":checked}]}]})).unwrap(),allow_lists:Some("full".into()),personal_task_lists:None,list_experiments:None}
}
pub(super) fn make_shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("00000000-0000-4000-8000-000000009996".into());
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.self_account.user_id = user.clone();
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic".into(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team,
                user_id: user,
                enterprise_id: None,
                name: "Fixture".into(),
                url: "https://example.test".into(),
                token: "synthetic".into(),
            },
        )]
        .into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    assert!(open_shell(&mut shell, file()));
    read(&mut shell, loaded(false));
    shell
}
pub(super) fn read(shell: &mut ShellState, loaded: Loaded) {
    let request = begin_read(shell).unwrap();
    finish_read(shell, &request, Ok(loaded));
}
pub(super) fn saving(shell: &mut ShellState) -> EditAttempt {
    let attempt = begin_edit(shell, "row1", "done").unwrap();
    validate_edit(shell, &attempt, loaded(false)).unwrap();
    attempt
}
fn cell(shell: &ShellState) -> Option<bool> {
    presentation(shell).unwrap().rows[0].cells[1].checkbox
}

#[test]
fn todo_completion_requires_ordinary_identity_again_at_preflight() {
    fn snapshot(personal_id: Option<&str>) -> Loaded {
        let mut value = loaded(false);
        let mut raw = value.document.raw.clone();
        raw["list_metadata"]["schema"][1]["type"] = json!("todo_completed");
        value.document = Document::parse(raw).unwrap();
        value.personal_task_lists = personal_id.map(|id| {
            PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
                "task_lists":"[]","assigned_to_list":id
            }}))
            .unwrap()
        });
        value
    }
    for invalid_policy in [None, Some("FL1")] {
        let mut shell = make_shell();
        read(&mut shell, snapshot(invalid_policy));
        assert!(!presentation(&shell).unwrap().rows[0].cells[1].editable);
        assert!(begin_edit(&mut shell, "row1", "done").is_err());
        read(&mut shell, snapshot(Some("F_OTHER")));
        assert!(presentation(&shell).unwrap().rows[0].cells[1].editable);
        let attempt = begin_edit(&mut shell, "row1", "done").unwrap();
        assert!(validate_edit(&mut shell, &attempt, snapshot(invalid_policy)).is_err());
        assert!(
            shell
                .slack_lists
                .operations
                .get("FL1")
                .is_some_and(|m| m.phase == Phase::Preparing)
        );
    }
    let mut shell = make_shell();
    read(&mut shell, snapshot(Some("F_OTHER")));
    let attempt = begin_edit(&mut shell, "row1", "done").unwrap();
    assert!(validate_edit(&mut shell, &attempt, snapshot(Some("F_OTHER"))).is_ok());
    assert_eq!(
        attempt.edit.cells(),
        json!([{"row_id":"row1","column_id":"done","checkbox":true}])
    );
}

#[test]
fn duplicate_edits_are_locked_and_only_fresh_readback_changes_the_display() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    assert!(begin_read(&mut shell).is_none());
    assert_eq!(cell(&shell), Some(false));
    finish_edit(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"cells":attempt.edit.cells()})),
    );
    assert_eq!(cell(&shell), Some(false));
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    read(&mut shell, loaded(true));
    assert_eq!(cell(&shell), Some(true));
    assert!(shell.slack_lists.operations.is_empty());
    assert!(presentation(&shell).unwrap().error.is_none());
    assert!(begin_edit(&mut shell, "row1", "done").is_ok());
}
#[test]
fn uncertain_write_survives_reopen_and_failed_or_incomplete_readback() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Offline("response lost".into())),
    );
    close_shell(&mut shell);
    assert!(open_shell(&mut shell, file()));
    let request = begin_read(&mut shell).unwrap();
    finish_read(
        &mut shell,
        &request,
        Err(Error::Transport("read failed".into())),
    );
    assert!(shell.slack_lists.operations.contains_key("FL1"));
    let mut partial = loaded(true);
    partial.records.records.clear();
    partial.records.next_cursor = Some("more".into());
    read(&mut shell, partial);
    assert!(presentation(&shell).unwrap().incomplete);
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    read(&mut shell, loaded(false));
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(cell(&shell), Some(false));
    assert!(begin_edit(&mut shell, "row1", "done").is_ok());
}
#[test]
fn changed_identity_does_not_resolve_uncertain_write() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    finish_edit(&mut shell, &attempt, Ok(json!({"ok":true})));
    let mut changed = loaded(true);
    changed.document.columns[1].raw["name"] = json!("Different column");
    read(&mut shell, changed);
    assert!(shell.slack_lists.operations.contains_key("FL1"));
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
}
#[test]
fn rejected_write_unlocks_but_requires_refresh_and_rate_limit_wait() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::RateLimited {
            retry_after_secs: Some(120),
        }),
    );
    assert!(shell.slack_lists.operations.is_empty());
    assert!(begin_read(&mut shell).is_none());
    assert_eq!(cell(&shell), Some(false));
    shell.slack_lists.view.as_mut().unwrap().retry_at = now() - 1;
    assert!(poll(&mut shell));
    read(&mut shell, loaded(false));
    assert!(begin_edit(&mut shell, "row1", "done").is_ok());
}
#[test]
fn permission_schema_cell_and_realtime_changes_cancel_before_dispatch() {
    for change in 0..5 {
        let mut shell = make_shell();
        let attempt = begin_edit(&mut shell, "row1", "done").unwrap();
        let mut current = loaded(false);
        match change {
            0 => current.allow_lists = None,
            1 => current.document.raw["is_locked"] = json!(true),
            2 => current.document.columns[1].raw["name"] = json!("Changed"),
            3 => current.records.records[0].raw["updated_timestamp"] = json!("101.0"),
            _ => {
                let team = shell.core.active_team.clone().unwrap();
                assert!(invalidate(&mut shell, &team, "FL1"));
            }
        }
        assert!(validate_edit(&mut shell, &attempt, current).is_err());
        cancel_preflight(&mut shell, &attempt, "Changed before dispatch");
        assert!(shell.slack_lists.operations.is_empty());
        assert_eq!(cell(&shell), Some(false));
    }
}
#[test]
fn stale_reads_keep_loaded_content_and_cannot_overwrite_a_new_visit_or_event() {
    let mut shell = make_shell();
    let original = presentation(&shell).unwrap().rows;
    let request = begin_read(&mut shell).unwrap();
    let team = shell.core.active_team.clone().unwrap();
    assert!(invalidate(&mut shell, &team, "FL1"));
    finish_read(&mut shell, &request, Ok(loaded(true)));
    assert_eq!(cell(&shell), Some(false));
    assert!(Arc::ptr_eq(&original, &presentation(&shell).unwrap().rows));
    let request = begin_read(&mut shell).unwrap();
    assert!(open_shell(&mut shell, file()));
    finish_read(&mut shell, &request, Ok(loaded(true)));
    assert!(presentation(&shell).unwrap().rows.is_empty());
    read(&mut shell, loaded(false));
    let request = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &request, Err(Error::Api("not_allowed".into())));
    assert_eq!(cell(&shell), Some(false));
    assert!(presentation(&shell).unwrap().stale);
}
#[test]
fn account_viewer_session_and_transport_changes_retire_requests_without_feedback() {
    for change in 0..8 {
        let mut shell = make_shell();
        let request = begin_read(&mut shell).unwrap();
        let team = shell.core.active_team.clone().unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("other".into()),
            2 => shell.core.active_team = Some("other".into()),
            3 => shell.self_account.user_id = "other".into(),
            4 => shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "other".into(),
            5 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token = "rotated".into()
            }
            6 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .url = "https://other.test".into()
            }
            _ => shell.core.transport = Some(Arc::new(Transport::new("other").unwrap())),
        }
        finish_read(&mut shell, &request, Ok(loaded(true)));
        assert!(presentation(&shell).is_none());
        assert!(poll(&mut shell));
        assert!(shell.slack_lists.view.is_none());
        assert!(shell.slack_lists.operations.is_empty());
        assert_ne!(shell.overlay, Some(Overlay::SlackList));
    }
}
#[test]
fn deletion_readonly_and_nonlist_never_allow_editing() {
    let mut shell = make_shell();
    let team = shell.core.active_team.clone().unwrap();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .deleted_files
        .insert("FL1".into());
    assert!(poll(&mut shell));
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    assert!(begin_read(&mut shell).is_none());
    let mut shell = make_shell();
    let mut read_only = loaded(false);
    read_only.allow_lists = None;
    read(&mut shell, read_only);
    assert!(presentation(&shell).unwrap().read_only_reason.is_some());
    assert!(!presentation(&shell).unwrap().rows[0].cells[1].editable);
    let mut other = file();
    other.extra.insert("mode".into(), json!("hosted"));
    assert!(open_shell(&mut shell, other));
    assert!(begin_read(&mut shell).is_none());
}
#[test]
fn ordinary_presentation_reuses_row_projection() {
    let shell = make_shell();
    let first = presentation(&shell).unwrap();
    let second = presentation(&shell).unwrap();
    assert!(Arc::ptr_eq(&first.rows, &second.rows));
}

#[test]
fn stale_write_completion_cannot_touch_another_account_or_list_visit() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    shell.core.account_epoch += 1;
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Offline("late result".into())),
    );
    assert!(poll(&mut shell));
    assert!(shell.slack_lists.operations.is_empty());
    assert!(cell_receipts::get(&shell, &attempt).is_some_and(|m| m.phase == Phase::Uncertain));
    let mut shell = make_shell();
    let attempt = begin_edit(&mut shell, "row1", "done").unwrap();
    close_shell(&mut shell);
    assert!(open_shell(&mut shell, file()));
    assert!(validate_edit(&mut shell, &attempt, loaded(false)).is_err());
    cancel_preflight(&mut shell, &attempt, "Visit changed");
    assert!(shell.slack_lists.operations.is_empty());
}

#[test]
fn reconnect_replaces_loading_read_and_preserves_mutation_locks() {
    let mut shell = make_shell();
    let previous = begin_read(&mut shell).unwrap();
    assert!(invalidate_visible_for_reconnect(&mut shell));
    let fresh = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &previous, Ok(loaded(true)));
    assert_eq!(cell(&shell), Some(false));
    assert!(presentation(&shell).unwrap().loading);
    finish_read(&mut shell, &fresh, Ok(loaded(false)));
    assert!(!presentation(&shell).unwrap().loading);
    assert!(!presentation(&shell).unwrap().stale);
    let attempt = begin_edit(&mut shell, "row1", "done").unwrap();
    assert!(invalidate_visible_for_reconnect(&mut shell));
    assert!(begin_read(&mut shell).is_none());
    assert!(validate_edit(&mut shell, &attempt, loaded(false)).is_err());
    cancel_preflight(&mut shell, &attempt, "Read again after reconnect");
    read(&mut shell, loaded(false));
    let attempt = saving(&mut shell);
    assert!(invalidate_visible_for_reconnect(&mut shell));
    assert!(begin_read(&mut shell).is_none());
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Offline("lost reply".into())),
    );
    assert!(begin_read(&mut shell).is_some());
}

#[test]
fn mutation_phases_reuse_base_rows_and_expose_only_the_small_target() {
    let mut shell = make_shell();
    let original = presentation(&shell).unwrap().rows;
    let attempt = begin_edit(&mut shell, "row1", "done").unwrap();
    let preparing = presentation(&shell).unwrap();
    assert!(Arc::ptr_eq(&original, &preparing.rows));
    let mutation = preparing.mutation.unwrap();
    assert_eq!(mutation.record_id, "row1");
    assert_eq!(mutation.column_id, "done");
    assert!(!mutation.uncertain);
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    validate_edit(&mut shell, &attempt, loaded(false)).unwrap();
    assert!(Arc::ptr_eq(&original, &presentation(&shell).unwrap().rows));
    finish_edit(&mut shell, &attempt, Err(Error::Offline("unknown".into())));
    let uncertain = presentation(&shell).unwrap();
    assert!(Arc::ptr_eq(&original, &uncertain.rows));
    assert!(uncertain.mutation.unwrap().uncertain);
    read(&mut shell, loaded(false));
    let original = presentation(&shell).unwrap().rows;
    let attempt = begin_edit(&mut shell, "row1", "done").unwrap();
    cancel_preflight(&mut shell, &attempt, "Access changed");
    let canceled = presentation(&shell).unwrap();
    assert!(Arc::ptr_eq(&original, &canceled.rows));
    assert!(canceled.mutation.is_none());
    assert!(canceled.stale);
}

#[test]
fn realtime_burst_keeps_one_read_and_requests_one_fresh_replacement() {
    let mut shell = make_shell();
    let original = presentation(&shell).unwrap().rows;
    let request = begin_read(&mut shell).unwrap();
    let team = shell.core.active_team.clone().unwrap();
    for _ in 0..50 {
        assert!(invalidate(&mut shell, &team, "FL1"));
        assert!(begin_read(&mut shell).is_none());
        assert!(presentation(&shell).unwrap().loading);
    }
    assert!(finish_read(&mut shell, &request, Ok(loaded(true))));
    assert!(Arc::ptr_eq(&original, &presentation(&shell).unwrap().rows));
    let fresh = begin_read(&mut shell).unwrap();
    assert!(begin_read(&mut shell).is_none());
    // Duplicate delivery of the obsolete result cannot request a third read.
    assert!(!finish_read(&mut shell, &request, Ok(loaded(true))));
    assert!(!finish_read(&mut shell, &fresh, Ok(loaded(false))));
    assert_eq!(cell(&shell), Some(false));
    assert!(!presentation(&shell).unwrap().stale);
    assert!(!finish_read(&mut shell, &fresh, Ok(loaded(true))));
    assert_eq!(cell(&shell), Some(false));
}

#[test]
fn coalesced_rate_limit_defers_replacement_until_explicit_read_after_cooldown() {
    for error in [
        Error::RateLimited {
            retry_after_secs: Some(120),
        },
        Error::HttpStatus {
            status: 429,
            retry_after_secs: Some(120),
        },
    ] {
        let mut shell = make_shell();
        let request = begin_read(&mut shell).unwrap();
        let team = shell.core.active_team.clone().unwrap();
        assert!(invalidate(&mut shell, &team, "FL1"));
        assert!(!finish_read(&mut shell, &request, Err(error)));
        let view = presentation(&shell).unwrap();
        assert!(!view.loading);
        assert!(view.stale);
        assert_eq!(cell(&shell), Some(false));
        assert!(begin_read(&mut shell).is_none());
        shell.slack_lists.view.as_mut().unwrap().retry_at = now() - 1;
        assert!(poll(&mut shell));
        assert!(begin_read(&mut shell).is_some());
    }
}

#[test]
fn hierarchy_collapses_locally_across_page_boundaries_without_reprojecting_cells() {
    let mut shell = make_shell();
    let mut data = loaded(false);
    let make = |id: String, parent: Option<&str>, position: i64| json!({"id":id,"parent_record_id":parent,"position":position,"fields":[{"key":"name","text":id,"rich_text":[]},{"key":"done","checkbox":false}]});
    let mut records = (0..52)
        .map(|index| make(format!("root-{index:03}"), None, index))
        .collect::<Vec<_>>();
    records.push(make("child-b".into(), Some("root-049"), 2));
    records.push(make("child-a".into(), Some("root-049"), 1));
    data.records = RecordsPage::parse(json!({"ok":true,"records":records})).unwrap();
    read(&mut shell, data.clone());
    let collapsed = presentation(&shell).unwrap();
    assert_eq!(collapsed.total_rows, 54);
    assert_eq!(collapsed.rows.len(), 52);
    assert_eq!(collapsed.rows[49].id, "root-049");
    let all = shell.slack_lists.view.as_ref().unwrap().all_rows.clone();
    let cells = collapsed.rows[49].cells.clone();
    let generation = collapsed.generation;
    assert!(toggle_expanded_shell(&mut shell, "root-049", generation));
    let expanded = presentation(&shell).unwrap();
    assert_eq!(expanded.rows.len(), 54);
    assert_eq!(expanded.rows[50].id, "child-a");
    assert_eq!(expanded.rows[51].id, "child-b");
    assert_eq!(expanded.rows[50].depth, 1);
    assert!(expanded.rows[49].expanded);
    assert!(Arc::ptr_eq(&cells, &expanded.rows[49].cells));
    assert!(Arc::ptr_eq(
        &all,
        &shell.slack_lists.view.as_ref().unwrap().all_rows
    ));
    assert!(toggle_expanded_shell(&mut shell, "root-049", generation));
    assert_eq!(presentation(&shell).unwrap().rows[50].id, "root-050");
    assert!(Arc::ptr_eq(
        &cells,
        &presentation(&shell).unwrap().rows[49].cells
    ));
    assert!(!toggle_expanded_shell(&mut shell, "child-a", generation));
    assert!(toggle_expanded_shell(&mut shell, "root-049", generation));
    read(&mut shell, data);
    assert!(presentation(&shell).unwrap().rows[49].expanded);
    close_shell(&mut shell);
    open_shell(&mut shell, file());
    assert!(!toggle_expanded_shell(&mut shell, "root-049", generation));
}

#[test]
fn orphan_context_and_primary_column_labels_survive_projection() {
    let mut shell = make_shell();
    let mut data = loaded(false);
    // Schema order is unrelated to which column supplies the item identity.
    data.document.columns.swap(0, 1);
    let mut record = data.records.records[0].raw.clone();
    record["parent_record_id"] = json!("unloaded-parent");
    data.records = RecordsPage::parse(
        json!({"ok":true,"records":[record],"response_metadata":{"next_cursor":"more"}}),
    )
    .unwrap();
    read(&mut shell, data);
    let view = presentation(&shell).unwrap();
    assert!(!view.columns[0].primary);
    assert!(view.columns[1].primary);
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].label, "Ship");
    assert_eq!(view.rows[0].parent_id.as_deref(), Some("unloaded-parent"));
    assert_eq!(
        view.rows[0].parent_notice.as_deref(),
        Some("Parent item is not loaded.")
    );
    assert_eq!(view.rows[0].depth, 0);
    assert!(view.incomplete);
}

#[test]
fn schema_without_primary_text_column_keeps_child_expansion_reachable() {
    let mut shell = make_shell();
    let mut data = loaded(false);
    data.document
        .columns
        .retain(|column| column.kind == "checkbox");
    assert!(data.document.primary_column().is_none());
    data.records = RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"parent","fields":[{"key":"done","checkbox":false}]},
        {"id":"child","parent_record_id":"parent","fields":[{"key":"done","checkbox":true}]}
    ]}))
    .unwrap();
    read(&mut shell, data);
    let view = presentation(&shell).unwrap();
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.columns.len(), 1);
    assert!(view.columns[0].primary);
    assert!(view.rows[0].has_children);
    assert_eq!(view.rows[0].label, "Unchecked");
    let generation = view.generation;
    assert!(toggle_expanded_shell(&mut shell, "parent", generation));
    assert_eq!(presentation(&shell).unwrap().rows.len(), 2);
    assert_eq!(presentation(&shell).unwrap().rows[1].id, "child");
    assert!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document
            .primary_column()
            .is_none()
    );
}

#[test]
fn checkbox_gestures_require_current_visit_and_visible_row_but_collapse_keeps_prior_intent() {
    let mut shell = make_shell();
    let mut data = loaded(false);
    data.records = RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"parent","fields":[{"key":"done","checkbox":false}]},
        {"id":"child","parent_record_id":"parent","fields":[{"key":"done","checkbox":false}]}
    ]}))
    .unwrap();
    read(&mut shell, data.clone());
    let visit = generation(&shell).unwrap();
    assert!(checkbox_visit_current(&shell, visit));
    assert!(begin_edit(&mut shell, "child", "done").is_err());
    assert!(shell.slack_lists.operations.is_empty());
    assert!(toggle_expanded_shell(&mut shell, "parent", visit));
    let attempt = begin_edit(&mut shell, "child", "done").unwrap();
    assert!(toggle_expanded_shell(&mut shell, "parent", visit));
    // Collapse only changes local visibility. An explicit edit already begun
    // still has to pass the ordinary fresh schema/permission preflight.
    assert!(validate_edit(&mut shell, &attempt, data).is_ok());
    close_shell(&mut shell);
    assert!(!checkbox_visit_current(&shell, visit));
    open_shell(&mut shell, file());
    assert!(!checkbox_visit_current(&shell, visit));
    assert!(checkbox_visit_current(&shell, generation(&shell).unwrap()));
    shell.overlay = None;
    assert!(!checkbox_visit_current(&shell, generation(&shell).unwrap()));
}
