//! Exact disconnected capability. Synthetic credentials exist only in the
//! request builder, never in ShellState; no fixture can fall back to live work.
use super::*;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
pub(super) const ACCOUNT: &str = "147b5480-9f1a-4443-89da-836472cbb209";
pub(super) const TS: &str = "1719800000.000100";
#[derive(Default)]
struct Record {
    mode: u8,
    archived: bool,
    messages: Vec<Value>,
    requests: Vec<Value>,
    prepared: Vec<Value>,
    release: bool,
}
static RECORD: OnceLock<Mutex<Record>> = OnceLock::new();
static WAKE: OnceLock<tokio::sync::Notify> = OnceLock::new();
pub(in crate::canvas) fn plain_requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-reply" | "canvas-comment-reply-narrow")
        )
}
pub(in crate::canvas) fn rich_requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-rich-reply" | "canvas-comment-rich-reply-narrow")
        )
}
pub(in crate::canvas) fn requested() -> bool {
    plain_requested() || rich_requested() || super::super::annotation::fixture::requested()
}
pub(in crate::canvas) fn initial_root(shell: &ShellState) -> Option<Message> {
    (rich_requested() && eligible(shell)).then(|| root(false))
}
pub(super) fn seed_rich(
    j: &mut Journal,
    scope: &DiskScope,
    ts: &str,
) -> Result<(), super_platinum_core::error::AppError> {
    if !rich_requested() {
        return Ok(());
    }
    let inventory = j.reply_inventory(scope, ts)?;
    if rich_requested()
        && scope.file == "Ffixture"
        && ts == TS
        && inventory.drafts().is_empty()
        && inventory.attempt().is_none()
    {
        // Stable fixture capture: its retired tombstone prevents reseeding a sent
        // or deliberately discarded version on a later inventory load.
        let _=j.save_rich_reply_draft(scope,ts,None,"a615065e-a780-4622-b9b8-9335e84e2edf",vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Review the ","style":{"bold":true}},
            {"type":"link","url":"https://example.invalid/plan","text":"launch plan","style":{"italic":true}},
            {"type":"text","text":" with "},{"type":"user","user_id":"U1"},
            {"type":"text","text":"\nKeep the exact formatting and \u{1f600}."}
        ]}]})])?;
    }
    Ok(())
}
fn eligible(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.canvas.as_ref().is_some_and(|c| c.id == "Ffixture")
}
pub(super) fn path(shell: &ShellState) -> Option<PathBuf> {
    if super::super::annotation::fixture::requested() {
        return super::super::annotation::fixture::path(shell);
    }
    if !eligible(shell)
        || std::env::var("SUPER_PLATINUM_CANVAS_COMMENT_REPLY_CHECK").as_deref() != Ok("1")
    {
        return None;
    }
    let base = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")?);
    let path = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_REPLY_DB")?);
    (base.is_absolute() && base.is_dir() && path == base.join("replies.sqlite")).then_some(path)
}
fn session(shell: &ShellState) -> Result<super_platinum_core::config::WorkspaceSession, Error> {
    if !eligible(shell) {
        return Err(Error::Api("reply_fixture_unavailable".into()));
    }
    Ok(super_platinum_core::config::WorkspaceSession {
        team_id: shell.core.active_team.clone().unwrap(),
        enterprise_id: None,
        user_id: shell.self_account.user_id.clone(),
        name: "Reply fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    })
}
fn wire(request: super_platinum_core::slack::client::PreparedRequest) -> Result<Value, Error> {
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        return Err(Error::Api("fixture_form".into()));
    };
    Ok(
        json!({"method":request.method,"url":request.url,"fields":fields.into_iter().filter(|(k,_)|k!="token").collect::<Vec<_>>()}),
    )
}
fn root(archived: bool) -> Message {
    serde_json::from_value(json!({"type":"message","channel":"Cfixture","ts":TS,"subtype":"document_comment_root","user":"U1","text":"Review this existing Canvas comment thread.","reply_count":1,"document_comment":{"thread_id":"fixture-comment-1","is_archived":archived}})).unwrap()
}
pub(super) fn canonical(shell: &ShellState, binding: &Binding) -> Result<(File, Message), Error> {
    if super::super::annotation::fixture::requested() {
        return super::super::annotation::fixture::reply_canonical(
            shell,
            &binding.owner,
            &binding.root,
        );
    }
    let session = session(shell)?;
    if path(shell).is_none() || !binding.owner.current(shell) {
        return Err(Error::Api("fixture_owner".into()));
    }
    let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
    record.requests.push(wire(api::canvas_info(
        &shell.core.client,
        &session,
        "Ffixture",
    ))?);
    record.requests.push(wire(api::canvas_comment_replies(
        &shell.core.client,
        &session,
        "Ffixture",
        TS,
        None,
    )?)?);
    if record.mode == 4 {
        record.archived = true;
        record.mode = 0
    }
    Ok((super::super::fixture::file(), root(record.archived)))
}
pub(super) async fn execute(
    state: Signal<ShellState>,
    binding: &Binding,
    intent: ReplyIntent,
) -> Result<ReplyAccepted, CanvasEditFailure> {
    if super::super::annotation::fixture::requested() {
        return super::super::annotation::fixture::reply(&state.peek(), &binding.owner, intent);
    }
    let mode = {
        let shell = state.peek();
        let session = session(&shell).map_err(CanvasEditFailure::NotSent)?;
        if path(&shell).is_none() || !binding.owner.current(&shell) {
            return Err(CanvasEditFailure::NotSent(Error::Api(
                "fixture_owner".into(),
            )));
        }
        let request = api::canvas_comment_reply(&shell.core.client, &session, &intent)
            .map_err(CanvasEditFailure::NotSent)?;
        let wire = wire(request).map_err(CanvasEditFailure::NotSent)?;
        let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
        record.prepared.push(wire.clone());
        let mode = std::mem::take(&mut record.mode);
        if mode == 1 {
            return Err(CanvasEditFailure::NotSent(Error::Offline(
                "synthetic before dispatch".into(),
            )));
        }
        record.requests.push(wire);
        mode
    };
    if mode == 3 {
        let wait = async {
            loop {
                let notified = WAKE.get_or_init(Default::default).notified();
                if RECORD.get_or_init(Default::default).lock().unwrap().release {
                    break;
                }
                notified.await;
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), wait)
            .await
            .map_err(|_| {
                CanvasEditFailure::Uncertain(Error::Transport(
                    "synthetic held response expired".into(),
                ))
            })?;
    }
    let raw = {
        let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
        let ts = format!("171981{:04}.000100", record.messages.len() + 1);
        let mut msg = json!({"type":"message","channel":"Cfixture","thread_ts":TS,"ts":ts,"user":intent.record().user(),"text":intent.record().text(),"client_msg_id":intent.record().client_msg_id()});
        if let Some(blocks) = intent.record().blocks() {
            msg["blocks"] = json!(blocks);
        }
        record.messages.push(msg.clone());
        json!({"ok":true,"channel":"Cfixture","ts":ts,"message":msg})
    };
    if mode == 2 {
        return Err(CanvasEditFailure::Uncertain(Error::Transport(
            "synthetic lost response".into(),
        )));
    }
    intent
        .accept(&raw)
        .map_err(|_| CanvasEditFailure::Uncertain(Error::Api("fixture_ack".into())))
}
pub(in crate::canvas) fn response(shell: &ShellState, ticket: &Ticket) -> Result<Value, Error> {
    if super::super::annotation::fixture::requested() {
        return super::super::annotation::fixture::response(shell, ticket);
    }
    let session = session(shell)?;
    let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
    record.requests.push(wire(super::super::request(
        &shell.core.client,
        &session,
        ticket,
    )?)?);
    let mut messages = vec![serde_json::to_value(root(record.archived)).unwrap()];
    if matches!(&ticket.query,Query::Replies(ts) if ts==TS) {
        messages.push(json!({"type":"message","channel":"Cfixture","ts":"1719800001.000100","thread_ts":TS,"text":"Ready for a reply.","user":"U2"}));
        messages.extend(record.messages.clone());
    }
    Ok(json!({"ok":true,"messages":messages}))
}
pub(super) fn mode(mode: u8) {
    let mut record = RECORD.get_or_init(Default::default).lock().unwrap();
    record.mode = mode;
    record.release = false;
    record.archived = false;
}
pub(super) fn release() {
    RECORD.get_or_init(Default::default).lock().unwrap().release = true;
    WAKE.get_or_init(Default::default).notify_waiters();
}
pub(super) fn ledger() -> Value {
    let record = RECORD.get_or_init(Default::default).lock().unwrap();
    json!({"requests":record.requests,"prepared":record.prepared})
}

pub(in crate::canvas) fn install(shell: &mut ShellState) {
    if requested() {
        shell.core.active_account = Some(ACCOUNT.into());
    }
}
