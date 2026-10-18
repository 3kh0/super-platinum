//! Disconnected reminder backend. The normal/narrow fixtures open the actual
//! dialog; writes remain held until an explicit fixture completion.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{api, client::RequestBody},
};
#[derive(Clone, Default)]
pub(super) struct State {
    saved: Option<SavedItem>,
    pending: Option<Attempt>,
    attempts: usize,
    reads: usize,
    #[cfg(test)]
    read_events: Vec<Value>,
    completed: Vec<Value>,
    forms: Vec<Value>,
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-todo-reminder" | "slack-list-todo-reminder-narrow"
    ));
    super::super::fixture::install(shell, "slack-list-incomplete");
    super::super::fixture::enable_reminders(shell);
    super::super::fixture::refresh(shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout, row) = (
        view.generation,
        view.layout.generation,
        view.rows[0].instance_id.clone(),
    );
    assert!(record_detail::open_shell(shell, &row, visit, layout));
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    let origin = begin(shell, detail.generation).unwrap();
    load(shell, origin);
}
pub(super) fn load(shell: &mut ShellState, origin: Origin) {
    if !super::super::fixture::enabled(shell) {
        return;
    }
    if !origin.read_current(shell) {
        super::loaded(shell, &origin, Err(Error::TransportNotConfigured));
        return;
    }
    let loaded = super::super::fixture::deletion_loaded(shell).unwrap();
    let item = shell.slack_lists.reminder.fixture.saved.clone();
    shell.slack_lists.reminder.fixture.reads += 1;
    #[cfg(test)]
    record_read(shell, "load", &origin, None);
    super::loaded(shell, &origin, Ok((loaded, item)));
}
pub(super) fn dispatch(shell: &mut ShellState, attempt: Attempt) {
    if !super::super::fixture::enabled(shell) {
        return;
    }
    let loaded = super::super::fixture::deletion_loaded(shell).unwrap();
    let item = shell.slack_lists.reminder.fixture.saved.clone();
    shell.slack_lists.reminder.fixture.reads += 1;
    #[cfg(test)]
    record_read(shell, "preflight", &attempt.origin, None);
    if !validate(shell, &attempt, &loaded, item.as_ref()) {
        rejected(
            shell,
            &attempt,
            "This reminder was cancelled or changed.",
            None,
        );
        return;
    }
    let session = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::list_reminder_change(&shell.core.client, &session, &attempt.intent);
    let RequestBody::Form(fields) = request.body else {
        panic!("reminder form")
    };
    let expected = attempt
        .intent
        .fields()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect::<Vec<_>>();
    assert_eq!(&fields[1..], expected.as_slice());
    assert_eq!(fields[0], ("token".into(), "fixture-not-a-token".into()));
    let fixture = &mut shell.slack_lists.reminder.fixture;
    fixture
        .forms
        .push(json!({"method":attempt.intent.method(),"fields":expected}));
    fixture.attempts += 1;
    fixture.pending = Some(attempt);
}
/// Complete the same dispatched attempt, optionally losing the response. The
/// canonical observation is independent and never changes the displayed copy.
pub(crate) fn complete(shell: &mut ShellState, lost_reply: bool, apply: bool) -> bool {
    if !super::super::fixture::enabled(shell) {
        return false;
    }
    let Some(attempt) = shell.slack_lists.reminder.fixture.pending.take() else {
        return false;
    };
    if apply {
        let fields = attempt.intent.fields();
        let due = fields
            .iter()
            .find(|(k, _)| *k == "date_due")
            .map(|(_, v)| v.parse::<i64>().unwrap());
        let mark = fields
            .iter()
            .find(|(k, _)| *k == "mark")
            .map(|(_, v)| v.as_str());
        let saved = if attempt.intent.method() == "saved.delete" {
            None
        } else {
            let mut item = shell
                .slack_lists
                .reminder
                .fixture
                .saved
                .clone()
                .unwrap_or_else(|| SavedItem {
                    item_type: "list_record".into(),
                    item_id: attempt.origin.list.clone(),
                    ts: attempt.origin.row.clone(),
                    date_created: 1700000000,
                    state: "in_progress".into(),
                    todo_state: "saved".into(),
                    ..Default::default()
                });
            item.date_due = due.unwrap();
            item.date_updated += 1;
            if mark == Some("unarchived") {
                item.is_archived = false;
            }
            if mark == Some("uncompleted") {
                item.date_completed = 0;
            }
            Some(item)
        };
        super::super::fixture::observe_reminder(shell, &attempt.origin.row, saved.as_ref());
        shell.slack_lists.reminder.fixture.saved = saved;
    }
    let outcome = if lost_reply {
        Err(ListReminderFailure::Unconfirmed(Error::Transport(
            "fixture lost reply".into(),
        )))
    } else {
        Ok(json!({"ok":true}))
    };
    shell.slack_lists.reminder.fixture.completed.push(json!({"method":attempt.intent.method(),"fields":attempt.intent.fields(),"lostReply":lost_reply,"applied":apply}));
    finish(shell, &attempt, outcome)
}
pub(super) fn check(shell: &mut ShellState, read: Read) {
    if !super::super::fixture::enabled(shell) {
        return;
    }
    if !read.current(shell) {
        finish_check(shell, &read, Err(Error::TransportNotConfigured));
        return;
    }
    shell.slack_lists.reminder.fixture.reads += 1;
    #[cfg(test)]
    {
        let origin = shell.slack_lists.reminder.receipts[&read.key]
            .attempt
            .origin
            .clone();
        shell.slack_lists.reminder.fixture.read_events.push(json!({
            "stage": "check", "key": read.key, "row": origin.row,
            "serial": read.serial, "visit": read.visit, "token": read.token
        }));
    }
    let loaded = super::super::fixture::deletion_loaded(shell).unwrap();
    if finish_check(shell, &read, Ok(loaded)) {
        super::super::fixture::refresh(shell);
    }
}

#[cfg(test)]
pub(super) fn attempts(shell: &ShellState) -> usize {
    shell.slack_lists.reminder.fixture.attempts
}
#[cfg(test)]
fn record_read(shell: &mut ShellState, stage: &str, origin: &Origin, token: Option<u64>) {
    shell.slack_lists.reminder.fixture.read_events.push(json!({
        "stage": stage, "key": origin.key(), "row": origin.row,
        "serial": origin.serial, "visit": origin.visit, "token": token
    }));
}
#[cfg(test)]
pub(super) fn read_events(shell: &ShellState) -> &[Value] {
    &shell.slack_lists.reminder.fixture.read_events
}
#[cfg(test)]
pub(super) fn reads(shell: &ShellState) -> usize {
    shell.slack_lists.reminder.fixture.reads
}
#[cfg(test)]
pub(super) fn forms(shell: &ShellState) -> &[Value] {
    &shell.slack_lists.reminder.fixture.forms
}

/// Diagnostic evidence from the actual held fixture and production reducer.
pub(crate) fn diagnostic(shell: &ShellState) -> Result<Value, String> {
    if !crate::slack_lists::check::todo::enabled()
        || !super::super::fixture::enabled(shell)
        || shell.core.transport.is_some()
        || shell.core.session.is_some()
    {
        return Err("Disconnected To-do diagnostic required".into());
    }
    let f = &shell.slack_lists.reminder.fixture;
    let p = super::presentation(shell);
    let receipts=shell.slack_lists.reminder.receipts.values().map(|r|json!({
        "serial":r.attempt.origin.serial,"list":r.attempt.origin.list,"row":r.attempt.origin.row,
        "phase":match r.phase {Phase::Preparing=>"Preparing",Phase::Sending=>"Sending",Phase::Uncertain=>"Uncertain"},
        "checking":r.checking,"fields":r.attempt.intent.fields(),"method":r.attempt.intent.method()})).collect::<Vec<_>>();
    Ok(
        json!({"forms":f.forms,"completions":f.completed,"reads":f.reads,"pending":f.pending.is_some(),
        "saved":f.saved,"receipts":receipts,"backend":super::super::fixture::deletion_loaded(shell).ok_or("Missing reminder backend")?.records.raw,
        "presentation":p.map(|p|json!({"serial":p.generation,"due":p.due,"editable":p.editable,"ready":p.ready,"loading":p.loading,"pending":p.pending,"canCheck":p.can_check,"canClear":p.can_clear,"message":p.message}))}),
    )
}
