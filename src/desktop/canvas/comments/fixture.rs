//! Exact named fixture; requests are built normally but never executed.
use super::*;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
pub(crate) mod attribution;
pub(in crate::canvas) mod canonical_files;
pub(super) mod file_fallback;
mod root_bodies;
pub(super) mod special;
#[derive(Default)]
struct Record {
    requests: Vec<Value>,
    responses: Vec<Value>,
    fail_next: bool,
    special_retained: Value,
    file_fallback_retained: Value,
}
static RECORD: OnceLock<Mutex<Record>> = OnceLock::new();
pub(in crate::canvas) fn requested() -> bool {
    annotation::fixture::requested()
        || attribution::requested()
        || special::requested()
        || file_fallback::requested()
        || canonical_files::requested()
        || reply::fixture::requested()
        || mutation::fixture::requested()
        || crate::fixture::is_fixture()
            && matches!(
                std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
                Ok("canvas-comments" | "canvas-comments-narrow")
            )
}
fn eligible(shell: &ShellState) -> bool {
    if attribution::requested() {
        return attribution::enabled(shell);
    }
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.canvas.as_ref().is_some_and(|v| v.id == "Ffixture")
}
pub(super) fn ledger() -> Vec<Value> {
    RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .requests
        .clone()
}
pub(super) fn responses() -> Vec<Value> {
    RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .responses
        .clone()
}
pub(super) fn fail_next() {
    RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .fail_next = true;
}
fn root(ts: &str, text: &str) -> Value {
    json!({"type":"message","subtype":"document_comment_root","ts":ts,"user":"U1","text":text,"reply_count":2,"document_comment":{"thread_id":format!("quip-{ts}")}})
}
fn reply(ts: &str, parent: &str, text: &str) -> Value {
    json!({"type":"message","ts":ts,"thread_ts":parent,"user":"U2","text":text})
}
pub(super) fn response(shell: &ShellState, ticket: &Ticket) -> Result<Value, Error> {
    if annotation::fixture::requested() {
        return annotation::fixture::response(shell, ticket);
    }
    if reply::fixture::requested() && !attribution::requested() && !canonical_files::requested() {
        return reply::fixture::response(shell, ticket);
    }
    if mutation::fixture::requested() {
        return mutation::fixture::response(shell, ticket);
    }
    if !eligible(shell) || !ticket.owner.current(shell) {
        return Err(Error::Api("canvas_comment_fixture_unavailable".into()));
    }
    let session = super_platinum_core::config::WorkspaceSession {
        team_id: ticket.owner.scope.team.clone(),
        enterprise_id: None,
        user_id: shell.self_account.user_id.clone(),
        name: "Comment fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let request = super::request(&shell.core.client, &session, ticket)?;
    let super_platinum_core::slack::client::RequestBody::Form(mut fields) = request.body else {
        return Err(Error::Api("fixture_request_not_form".into()));
    };
    fields.retain(|(key, _)| key != "token");
    let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
    record
        .requests
        .push(json!({"url":request.url,"method":request.method,"fields":fields}));
    if std::mem::take(&mut record.fail_next) {
        record.responses.push(Value::Null);
        return Err(Error::Api("fixture_read_failed".into()));
    }
    let r = "1719800000.000100";
    let response = if attribution::requested() {
        attribution::response(ticket)?
    } else if special::requested() {
        special::response(ticket)?
    } else if file_fallback::requested() {
        file_fallback::response(ticket)?
    } else if canonical_files::requested() {
        canonical_files::response(ticket)?
    } else {
        match (&ticket.query, ticket.cursor.as_deref()) {
            (Query::Stream(Filter::Recent), None) => {
                json!({"ok":true,"has_more":true,"response_metadata":{"next_cursor":"stream-next"},"messages":[root_bodies::rich(),reply("1719800001.000100",r,"The keyboard review is complete.\nReady for the final pass."),root_bodies::fallback()]})
            }
            (Query::Stream(Filter::Recent), Some("stream-next")) => {
                json!({"ok":true,"has_more":false,"messages":[root_bodies::markdown()]})
            }
            (Query::Stream(Filter::Resolved), None) => {
                json!({"ok":true,"messages":[root_bodies::literal(),root_bodies::hidden()]})
            }
            (Query::Stream(Filter::Unread), None) => json!({"ok":true,"messages":[]}),
            (Query::Replies(id), None) if id == r => {
                json!({"ok":true,"has_more":true,"response_metadata":{"next_cursor":"reply-next"},"messages":[root_bodies::rich(),reply("1719800001.000100",r,"The keyboard review is complete.\nReady for the final pass.")]})
            }
            (Query::Replies(id), Some("reply-next")) if id == r => {
                json!({"ok":true,"messages":[reply("1719800002.000100",r,"Unicode remains readable: \u{65e5}\u{672c}\u{8a9e} \u{1f98a} e\u{301}.")]})
            }
            _ => return Err(Error::Api("fixture_unexpected_comment_query".into())),
        }
    };
    record.responses.push(response.clone());
    Ok(response)
}

pub(super) fn file() -> super_platinum_core::slack::models::File {
    if annotation::fixture::requested() {
        return annotation::fixture::file();
    }
    serde_json::from_value(
        serde_json::json!({"id":"Ffixture","filetype":"quip","access":"write","is_locked":false}),
    )
    .unwrap()
}
