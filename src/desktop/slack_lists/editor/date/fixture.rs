//! Named disconnected calendar-date editor on an account-scoped personal List.
use super::*;
use serde_json::json;
pub(crate) const NAMES: [&str; 4] = [
    "slack-list-task-date",
    "slack-list-task-date-narrow",
    "slack-list-task-reminder-route",
    "slack-list-task-reminder-route-narrow",
];
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(NAMES.contains(&name));
    crate::slack_lists::fixture::install(shell, name);
    seed(shell, "todo_due_date", true);
    crate::slack_lists::fixture::task_date::install(shell);
    if name.starts_with("slack-list-task-reminder-route") {
        personal_route(shell);
    }
}
pub(crate) fn personal_route(shell: &mut ShellState) {
    let generation = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation;
    super::super::cancel_shell(shell, generation);
    crate::slack_lists::fixture::task_date_mode(shell, true, true);
}
pub(crate) fn seed(shell: &mut ShellState, kind: &str, personal: bool) {
    assert!(matches!(kind, "date" | "due_date" | "todo_due_date"));
    let id = shell.slack_lists.view.as_ref().unwrap().id.clone();
    let document = json!({"id":id,"mode":"list","title":"Personal task dates","access":"write","is_locked":false,"created":1,"user":"U1","list_metadata":{"schema":[
        {"id":"C_NAME","key":"name","name":"Item","type":"rich_text","is_primary_column":true},
        {"id":"C_DATE","key":"due","name":"Due date","type":kind,"options":{"dateFormat":"MM/DD/YYYY"}}
    ],"views":[],"opaque":{"retain":true}}});
    let records = json!({"ok":true,"records":[{"id":"R_DATE","list_id":id,"date_created":1,"created_by":"U1","fields":[
        {"key":"name","rich_text":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Review the launch plan"}]}]}]},
        {"key":"due","date":["2026-10-17"],"timestamp":[-1],"opaque":{"retain":true}}
    ]}],"response_metadata":{"next_cursor":"retained-date-page-cursor"},"opaque":17});
    let policy = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{"task_lists":if personal {serde_json::to_string(&vec![&id]).unwrap()} else {"[]".into()},"assigned_to_list":""}})).unwrap();
    shell.slack_lists.view.as_mut().unwrap().editor = None;
    crate::slack_lists::fixture::replace_task_date_seed(shell, document, records, policy);
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    super::super::begin_shell(shell, "R_DATE", "C_DATE", visit).unwrap();
}
