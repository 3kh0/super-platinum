//! Disconnected canonical backend for native interaction checks. No transport is constructed.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{api, client::RequestBody},
};
pub(crate) mod board_creation;
pub(crate) mod board_heading;
pub(crate) mod board_move;
mod board_views;
mod channel_edit;
pub(crate) mod column_precision;
pub(crate) mod column_rename;
pub(crate) mod compound_filter;
mod creation;
mod date_edit;
mod date_views;
mod duplicate;
pub(crate) mod entity_settings;
pub(crate) mod field_delete;
pub(crate) mod field_visibility;
pub(crate) mod find;
pub(crate) mod hydration;
mod link_edit;
pub(crate) mod number_filter;
pub(crate) mod number_rename;
pub(crate) mod people_avatars;
pub(crate) mod rating;
mod record_detail;
mod record_open;
pub(crate) mod select_filter;
pub(crate) mod text_field_create;
mod user_edit;
pub(crate) mod view_create;
pub(crate) mod view_filter;
pub(crate) mod view_grouping;
pub(crate) mod view_order;
pub(crate) mod view_rename;
pub(crate) mod view_sort;
pub(crate) mod vote;
pub(crate) use board_views::{add_board_views, enable_board_tasks, restore_board_views};
pub(crate) use channel_edit::{add_channel_edits, channel_edit_value, restore_channel_edits};
pub(super) use creation::dispatch_creation;
pub(crate) use creation::{
    add_creation_views, complete_creation, created_record, creation_attempts,
    creation_prepared_valid, restore_creation_views, set_creation_limit, set_creation_uncertain,
};
pub(crate) use date_edit::{
    add_date_edits, change_date_edit_due, date_edit_value, restore_date_edits,
};
pub(crate) use date_views::{add_date_views, remove_date_views, set_date_clock, tick_date_clock};
pub(crate) use duplicate::add_duplicate_views;
pub(crate) use link_edit::{add_link_edits, link_edit_value, restore_link_edits};
pub(crate) use record_detail::{add_record_details, change_detail_number};
pub(super) use record_open::record_open;
pub(crate) use record_open::{record_open_count, record_open_valid};
pub(crate) use user_edit::{add_user_edits, restore_user_edits, user_edit_value};

#[derive(Clone)]
pub(crate) struct Fixture {
    checkbox: Option<text_field_create::checkbox::State>,
    date: Option<text_field_create::date::State>,
    number_precision: Option<text_field_create::number_precision::State>,
    rating: Option<text_field_create::rating::State>,
    vote: Option<text_field_create::vote::State>,
    select_create: Option<text_field_create::select::State>,
    people_create: Option<text_field_create::people::State>,
    link_create: Option<text_field_create::link::State>,
    channel_create: Option<text_field_create::channel::State>,
    board_move_check: Option<board_move::check::State>,
    board_creation: board_creation::State,
    record_open: record_open::State,
    creation: creation::State,
    document: Value,
    records: Value,
    pending: Option<EditAttempt>,
    attempts: usize,
    uncertain: bool,
    error: bool,
    prepared_valid: bool,
    experiments: api::ListExperiments,
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    shell.core.transport.is_none()
        && shell.core.session.is_none()
        && shell.slack_lists.fixture.is_some()
}
fn loaded(fixture: &Fixture) -> Loaded {
    Loaded {
        document: Document::parse(fixture.document.clone()).expect("fixture document"),
        records: RecordsPage::parse(fixture.records.clone()).expect("fixture records"),
        allow_lists: Some("full".into()),
        list_experiments: Some(fixture.experiments),
        personal_task_lists: PersonalTaskLists::from_response(
            &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .ok(),
    }
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(shell.core.transport.is_none() && shell.core.session.is_none());
    let document = json!({"id":"F_LIST_FIXTURE","mode":"list","title":"Launch checklist","access":"write","is_locked":false,
        "list_metadata":{"schema":[
            {"id":"C_NAME","key":"NAME","name":"Item","type":"text","is_primary_column":true},
            {"id":"C_CHECK","key":"done","name":"Done","type":"checkbox"},
            {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number"},
            {"id":"C_FUTURE","key":"future","name":"Extended field","type":"future_field"},
            {"id":"C_RICH","key":"notes","name":"Notes","type":"rich_text"},
            {"id":"C_SELECT","key":"status","name":"Status","type":"select","options":{"choices":[{"value":"OPT_PROGRESS","label":"In progress","color":"blue"},{"value":"OPT_DONE","label":"Done","color":"green"}]}},
            {"id":"C_USER","key":"assignee","name":"Assignee","type":"user"},
            {"id":"C_CHANNEL","key":"channel","name":"Channel","type":"channel"},
            {"id":"C_LINK","key":"link","name":"Reference","type":"link"},
            {"id":"C_CURRENCY","key":"budget","name":"Budget","type":"currency"},
            {"id":"C_MULTI","key":"teams","name":"Teams","type":"select","options":{"format":"multi_select","choices":[{"value":"OPT_DESIGN","label":"Design"},{"value":"OPT_ENGINEERING","label":"Engineering"}]}}
        ],"views":[]}});
    let records=(0..73).map(|i| {
        let mut row=json!({"id":format!("R_LIST_{i:03}"),"list_id":"F_LIST_FIXTURE","position":i,"date_created":1700000000+i,
            "fields":[{"key":"NAME","text":format!("Launch item {:02}",i+1)},
                {"key":"done","checkbox":i%2==1},{"key":"estimate","number":[i]},
                {"key":"future","text":"Future field value","future_payload":{"preserved":true}},
                {"key":"notes","text":"Ready to ship review Spec","rich_text":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                    {"type":"text","text":"Ready to ship","style":{"bold":true}},
                    {"type":"text","text":" review","style":{"italic":true}},
                    {"type":"text","text":" "},
                    {"type":"link","url":"https://example.com/spec","text":"Spec"}
                ]}]}]},
                {"key":"status","select":["OPT_PROGRESS"]},
                {"key":"assignee","user":["U1"]},
                {"key":"channel","channel":["C1"]},
                {"key":"link","link":[{"original_url":"https://example.com/reference","display_name":"Project reference","display_as_url":false}]},
                {"key":"budget","number":[42.5]},
                {"key":"teams","select":[]}
            ]});
        if i==11 || i==12 {row["parent_record_id"]=json!("R_LIST_010");}
        if i==20 {row["fields"].as_array_mut().unwrap().retain(|field|field["key"]!="NAME");}
        if i==5 {row["fields"][2]["number"]=json!([12.5]);}
        row
    }).collect::<Vec<_>>();
    let file: File = serde_json::from_value(document.clone()).expect("fixture file");
    shell.slack_lists.fixture = Some(Fixture {
        checkbox: None,
        date: None,
        number_precision: None,
        rating: None,
        vote: None,
        select_create: None,
        people_create: None,
        link_create: None,
        channel_create: None,
        board_move_check: None,
        board_creation: board_creation::State::default(),
        record_open: record_open::State::default(),
        creation: creation::State::default(),
        document,
        records: json!({"ok":true,"records":records}),
        pending: None,
        attempts: 0,
        uncertain: false,
        error: false,
        prepared_valid: true,
        experiments: api::ListExperiments::default(),
    });
    assert!(open_shell(shell, file));
    match name {
        "slack-list-readonly" => {
            shell.slack_lists.fixture.as_mut().unwrap().document["is_locked"] = json!(true)
        }
        "slack-list-incomplete" => {
            shell.slack_lists.fixture.as_mut().unwrap().records["response_metadata"] =
                json!({"next_cursor":"fixture-unloaded-page"})
        }
        "slack-list-error" => shell.slack_lists.fixture.as_mut().unwrap().error = true,
        "slack-list-loading" => {
            begin_read(shell);
            return;
        }
        _ => {}
    }
    refresh(shell);
}
pub(crate) fn refresh(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let Some(attempt) = begin_read(shell) else {
        return;
    };
    let fixture = shell.slack_lists.fixture.as_ref().unwrap();
    let mut result = if fixture.error {
        Err(Error::Offline("fixture offline".into()))
    } else {
        Ok(loaded(fixture))
    };
    if let Err(error) = text_field_create::checkbox::read_input(shell, &attempt, &mut result) {
        text_field_create::checkbox::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::date::read_input(shell, &attempt, &mut result) {
        text_field_create::date::fail(shell, error);
        return;
    }
    if let Err(error) =
        text_field_create::number_precision::read_input(shell, &attempt, &mut result)
    {
        text_field_create::number_precision::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::rating::read_input(shell, &attempt, &mut result) {
        text_field_create::rating::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::vote::read_input(shell, &attempt, &mut result) {
        text_field_create::vote::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::select::read_input(shell, &attempt, &mut result) {
        text_field_create::select::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::people::read_input(shell, &attempt, &mut result) {
        text_field_create::people::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::link::read_input(shell, &attempt, &mut result) {
        text_field_create::link::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::channel::read_input(shell, &attempt, &mut result) {
        text_field_create::channel::fail(shell, error);
        return;
    }
    if let Err(error) = board_move::check::read_input(shell, &attempt, &mut result) {
        board_move::check::fail(shell, error);
        return;
    }
    finish_read(shell, &attempt, result);
    if let Err(error) = text_field_create::checkbox::read_completed(shell, &attempt) {
        text_field_create::checkbox::fail(shell, error);
    }
    if let Err(error) = text_field_create::date::read_completed(shell, &attempt) {
        text_field_create::date::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::number_precision::read_completed(shell, &attempt) {
        text_field_create::number_precision::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::rating::read_completed(shell, &attempt) {
        text_field_create::rating::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::vote::read_completed(shell, &attempt) {
        text_field_create::vote::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::select::read_completed(shell, &attempt) {
        text_field_create::select::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::people::read_completed(shell, &attempt) {
        text_field_create::people::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::link::read_completed(shell, &attempt) {
        text_field_create::link::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::channel::read_completed(shell, &attempt) {
        text_field_create::channel::fail(shell, error);
        return;
    }
    if let Err(error) = board_move::check::read_completed(shell, &attempt) {
        board_move::check::fail(shell, error);
        return;
    }
}
pub(crate) fn toggle_checkbox(shell: &mut ShellState, row: &str, column: &str) {
    if !enabled(shell) {
        return;
    }
    let Ok(attempt) = begin_edit(shell, row, column) else {
        return;
    };
    dispatch(shell, attempt);
}
pub(super) fn dispatch(shell: &mut ShellState, attempt: EditAttempt) {
    if !enabled(shell) {
        return;
    }
    let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    if let Err(error) = validate_edit(shell, &attempt, canonical.clone()) {
        cancel_preflight(shell, &attempt, error);
        return;
    }
    if let Err(error) = text_field_create::checkbox::preflight(shell, &attempt, &canonical) {
        text_field_create::checkbox::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::date::preflight(shell, &attempt, &canonical) {
        text_field_create::date::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::number_precision::preflight(shell, &attempt, &canonical)
    {
        text_field_create::number_precision::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::rating::preflight(shell, &attempt, &canonical) {
        text_field_create::rating::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::vote::preflight(shell, &attempt, &canonical) {
        text_field_create::vote::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::select::preflight(shell, &attempt, &canonical) {
        text_field_create::select::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::people::preflight(shell, &attempt, &canonical) {
        text_field_create::people::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::link::preflight(shell, &attempt, &canonical) {
        text_field_create::link::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::channel::preflight(shell, &attempt, &canonical) {
        text_field_create::channel::fail(shell, error);
        return;
    }
    if let Err(error) = board_move::check::preflight(shell, &attempt, &canonical) {
        board_move::check::fail(shell, error);
        return;
    }
    let session = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::list_cell_update(&shell.core.client, &session, &attempt.edit);
    if let Err(error) = text_field_create::checkbox::request(shell, &attempt, &request) {
        text_field_create::checkbox::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::date::request(shell, &attempt, &request) {
        text_field_create::date::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::number_precision::request(shell, &attempt, &request) {
        text_field_create::number_precision::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::rating::request(shell, &attempt, &request) {
        text_field_create::rating::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::vote::request(shell, &attempt, &request) {
        text_field_create::vote::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::select::request(shell, &attempt, &request) {
        text_field_create::select::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::people::request(shell, &attempt, &request) {
        text_field_create::people::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::link::request(shell, &attempt, &request) {
        text_field_create::link::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::channel::request(shell, &attempt, &request) {
        text_field_create::channel::fail(shell, error);
        return;
    }
    if let Err(error) = board_move::check::request(shell, &attempt, &request) {
        board_move::check::fail(shell, error);
        return;
    }
    let no_retry = !request.retry_safe();
    let valid = match request.body {
        RequestBody::Form(fields) => {
            fields
                .iter()
                .find(|(k, _)| k == "list_id")
                .is_some_and(|(_, v)| v == attempt.edit.list_id())
                && fields
                    .iter()
                    .find(|(k, _)| k == "cells")
                    .and_then(|(_, v)| serde_json::from_str::<Value>(v).ok())
                    == Some(attempt.edit.cells())
        }
        _ => false,
    };
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.prepared_valid &= valid && no_retry;
    fixture.attempts += 1;
    fixture.pending = Some(attempt);
}
pub(crate) fn complete_pending(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    let Some(attempt) = fixture.pending.take() else {
        return;
    };
    let cells = link_edit::canonical_cells(&attempt.edit.cells());
    if let Some(cell) = cells.as_array().and_then(|cells| cells.first()) {
        for record in fixture.records["records"].as_array_mut().unwrap() {
            if record["id"] != attempt.edit.record_id() {
                continue;
            }
            let fields = record["fields"].as_array_mut().unwrap();
            let index = fields
                .iter()
                .position(|field| field["key"] == attempt.edit.column_key())
                .unwrap_or_else(|| {
                    fields.push(json!({"key":attempt.edit.column_key()}));
                    fields.len() - 1
                });
            let field = &mut fields[index];
            for key in [
                "checkbox",
                "rich_text",
                "number",
                "rating",
                "select",
                "date",
                "user",
                "channel",
                "link",
            ] {
                if let Some(value) = cell.get(key) {
                    field[key] = value.clone();
                    if key == "rich_text" {
                        // The live service assigns IDs to root rich-text blocks.
                        // Keep the submitted/receipt payload untouched so refresh
                        // exercises semantic readback instead of literal equality.
                        if let Some(blocks) = field[key].as_array_mut() {
                            for (index, block) in blocks.iter_mut().enumerate() {
                                if block["type"] == "rich_text" {
                                    block["block_id"] = json!(format!(
                                        "fixture-server-{}-{index}",
                                        fixture.attempts
                                    ));
                                }
                            }
                        }
                        field.as_object_mut().unwrap().remove("text");
                        field.as_object_mut().unwrap().remove("richText");
                    }
                }
            }
        }
    }
    let uncertain = std::mem::take(&mut fixture.uncertain);
    let result = if uncertain {
        Err(Error::Transport("fixture dropped reply".into()))
    } else {
        Ok(json!({"ok":true,"cells":cells}))
    };
    finish_edit(shell, &attempt, result);
    if let Err(error) = text_field_create::checkbox::completion(shell, &attempt, uncertain) {
        text_field_create::checkbox::fail(shell, error);
    }
    if let Err(error) = text_field_create::date::completion(shell, &attempt, uncertain) {
        text_field_create::date::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::number_precision::completion(shell, &attempt, uncertain)
    {
        text_field_create::number_precision::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::rating::completion(shell, &attempt, uncertain) {
        text_field_create::rating::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::vote::completion(shell, &attempt, uncertain) {
        text_field_create::vote::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::select::completion(shell, &attempt, uncertain) {
        text_field_create::select::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::people::completion(shell, &attempt, uncertain) {
        text_field_create::people::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::link::completion(shell, &attempt, uncertain) {
        text_field_create::link::fail(shell, error);
        return;
    }
    if let Err(error) = text_field_create::channel::completion(shell, &attempt, uncertain) {
        text_field_create::channel::fail(shell, error);
        return;
    }
    if let Err(error) = board_move::check::completion(shell, &attempt, uncertain) {
        board_move::check::fail(shell, error);
        return;
    }
}
pub(crate) fn attempt_count(shell: &ShellState) -> usize {
    shell.slack_lists.fixture.as_ref().map_or(0, |f| f.attempts)
}
pub(crate) fn prepared_valid(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .is_some_and(|f| f.prepared_valid)
}
pub(crate) fn set_outcome_uncertain(shell: &mut ShellState) {
    if enabled(shell) {
        shell.slack_lists.fixture.as_mut().unwrap().uncertain = true;
    }
}
pub(crate) fn set_read_only(shell: &mut ShellState) {
    if enabled(shell) {
        shell.slack_lists.fixture.as_mut().unwrap().document["is_locked"] = json!(true);
        refresh(shell);
    }
}
pub(crate) fn set_incomplete(shell: &mut ShellState) {
    if enabled(shell) {
        shell.slack_lists.fixture.as_mut().unwrap().records["response_metadata"] =
            json!({"next_cursor":"fixture-unloaded-page"});
        refresh(shell);
    }
}
pub(crate) fn set_error(shell: &mut ShellState) {
    if enabled(shell) {
        shell.slack_lists.fixture.as_mut().unwrap().error = true;
        refresh(shell);
    }
}
pub(crate) fn restore(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.error = false;
    fixture.document["is_locked"] = json!(false);
    fixture.records["response_metadata"] = json!({});
    refresh(shell);
}

pub(crate) fn reorder_rows(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let records = shell.slack_lists.fixture.as_mut().unwrap().records["records"]
        .as_array_mut()
        .unwrap();
    let count = records.len();
    for (i, record) in records.iter_mut().enumerate() {
        record["position"] = json!(count - i);
    }
    refresh(shell);
}
pub(crate) fn remove_row(shell: &mut ShellState, row_id: &str) {
    if !enabled(shell) {
        return;
    }
    shell.slack_lists.fixture.as_mut().unwrap().records["records"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["id"] != row_id);
    refresh(shell);
}

pub(crate) fn set_number_precision(shell: &mut ShellState, precision: Option<u64>) {
    if !enabled(shell) {
        return;
    }
    let schema = shell.slack_lists.fixture.as_mut().unwrap().document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap();
    let column = schema
        .iter_mut()
        .find(|column| column["id"] == "C_NUMBER")
        .unwrap();
    column["options"] = precision
        .map(|precision| json!({"precision":precision}))
        .unwrap_or(json!({}));
    refresh(shell);
}

pub(crate) fn set_locale(shell: &mut ShellState, locale: Option<&str>) {
    if !enabled(shell) {
        return;
    }
    let Some(team) = shell.core.active_team.clone() else {
        return;
    };
    shell.core.workspaces.get_mut(&team).unwrap().locale = locale.map(str::to_owned);
    super::locale_changed(shell, &team);
}

/// Add saved-view scenarios only after the existing baseline checks. No view is
/// selected here, and this disconnected helper never invokes a Slack endpoint.
pub(crate) fn add_saved_views(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    let schema = fixture.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap();
    let teams = schema
        .iter_mut()
        .find(|column| column["id"] == "C_MULTI")
        .unwrap();
    teams["options"]["choices"] = json!([
        {"value":"OPT_DESIGN","label":"Design"},
        {"value":"OPT_ENGINEERING","label":"Engineering"},
        {"value":"OPT_OPERATIONS","label":"Operations"}
    ]);
    let column = |id: &str, key: &str, visible: bool| json!({"id":id,"key":key,"visible":visible});
    let all_columns = schema.iter().enumerate().map(|(i, c)| {
        json!({"id":c["id"],"key":c["key"],"visible":true,"position":(5_000_000_000u64+i as u64).to_string()})
    }).collect::<Vec<_>>();
    let mut wide_reordered = all_columns.clone();
    wide_reordered.swap(0, 1);
    for (index, column) in wide_reordered.iter_mut().enumerate() {
        column["position"] = json!(index.to_string());
    }
    let reordered = json!([
        column("C_SELECT", "status", true),
        column("C_NAME", "NAME", true),
        column("C_NUMBER", "estimate", false),
        column("C_CHECK", "done", true)
    ]);
    let compact = json!([
        column("C_NAME", "NAME", true),
        column("C_NUMBER", "estimate", true),
        column("C_CHECK", "done", true)
    ]);
    let grouped = json!([
        column("C_NAME", "NAME", true),
        column("C_CHECK", "done", true),
        column("C_MULTI", "teams", true),
        column("C_NUMBER", "estimate", false)
    ]);
    // These raw shapes follow the captured table view: column IDs and keys are
    // distinct, saved order is the array order, grouping stores both identities.
    fixture.document["list_metadata"]["views"] = json!([
        {"id":"V_LIST_ALL","name":"All items","type":"table","position":"1","is_all_items_view":true,"default_view_key":"all_items","show_completed_items":true,"columns":all_columns,"filters":[],"sorts":[]},
        {"id":"V_LIST_REORDERED","name":"Compact status","type":"table","position":"2","columns":reordered,"filters":[],"sorts":[]},
        {"id":"V_LIST_REORDERED_WIDE","name":"Wide reordered columns","type":"table","position":"7","columns":wide_reordered,"filters":[],"sorts":[]},
        {"id":"V_LIST_FILTERED","name":"Open high estimates","type":"table","position":"3","columns":compact,"filters":[
            {"key":"done","column_id":"C_CHECK","operator":"is_empty","values":[]},
            {"key":"estimate","column_id":"C_NUMBER","operator":"greater_than_or_equal","values":[68]}
        ],"sorts":[{"key":"estimate","column_id":"C_NUMBER","ascending":false}]},
        {"id":"V_LIST_GROUPED","name":"By team","type":"table","position":"4","columns":grouped,"filters":[],"sorts":[],"grouping":{"group_by":"teams","group_by_column_id":"C_MULTI"}},
        {"id":"V_LIST_BOARD","name":"Unsupported preview","type":"future_layout","position":"5","columns":compact,"filters":[],"sorts":[],"grouping":{"group_by":"status","group_by_column_id":"C_SELECT"}},
        {"id":"V_LIST_NONAME","name":"Estimates only","type":"table","position":"6","columns":[
            column("C_NUMBER", "estimate", true), column("C_SELECT", "status", true)
        ],"filters":[],"sorts":[]}
    ]);
    for record in fixture.records["records"].as_array_mut().unwrap() {
        let selected = match record["id"].as_str() {
            Some("R_LIST_002") => json!(["OPT_DESIGN", "OPT_ENGINEERING"]),
            Some("R_LIST_004") => json!(["OPT_DESIGN"]),
            Some("R_LIST_006") => json!(["OPT_ENGINEERING"]),
            _ => json!([]),
        };
        let field = record["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|field| field["key"] == "teams")
            .unwrap();
        field["select"] = selected;
        if record["id"] == "R_LIST_020" {
            let estimate = record["fields"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|field| field["key"] == "estimate")
                .unwrap();
            estimate["number"] = json!([]);
        }
    }
    refresh(shell);
}

#[cfg(test)]
pub(super) fn edit_backend_for_test(
    shell: &mut ShellState,
    edit: impl FnOnce(&mut Value, &mut Value),
) {
    let fixture = shell
        .slack_lists
        .fixture
        .as_mut()
        .expect("disconnected list fixture");
    edit(&mut fixture.document, &mut fixture.records);
}

// Narrow disconnected backend access for the deletion fixture. No production path
// can use these without the same no-transport/no-session fixture gate.
pub(super) fn deletion_loaded(shell: &ShellState) -> Option<Loaded> {
    enabled(shell).then(|| loaded(shell.slack_lists.fixture.as_ref().unwrap()))
}
pub(super) fn deletion_remove(shell: &mut ShellState, id: &str) {
    if !enabled(shell) {
        return;
    }
    let records = shell.slack_lists.fixture.as_mut().unwrap().records["records"]
        .as_array_mut()
        .unwrap();
    records.retain(|record| record["id"] != id && record["parent_record_id"] != id);
}
pub(super) fn deletion_restore(shell: &mut ShellState, restored: &[Value]) {
    if !enabled(shell) {
        return;
    }
    let records = shell.slack_lists.fixture.as_mut().unwrap().records["records"]
        .as_array_mut()
        .unwrap();
    for record in restored {
        if !records.iter().any(|current| current["id"] == record["id"]) {
            records.push(record.clone());
        }
    }
}

pub(crate) mod subitem;
