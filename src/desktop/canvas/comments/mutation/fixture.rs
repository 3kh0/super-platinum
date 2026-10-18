//! Private fixture capability. No Session/Transport and no live fallback.
use super::*;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
pub(super) const ACCOUNT: &str = "147b5480-9f1a-4443-89da-836472cbb209";
pub(super) const TS: &str = "1719800000.000100";
#[derive(Default)]
struct Record {
    archived: bool,
    mode: u8,
    requests: Vec<Value>,
    prepared: Vec<Value>,
}
static RECORD: OnceLock<Mutex<Record>> = OnceLock::new();
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-resolve" | "canvas-comment-resolve-narrow")
        )
}
fn eligible(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell
            .canvas
            .as_ref()
            .is_some_and(|file| file.id == "Ffixture")
}
pub(super) fn path(shell: &ShellState) -> Option<PathBuf> {
    if !eligible(shell)
        || std::env::var("SUPER_PLATINUM_CANVAS_COMMENT_RESOLVE_CHECK").as_deref() != Ok("1")
    {
        return None;
    }
    let base = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")?);
    let path = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_COMMENT_DB")?);
    (base.is_absolute() && base.is_dir() && path == base.join("comments.sqlite")).then_some(path)
}
fn session(shell: &ShellState) -> Result<super_platinum_core::config::WorkspaceSession, Error> {
    if !eligible(shell) {
        return Err(Error::Api("comment_fixture_unavailable".into()));
    }
    Ok(super_platinum_core::config::WorkspaceSession {
        team_id: shell
            .core
            .active_team
            .clone()
            .ok_or_else(|| Error::Api("fixture_team".into()))?,
        enterprise_id: None,
        user_id: shell.self_account.user_id.clone(),
        name: "Comment fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    })
}
fn remember(
    record: &mut Record,
    request: super_platinum_core::slack::client::PreparedRequest,
) -> Result<(), Error> {
    let super_platinum_core::slack::client::RequestBody::Form(mut fields) = request.body else {
        return Err(Error::Api("fixture_form".into()));
    };
    fields.retain(|(key, _)| key != "token");
    record
        .requests
        .push(json!({"url":request.url,"method":request.method,"fields":fields}));
    Ok(())
}
fn root(archived: bool) -> Message {
    serde_json::from_value(json!({"type":"message","channel":"Cfixture","ts":TS,"subtype":"document_comment_root","user":"U1","text":"Resolve this review after checking the launch plan.","reply_count":1,"document_comment":{"thread_id":"fixture-comment-1","is_archived":archived}})).unwrap()
}
pub(super) fn canonical(shell: &ShellState, attempt: &Attempt) -> Result<(File, Message), Error> {
    let session = session(shell)?;
    if path(shell).is_none() || !attempt.owner.scope.current(shell) {
        return Err(Error::Api("fixture_owner".into()));
    }
    let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
    remember(
        &mut record,
        api::canvas_info(&shell.core.client, &session, "Ffixture"),
    )?;
    remember(
        &mut record,
        api::canvas_comment_replies(&shell.core.client, &session, "Ffixture", TS, None)?,
    )?;
    if record.mode == 3 {
        record.archived = !record.archived;
        record.mode = 0;
    }
    Ok((super::super::fixture::file(), root(record.archived)))
}
pub(super) fn execute(
    shell: &ShellState,
    attempt: &Attempt,
) -> Result<ArchiveAccepted, CanvasEditFailure> {
    let session = session(shell).map_err(CanvasEditFailure::NotSent)?;
    if path(shell).is_none() {
        return Err(CanvasEditFailure::NotSent(Error::Api(
            "fixture_binding".into(),
        )));
    }
    let request = api::canvas_comment_archive(&shell.core.client, &session, &attempt.intent)
        .map_err(CanvasEditFailure::NotSent)?;
    let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
    let super_platinum_core::slack::client::RequestBody::Form(fields) = &request.body else {
        unreachable!()
    };
    record.prepared.push(json!({"url":request.url,"method":request.method,"fields":fields.iter().filter(|(key,_)|key!="token").cloned().collect::<Vec<_>>()}));
    if record.mode == 1 {
        record.mode = 0;
        return Err(CanvasEditFailure::NotSent(Error::Offline(
            "synthetic before dispatch".into(),
        )));
    }
    remember(&mut record, request).map_err(CanvasEditFailure::NotSent)?;
    record.archived = attempt.intent.desired();
    if record.mode == 2 {
        record.mode = 0;
        return Err(CanvasEditFailure::Uncertain(Error::Transport(
            "synthetic lost response".into(),
        )));
    }
    attempt
        .intent
        .clone()
        .accept(&json!({"ok":true,"message":root(record.archived)}))
        .map_err(|_| CanvasEditFailure::Uncertain(Error::Api("fixture_ack".into())))
}
pub(in crate::canvas) fn response(shell: &ShellState, ticket: &Ticket) -> Result<Value, Error> {
    let session = session(shell)?;
    let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
    remember(
        &mut record,
        super::super::request(&shell.core.client, &session, ticket)?,
    )?;
    let messages = match &ticket.query {
        Query::Stream(Filter::Recent) if record.archived => vec![],
        Query::Stream(Filter::Resolved) if !record.archived => vec![],
        Query::Stream(Filter::Unread) => vec![],
        Query::Stream(_) => vec![serde_json::to_value(root(record.archived)).unwrap()],
        Query::Replies(ts) if ts == TS => vec![
            serde_json::to_value(root(record.archived)).unwrap(),
            json!({"type":"message","channel":"Cfixture","ts":"1719800001.000100","thread_ts":TS,"user":"U2","text":"The final review is complete."}),
        ],
        _ => return Err(Error::Api("fixture_query".into())),
    };
    Ok(json!({"ok":true,"messages":messages}))
}
pub(in crate::canvas) fn mode(value: u8) {
    RECORD.get_or_init(Default::default).lock().unwrap().mode = value;
}
pub(in crate::canvas) fn ledger() -> Vec<Value> {
    RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .requests
        .clone()
}

pub(crate) fn install(shell: &mut ShellState) {
    if requested() {
        shell.core.active_account = Some(ACCOUNT.into());
    }
}

pub(in crate::canvas) fn prepared() -> Vec<Value> {
    RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .prepared
        .clone()
}
