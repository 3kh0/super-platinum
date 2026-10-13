//! Disconnected fixture data; responses never reach a Slack transport.
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::slack::api::{
    BlockViewRejection, BlockViewResponse, BlockViewResponseAction, BlockViewSubmission,
};

use super::Session;
use crate::state::{Overlay, ShellState};

pub(crate) fn install(state: &mut ShellState) {
    state.toast = None;
    state.app_modal = Some(Session::fixture(view()));
    state.overlay = Some(Overlay::AppModal);
}

fn option(text: &str, value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":text},"value":value})
}

fn input(id: &str, label: &str, optional: bool, mut element: Value) -> Value {
    element["action_id"] = json!("value");
    json!({"type":"input","block_id":id,"label":{"type":"plain_text","text":label},"optional":optional,"element":element})
}

pub(crate) fn view() -> Value {
    let choices = vec![
        option("Desktop", "desktop"),
        option("Web, mobile", "web,mobile"),
        option("Empty value", ""),
    ];
    let checks = vec![
        option("Documentation", "docs"),
        option("Announcement", "announce"),
    ];
    let radio = vec![
        option("Steady release", "steady"),
        option("Fast release", "fast"),
    ];
    json!({
        "id":"V_FORM_FIXTURE","app_id":"A_FIXTURE","type":"modal","root_view_id":"V_FORM_FIXTURE",
        "title":{"type":"plain_text","text":"Plan a release"},
        "submit":{"type":"plain_text","text":"Review release"},
        "close":{"type":"plain_text","text":"Cancel"},
        "blocks":[
            {"type":"section","block_id":"intro","text":{"type":"mrkdwn","text":"*Release details*\nChoose the channels and schedule for the next release."}},
            input("name", "Release name", false, json!({"type":"plain_text_input","max_length":80,"placeholder":{"type":"plain_text","text":"Name this release"}})),
            input("note", "Release notes", true, json!({"type":"plain_text_input","multiline":true,"initial_value":"Initial note"})),
            input("static", "Primary platform", true, json!({"type":"static_select","options":choices,"initial_option":choices[0]})),
            input("multi", "Additional platforms", true, json!({"type":"multi_static_select","options":choices,"initial_options":[choices[0]],"max_selected_items":3})),
            input("checks", "Include", true, json!({"type":"checkboxes","options":checks,"initial_options":[checks[0]]})),
            input("radio", "Release pace", true, json!({"type":"radio_buttons","options":radio,"initial_option":radio[0]})),
            input("date", "Release date", true, json!({"type":"datepicker","initial_date":"2026-10-15"})),
            input("time", "Release time", true, json!({"type":"timepicker","initial_time":"09:30","timezone":"America/New_York"})),
            input("datetime", "Reminder", true, json!({"type":"datetimepicker","initial_date_time":1792069200})),
            {"type":"context","block_id":"footer","elements":[{"type":"plain_text","text":"Your edits stay in place when the app updates this form."}]}
        ]
    })
}

pub(crate) fn remote_update(state: Signal<ShellState>) {
    let mut updated = view();
    updated["blocks"][1]["element"]["initial_value"] = json!("Remote release name");
    updated["blocks"][2]["element"]["initial_value"] = json!("Remote note");
    super::apply_fixture_update(state, updated);
}

pub(crate) fn submission(
    session: &mut Session,
    state: &Value,
) -> BlockViewResponse<BlockViewSubmission> {
    if session
        .current_form()
        .is_some_and(|form| form.view_id == "V_FORM_REVIEW")
    {
        return BlockViewResponse::Accepted(BlockViewSubmission {
            action: BlockViewResponseAction::Clear,
            toast_message: Some("Release saved".into()),
        });
    }
    session.fixture_attempts += 1;
    if session.fixture_attempts == 1 {
        return BlockViewResponse::Rejected(BlockViewRejection {
            error: "validation_errors".into(),
            message: None,
            errors: [("name".into(), "Choose another release name.".into())].into(),
            view_error: None,
        });
    }
    let name = state
        .pointer("/values/name/value/value")
        .and_then(Value::as_str)
        .unwrap_or("Release");
    BlockViewResponse::Accepted(BlockViewSubmission {
        action: BlockViewResponseAction::Push(json!({
            "id":"V_FORM_REVIEW","type":"modal","app_id":"A_FIXTURE",
            "previous_view_id":"V_FORM_FIXTURE","root_view_id":"V_FORM_FIXTURE",
            "title":{"type":"plain_text","text":"Review release"},
            "submit":{"type":"plain_text","text":"Save release"},
            "close":{"type":"plain_text","text":"Cancel"},
            "blocks":[{"type":"section","block_id":"review","text":{"type":"plain_text","text":format!("Ready to save {name}?")}}]
        })),
        toast_message: None,
    })
}

fn lifecycle_check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_APP_MODAL_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-lifecycle" | "app-modal-lifecycle-narrow")
        )
}

/// Render transport states without creating or sending a request.
pub(crate) fn set_status(mut state: Signal<ShellState>, busy: bool, uncertain: bool) {
    if !lifecycle_check_enabled() {
        return;
    }
    if let Some(session) = state.write().app_modal.as_mut().filter(|s| s.fixture) {
        session.busy = busy;
        session.uncertain = uncertain;
    }
}

/// Exercise a dirty selected option disappearing and returning on a remote edit.
pub(crate) fn change_options(state: Signal<ShellState>, remove_selected: bool) {
    if !lifecycle_check_enabled() {
        return;
    }
    let mut updated = view();
    if remove_selected {
        updated["blocks"][4]["element"]["options"]
            .as_array_mut()
            .unwrap()
            .pop();
    }
    super::apply_fixture_update(state, updated);
}

/// Preserve the correlated modal stack while enabling disconnected responses.
pub(crate) fn enable_responses(shell: &mut ShellState) {
    if !crate::message_buttons::fixture::enabled() || shell.core.transport.is_some() {
        return;
    }
    if let Some(session) = shell.app_modal.as_mut() {
        session.fixture = true;
    }
}
