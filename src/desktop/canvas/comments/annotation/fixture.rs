//! Named disconnected backend, retained by the shell. Every write builds its
//! ordinary request and consumes its ordinary typed proof. No Transport exists.
use super::*;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Mutex};
use super_platinum_core::{
    canvas::{
        comments::{
            bootstrap::BootstrapIntent,
            reply::{ReplyAccepted, ReplyIntent},
        },
        edit::annotation::PreparedAnnotation,
        wire::{EditAck, EncodedField as F, RawMessage, Section},
    },
    config::WorkspaceSession,
    slack::{
        client::{PreparedRequest, RequestBody},
        models::File,
    },
};
mod blockquote;
mod columns;
pub(in crate::canvas) mod heading;
mod image;
mod list;
mod section;
mod table;
mod table_bodies;
pub(in crate::canvas) mod underline;
mod whole;
pub(in crate::canvas) const ACCOUNT: &str = "147b5480-9f1a-4443-89da-836472cbb209";
#[derive(Default)]
pub(super) struct State {
    record: Mutex<Record>,
}
#[derive(Default)]
struct Record {
    canonical: Option<CanvasCollabSnapshot>,
    roots: Vec<Value>,
    replies: Vec<Value>,
    requests: Vec<Value>,
    prepared: Vec<Value>,
    // Explicit fixture initialization receipts, never counted as driven writes.
    seed_requests: Vec<Value>,
    underline: underline::Control,
    heading: heading::Control,
    // Test-only controls never produce an ambient/network capability.
    #[cfg(test)]
    mode: u8,
}
pub(in crate::canvas) fn requested() -> bool {
    heading::requested()
        || underline::requested()
        || crate::fixture::is_fixture()
            && matches!(
                std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
                Ok("canvas-comment-annotation"
                    | "canvas-comment-annotation-narrow"
                    | "canvas-comment-inline"
                    | "canvas-comment-inline-narrow"
                    | "canvas-comment-existing-inline"
                    | "canvas-comment-existing-inline-narrow"
                    | "canvas-comment-archived-inline"
                    | "canvas-comment-archived-inline-narrow"
                    | "canvas-comment-hidden-inline"
                    | "canvas-comment-hidden-inline-narrow"
                    | "canvas-comment-section"
                    | "canvas-comment-section-narrow"
                    | "canvas-comment-section-inline"
                    | "canvas-comment-section-inline-narrow"
                    | "canvas-comment-list"
                    | "canvas-comment-list-narrow"
                    | "canvas-comment-image"
                    | "canvas-comment-image-narrow"
                    | "canvas-comment-table"
                    | "canvas-comment-table-narrow"
                    | "canvas-comment-table-bodies"
                    | "canvas-comment-table-bodies-narrow"
                    | "canvas-comment-blockquote"
                    | "canvas-comment-blockquote-narrow"
                    | "canvas-comment-flexbox"
                    | "canvas-comment-flexbox-narrow"
                    | "canvas-comment-callout"
                    | "canvas-comment-callout-narrow")
            )
}
pub(in crate::canvas) fn inline_requested() -> bool {
    if whole::requested() || underline::requested() || heading::requested() {
        return true;
    }
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-inline"
                | "canvas-comment-inline-narrow"
                | "canvas-comment-existing-inline"
                | "canvas-comment-existing-inline-narrow"
                | "canvas-comment-archived-inline"
                | "canvas-comment-archived-inline-narrow"
                | "canvas-comment-hidden-inline"
                | "canvas-comment-hidden-inline-narrow")
        )
}
fn archived_requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-archived-inline" | "canvas-comment-archived-inline-narrow")
        )
}
fn hidden_requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-hidden-inline" | "canvas-comment-hidden-inline-narrow")
        )
}
fn existing_requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-existing-inline"
                | "canvas-comment-existing-inline-narrow"
                | "canvas-comment-archived-inline"
                | "canvas-comment-archived-inline-narrow"
                | "canvas-comment-hidden-inline"
                | "canvas-comment-hidden-inline-narrow")
        )
}
fn eligible(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.core.active_team.as_deref() == Some("T1")
        && shell.self_account.user_id == "U1"
        && shell.canvas.as_ref().is_some_and(|v| v.id == "Ffixture")
        && shell.canvas_checklist.comment_annotations.fixture.is_some()
}
pub(in crate::canvas) fn path(shell: &ShellState) -> Option<PathBuf> {
    if !eligible(shell)
        || std::env::var("SUPER_PLATINUM_CANVAS_ANNOTATION_CHECK").as_deref() != Ok("1")
    {
        return None;
    }
    let base = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")?);
    let path = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_ANNOTATION_DB")?);
    (base.is_absolute() && base.is_dir() && path == base.join("annotations.sqlite")).then_some(path)
}
fn session(shell: &ShellState, owner: &Owner) -> Result<WorkspaceSession, Error> {
    if path(shell).is_none() || !owner.current(shell) {
        return Err(failure());
    }
    Ok(WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Annotation fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    })
}
fn failure() -> Error {
    Error::Api("annotation_fixture_unavailable".into())
}
fn capability(shell: &ShellState) -> Result<Arc<State>, Error> {
    if !eligible(shell) {
        return Err(failure());
    }
    shell
        .canvas_checklist
        .comment_annotations
        .fixture
        .clone()
        .ok_or_else(failure)
}
fn wire(request: PreparedRequest) -> Result<Value, Error> {
    let RequestBody::Form(fields) = request.body else {
        return Err(failure());
    };
    Ok(json!({"method":request.method,"url":request.url,
        "fields":fields.into_iter().filter(|(k,_)|k!="token").collect::<Vec<_>>()}))
}
fn initial() -> CanvasCollabSnapshot {
    let mut s = (*crate::canvas::text::structure_fixture::snapshot()).clone();
    s.file.extra.insert("access".into(), json!("comment"));
    s.thread = s
        .thread
        .replace_fields(23, &[F::uint(23, 5).unwrap()])
        .unwrap();
    if heading::requested() {
        heading::snapshot(s)
    } else if underline::requested() {
        underline::snapshot(s)
    } else if inline_requested() {
        inline_snapshot(s)
    } else {
        s
    }
}
fn inline_snapshot(mut s: CanvasCollabSnapshot) -> CanvasCollabSnapshot {
    let item = s
        .sections
        .iter_mut()
        .find(|s| s.id() == "paragraph")
        .unwrap();
    let content = item.raw().message(12).unwrap().unwrap();
    let text = content
        .message(1)
        .unwrap()
        .unwrap()
        .replace_fields(
            1,
            &[F::bytes(
                1,
                b"<b>A&amp;\xF0\x9F\x98\x80B</b><br><br><i>Second line</i>",
            )
            .unwrap()],
        )
        .unwrap();
    *item = Section::from_raw(
        item.raw()
            .remove(38)
            .unwrap()
            .replace_fields(
                12,
                &[F::message(
                    12,
                    &content
                        .replace_fields(1, &[F::message(1, &text).unwrap()])
                        .unwrap(),
                )
                .unwrap()],
            )
            .unwrap(),
    )
    .unwrap();
    s
}
// A disconnected pre-existing thread fixture. Its seed forms remain separated
// from user-driven requests; no network or saved account is available.
fn existing_seed(snapshot: CanvasCollabSnapshot) -> Record {
    existing_seed_at(snapshot, "paragraph", 1, 4)
}
fn existing_seed_at(snapshot: CanvasCollabSnapshot, id: &str, start: usize, end: usize) -> Record {
    let now = crate::canvas::checklist::save::now_usec().unwrap();
    let client = super_platinum_core::slack::client::SlackClient::default();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Annotation fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let p = AnnotationIntent::capture_range(&snapshot, id, start, end)
        .unwrap()
        .prepare_at(&snapshot, now)
        .unwrap();
    let old = p.control_id().to_owned();
    let request = wire(api::canvas_annotation(&client, &session, &snapshot, &p).unwrap()).unwrap();
    let mut record = Record {
        canonical: Some(snapshot.clone()),
        ..Default::default()
    };
    let accepted = complete_anchor(&mut record, &snapshot, p, request).unwrap();
    let verified = accepted
        .confirm(record.canonical.as_ref().unwrap())
        .unwrap();
    let intent = BootstrapIntent::capture(verified, 1_720_000_000_001).unwrap();
    let request = wire(api::canvas_comment_bootstrap(&client, &session, &intent).unwrap()).unwrap();
    complete_bootstrap(&mut record, intent, request).unwrap();
    // Explicit synthetic fixture history: one existing reply makes the retained
    // metadata visible independently of hideStubAnnotations and reaction state.
    let root = &mut record.roots[0];
    root["reply_count"] = json!(1);
    let ts = root["ts"].as_str().unwrap().to_owned();
    record.replies.push(json!({"type":"message","channel":"Cfixture","ts":"1720000002.000200","thread_ts":ts,"text":"Existing comment","user":"U1"}));
    let meta = RawMessage::from_fields(&[
        F::bytes(1, old.as_bytes()).unwrap(),
        F::uint(2, 1).unwrap(),
        F::uint(4, 1).unwrap(),
        F::uint(6, 0).unwrap(),
        F::uint(7, now).unwrap(),
        F::uint(8, 0).unwrap(),
    ])
    .unwrap();
    let container = RawMessage::from_fields(&[F::message(1, &meta).unwrap()]).unwrap();
    let canonical = record.canonical.as_mut().unwrap();
    canonical.document = canonical
        .document
        .replace_fields(16, &[F::message(16, &container).unwrap()])
        .unwrap();
    record.seed_requests = std::mem::take(&mut record.requests);
    record.prepared.clear();
    record
}
// The archived predecessor is fixture setup only; no Resolve request is
// fabricated or counted as a driven write, and its existing thread is retained.
fn archived_seed(snapshot: CanvasCollabSnapshot) -> Record {
    let mut record = existing_seed(snapshot);
    let canonical = record.canonical.as_mut().unwrap();
    let container = canonical.document.message(16).unwrap().unwrap();
    let entries = container
        .messages(1)
        .unwrap()
        .iter()
        .map(|meta| {
            F::message(
                1,
                &meta.replace_fields(8, &[F::uint(8, 1).unwrap()]).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    canonical.document = canonical
        .document
        .replace_fields(
            16,
            &[F::message(16, &RawMessage::from_fields(&entries).unwrap()).unwrap()],
        )
        .unwrap();
    record
}
// Explicit zero-reply fixture history. The existing root/control remains real
// fixture output; only its synthetic metadata is a hidden-stub setup state.
fn hidden_seed(snapshot: CanvasCollabSnapshot) -> Record {
    let mut record = existing_seed(snapshot);
    record.roots[0]["reply_count"] = json!(0);
    record.replies.clear();
    let canonical = record.canonical.as_mut().unwrap();
    let entries = canonical
        .document
        .message(16)
        .unwrap()
        .unwrap()
        .messages(1)
        .unwrap()
        .iter()
        .map(|meta| {
            F::message(
                1,
                &meta.replace_fields(2, &[F::uint(2, 0).unwrap()]).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    canonical.document = canonical
        .document
        .replace_fields(
            16,
            &[F::message(16, &RawMessage::from_fields(&entries).unwrap()).unwrap()],
        )
        .unwrap();
    record
}
pub(in crate::canvas) fn refresh_reader(
    shell: &mut ShellState,
    reader: &mut crate::canvas::Reader,
) {
    if !eligible(shell) {
        return;
    }
    if crate::canvas::checklist::presentation(shell).is_none() {
        crate::canvas::checklist::open_visit(shell);
    }
    let Some(owner) = Owner::capture(shell) else {
        return;
    };
    let Ok(snapshot) = canonical(shell, &owner) else {
        return;
    };
    let Some(read) = crate::canvas::checklist::begin_read(shell) else {
        return;
    };
    reader.document = Some(Arc::new(
        heading::document(&snapshot)
            .or_else(|| columns::document(&snapshot))
            .or_else(|| blockquote::document(&snapshot))
            .or_else(|| table_bodies::document(&snapshot))
            .or_else(|| table::document(&snapshot))
            .or_else(|| image::document(&snapshot))
            .or_else(|| list::document(&snapshot))
            .unwrap_or_else(|| crate::canvas::text::structure_fixture::document(&snapshot)),
    ));
    reader.loading = false;
    crate::canvas::checklist::finish_read(shell, &read, Ok(Arc::new(snapshot)));
    crate::canvas::checklist::reader::update_mapping(shell, reader);
}

pub(in crate::canvas) fn install(shell: &mut ShellState) {
    if !requested() || shell.core.session.is_some() || shell.core.transport.is_some() {
        return;
    }
    install_disconnected_owner(shell, &initial());
}
fn install_disconnected_owner(shell: &mut ShellState, snapshot: &CanvasCollabSnapshot) -> bool {
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_team.as_deref() != Some(snapshot.init.workspace_id())
    {
        return false;
    }
    let Some(user) = snapshot.init.options().string(33).ok().flatten() else {
        return false;
    };
    let Some(workspace) = shell.core.workspaces.get_mut(snapshot.init.workspace_id()) else {
        return false;
    };
    shell.core.active_account = Some(ACCOUNT.into());
    workspace.self_user_id = user.into();
    shell.self_account.user_id = user.into();
    let record = if heading::requested() {
        heading::seed(snapshot.clone())
    } else if underline::requested() {
        underline::seed(snapshot.clone())
    } else if columns::requested() {
        columns::seed(snapshot.clone())
    } else if blockquote::requested() {
        blockquote::seed(snapshot.clone())
    } else if table_bodies::requested() {
        table_bodies::seed(snapshot.clone())
    } else if table::requested() {
        table::seed(snapshot.clone())
    } else if image::requested() {
        image::seed(snapshot.clone())
    } else if list::requested() {
        list::seed(snapshot.clone(), 7)
    } else if whole::requested() {
        whole::seed(snapshot.clone())
    } else if section::requested() {
        section::seed(snapshot.clone())
    } else if hidden_requested() {
        hidden_seed(snapshot.clone())
    } else if archived_requested() {
        archived_seed(snapshot.clone())
    } else if existing_requested() {
        existing_seed(snapshot.clone())
    } else {
        Record::default()
    };
    if table_bodies::requested() || blockquote::requested() || columns::requested() {
        table_bodies::preload(shell, record.canonical.as_ref().unwrap());
    }
    if image::requested() {
        image::preload(shell, record.canonical.as_ref().unwrap());
    }
    shell.canvas_checklist.comment_annotations.fixture = Some(Arc::new(State {
        record: Mutex::new(record),
    }));
    true
}
pub(super) fn canonical(shell: &ShellState, owner: &Owner) -> Result<CanvasCollabSnapshot, Error> {
    let _ = session(shell, owner)?;
    let capability = capability(shell)?;
    let mut r = capability.record.lock().map_err(|_| failure())?;
    r.requests
        .push(json!({"observation":"canonical_fixture_snapshot","file":"Ffixture"}));
    Ok(r.canonical.get_or_insert_with(initial).clone())
}
pub(super) fn anchor(
    shell: &ShellState,
    owner: &Owner,
    fresh: &CanvasCollabSnapshot,
    p: PreparedAnnotation,
) -> Result<AcknowledgedAnnotation, CanvasEditFailure> {
    let session = session(shell, owner).map_err(CanvasEditFailure::NotSent)?;
    let request = api::canvas_annotation(&shell.core.client, &session, fresh, &p)
        .and_then(wire)
        .map_err(CanvasEditFailure::NotSent)?;
    let capability = capability(shell).map_err(CanvasEditFailure::NotSent)?;
    let mut r = capability
        .record
        .lock()
        .map_err(|_| CanvasEditFailure::NotSent(failure()))?;
    complete_anchor(&mut r, fresh, p, request)
}
fn complete_anchor(
    r: &mut Record,
    fresh: &CanvasCollabSnapshot,
    p: PreparedAnnotation,
    request: Value,
) -> Result<AcknowledgedAnnotation, CanvasEditFailure> {
    let current = r.canonical.get_or_insert_with(initial);
    // Same exact current fixture revision as the real builder just validated.
    if current.sections != fresh.sections {
        return Err(CanvasEditFailure::NotSent(failure()));
    }
    let sections = section::responses(&p, current)?;
    r.prepared.push(request.clone());
    #[cfg(test)]
    if r.mode == 1 {
        r.mode = 0;
        return Err(CanvasEditFailure::NotSent(Error::Offline(
            "synthetic before handoff".into(),
        )));
    }
    r.requests.push(request);
    let mut data_fields = Vec::new();
    for section in sections {
        let canonical = r.canonical.as_mut().unwrap();
        if let Some(old) = canonical
            .sections
            .iter_mut()
            .find(|s| s.id() == section.id())
        {
            *old = section.clone();
        } else {
            canonical.sections.push(section.clone());
        }
        data_fields.push(F::message(1, section.raw()).unwrap());
    }
    let data = RawMessage::from_fields(&data_fields).unwrap();
    let ack = EditAck::decode(
        RawMessage::from_fields(&[F::uint(1, 1).unwrap(), F::message(2, &data).unwrap()])
            .unwrap()
            .raw(),
    )
    .unwrap();
    #[cfg(test)]
    if r.mode == 2 {
        r.mode = 0;
        return Err(CanvasEditFailure::Uncertain(Error::Transport(
            "synthetic lost anchor reply".into(),
        )));
    }
    if heading::requested() {
        heading::ack(r, &ack);
    }
    if underline::requested() {
        underline::ack(r, &ack)?;
    }
    p.validate_ack(&ack)
        .map_err(|_| CanvasEditFailure::Uncertain(failure()))
}
pub(super) fn bootstrap(
    shell: &ShellState,
    owner: &Owner,
    intent: BootstrapIntent,
) -> Result<BootstrapAccepted, CanvasEditFailure> {
    let session = session(shell, owner).map_err(CanvasEditFailure::NotSent)?;
    let request = api::canvas_comment_bootstrap(&shell.core.client, &session, &intent)
        .and_then(wire)
        .map_err(CanvasEditFailure::NotSent)?;
    let capability = capability(shell).map_err(CanvasEditFailure::NotSent)?;
    let mut r = capability
        .record
        .lock()
        .map_err(|_| CanvasEditFailure::NotSent(failure()))?;
    complete_bootstrap(&mut r, intent, request)
}
fn complete_bootstrap(
    r: &mut Record,
    intent: BootstrapIntent,
    request: Value,
) -> Result<BootstrapAccepted, CanvasEditFailure> {
    r.prepared.push(request.clone());
    #[cfg(test)]
    if r.mode == 3 {
        r.mode = 0;
        return Err(CanvasEditFailure::NotSent(Error::Offline(
            "synthetic before root handoff".into(),
        )));
    }
    r.requests.push(request);
    let control = intent.record().annotation().control_id();
    let root = if let Some(root) = r
        .roots
        .iter()
        .find(|v| v["document_comment"]["thread_id"] == control)
    {
        root.clone()
    } else {
        let root = json!({"type":"message","subtype":"document_comment_root","channel":"Cfixture","ts":format!("172000{:04}.000100",r.roots.len()+1),
            "text":intent.record().annotation().snippet()["mrkdwn"].as_str().unwrap_or(""),"user":"U1","reply_count":0,"document_comment":{"thread_id":control}});
        r.roots.push(root.clone());
        root
    };
    #[cfg(test)]
    if r.mode == 4 {
        r.mode = 0;
        return Err(CanvasEditFailure::Uncertain(Error::Transport(
            "synthetic lost root reply".into(),
        )));
    }
    intent
        .accept(&json!({"ok":true,"has_more":false,"messages":[root]}))
        .map_err(|_| CanvasEditFailure::Uncertain(failure()))
}
pub(in crate::canvas) fn response(
    shell: &ShellState,
    ticket: &super::super::Ticket,
) -> Result<Value, Error> {
    let session = session(shell, &ticket.owner)?;
    let request = wire(super::super::request(&shell.core.client, &session, ticket)?)?;
    let capability = capability(shell)?;
    let mut r = capability.record.lock().map_err(|_| failure())?;
    r.requests.push(request);
    let messages = match &ticket.query {
        Query::Stream(_) => r.roots.clone(),
        Query::Replies(ts) => {
            let root = r
                .roots
                .iter()
                .find(|v| v["ts"] == *ts)
                .ok_or_else(failure)?
                .clone();
            let mut rows = vec![root];
            rows.extend(r.replies.iter().filter(|v| v["thread_ts"] == *ts).cloned());
            rows
        }
    };
    Ok(json!({"ok":true,"messages":messages,"has_more":false}))
}
pub(in crate::canvas) fn reply_canonical(
    shell: &ShellState,
    owner: &Owner,
    ts: &str,
) -> Result<(File, Message), Error> {
    let session = session(shell, owner)?;
    let capability = capability(shell)?;
    let mut r = capability.record.lock().map_err(|_| failure())?;
    r.requests.push(wire(api::canvas_info(
        &shell.core.client,
        &session,
        "Ffixture",
    ))?);
    r.requests.push(wire(api::canvas_comment_replies(
        &shell.core.client,
        &session,
        "Ffixture",
        ts,
        None,
    )?)?);
    let root = r.roots.iter().find(|v| v["ts"] == ts).ok_or_else(failure)?;
    Ok((
        r.canonical.as_ref().ok_or_else(failure)?.file.clone(),
        serde_json::from_value(root.clone()).map_err(|_| failure())?,
    ))
}
pub(in crate::canvas) fn reply(
    shell: &ShellState,
    owner: &Owner,
    intent: ReplyIntent,
) -> Result<ReplyAccepted, CanvasEditFailure> {
    let session = session(shell, owner).map_err(CanvasEditFailure::NotSent)?;
    let request = api::canvas_comment_reply(&shell.core.client, &session, &intent)
        .and_then(wire)
        .map_err(CanvasEditFailure::NotSent)?;
    let capability = capability(shell).map_err(CanvasEditFailure::NotSent)?;
    let mut r = capability
        .record
        .lock()
        .map_err(|_| CanvasEditFailure::NotSent(failure()))?;
    let record = intent.record();
    if !r.roots.iter().any(|v| v["ts"] == record.root_ts()) {
        return Err(CanvasEditFailure::NotSent(failure()));
    }
    r.prepared.push(request.clone());
    r.requests.push(request);
    let ts = format!("172001{:04}.000100", r.replies.len() + 1);
    let mut message = json!({"type":"message","channel":"Cfixture","thread_ts":record.root_ts(),"ts":ts,"user":"U1","text":record.text(),"client_msg_id":record.client_msg_id()});
    if let Some(blocks) = record.blocks() {
        message["blocks"] = json!(blocks);
    }
    r.replies.push(message.clone());
    intent
        .accept(&json!({"ok":true,"channel":"Cfixture","ts":ts,"message":message}))
        .map_err(|_| CanvasEditFailure::Uncertain(failure()))
}

pub(in crate::canvas) fn document() -> super_platinum_core::canvas::CanvasDocument {
    heading::document(&initial())
        .unwrap_or_else(|| crate::canvas::text::structure_fixture::document(&initial()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Synthetic".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        }
    }
    #[test]
    fn named_canonical_fixture_builds_and_accepts_actual_anchor_then_qpc_forms() {
        let s = initial();
        let client = super_platinum_core::slack::client::SlackClient::default();
        let p = AnnotationIntent::capture(&s, "paragraph")
            .unwrap()
            .prepare(&s)
            .unwrap();
        let original = s.sections.clone();
        let prepared_wire =
            wire(api::canvas_annotation(&client, &workspace(), &s, &p).unwrap()).unwrap();
        let mut r = Record::default();
        let ack = complete_anchor(&mut r, &s, p, prepared_wire).unwrap();
        let fresh = r.canonical.as_ref().unwrap();
        assert_eq!(&fresh.sections[..original.len()], original.as_slice());
        let confirmed = ack.confirm(fresh).unwrap();
        let intent = BootstrapIntent::capture(confirmed, 1_720_000_000_001).unwrap();
        let request =
            wire(api::canvas_comment_bootstrap(&client, &workspace(), &intent).unwrap()).unwrap();
        let result = complete_bootstrap(&mut r, intent, request).unwrap();
        assert_eq!(r.requests.len(), 2);
        assert_eq!(r.prepared.len(), 2);
        assert_eq!(
            result.root().extra["document_comment"]["thread_id"],
            r.canonical.as_ref().unwrap().sections.last().unwrap().id()
        );
        assert_eq!(r.roots.len(), 1);
        assert!(r.requests.iter().all(|r| {
            r["fields"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v[0] != "token")
        }));
    }
    #[test]
    fn lost_anchor_reply_keeps_changed_server_state_and_never_manufactures_acceptance() {
        let s = initial();
        let p = AnnotationIntent::capture(&s, "paragraph")
            .unwrap()
            .prepare(&s)
            .unwrap();
        let mut r = Record {
            mode: 2,
            ..Default::default()
        };
        assert!(matches!(
            complete_anchor(&mut r, &s, p, json!({"test":"exact local executor"})),
            Err(CanvasEditFailure::Uncertain(_))
        ));
        assert_eq!(r.requests.len(), 1);
        assert_eq!(
            r.canonical.as_ref().unwrap().sections.len(),
            s.sections.len() + 1
        );
        assert!(AnnotationIntent::capture(r.canonical.as_ref().unwrap(), "paragraph").is_err());
    }
    #[test]
    fn before_handoff_refusal_has_no_request_or_created_control() {
        let s = initial();
        let p = AnnotationIntent::capture(&s, "paragraph")
            .unwrap()
            .prepare(&s)
            .unwrap();
        let mut r = Record {
            mode: 1,
            ..Default::default()
        };
        assert!(matches!(
            complete_anchor(&mut r, &s, p, json!({"test":"exact local executor"})),
            Err(CanvasEditFailure::NotSent(_))
        ));
        assert_eq!(r.prepared.len(), 1);
        assert!(r.requests.is_empty());
        assert_eq!(r.canonical.as_ref().unwrap().sections, s.sections);
    }
    #[test]
    fn inline_fixture_consumes_both_updates_then_actual_bootstrap_and_lost_ack_stays_unknown() {
        let s = inline_snapshot(initial());
        for lost in [false, true] {
            let p = AnnotationIntent::capture_range(&s, "paragraph", 1, 4)
                .unwrap()
                .prepare_at(&s, 1_000_000)
                .unwrap();
            let request = wire(
                api::canvas_annotation(
                    &super_platinum_core::slack::client::SlackClient::default(),
                    &workspace(),
                    &s,
                    &p,
                )
                .unwrap(),
            )
            .unwrap();
            let mut r = Record {
                canonical: Some(s.clone()),
                mode: if lost { 2 } else { 0 },
                ..Default::default()
            };
            let result = complete_anchor(&mut r, &s, p, request);
            assert_eq!(r.requests.len(), 1);
            assert_ne!(
                r.canonical.as_ref().unwrap().section("paragraph"),
                s.section("paragraph")
            );
            if lost {
                assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
            } else {
                let proof = result
                    .unwrap()
                    .confirm(r.canonical.as_ref().unwrap())
                    .unwrap();
                let intent = BootstrapIntent::capture(proof, 1_720_000_000_001).unwrap();
                let request = wire(
                    api::canvas_comment_bootstrap(
                        &super_platinum_core::slack::client::SlackClient::default(),
                        &workspace(),
                        &intent,
                    )
                    .unwrap(),
                )
                .unwrap();
                assert!(complete_bootstrap(&mut r, intent, request).is_ok());
                assert_eq!(r.roots.len(), 1);
            }
            assert!(
                AnnotationIntent::capture_range(r.canonical.as_ref().unwrap(), "paragraph", 1, 4)
                    .is_err()
            );
        }
    }

    #[test]
    fn existing_inline_fixture_preserves_prior_thread_through_second_disjoint_ack_or_loss() {
        for lost in [false, true] {
            let mut s =
                inline_snapshot((*crate::canvas::text::structure_fixture::snapshot()).clone());
            // Match initial(): body-edit fixture permissions do not grant comments.
            s.file.extra.insert("access".into(), json!("comment"));
            s.thread = s
                .thread
                .replace_fields(23, &[F::uint(23, 5).unwrap()])
                .unwrap();
            let mut r = existing_seed(s);
            assert_eq!(r.seed_requests.len(), 2);
            assert!(r.requests.is_empty());
            assert_eq!(r.roots.len(), 1);
            assert_eq!(r.replies.len(), 1);
            let before = r.canonical.clone().unwrap();
            let old = before
                .sections
                .iter()
                .find(|s| s.section_type().unwrap() == Some(9))
                .unwrap()
                .clone();
            let mapping =
                super_platinum_core::canvas::edit::annotation::range_seeds(&before).unwrap();
            assert!(mapping.iter().any(|(id, p)| {
                id == "paragraph"
                    && p.runs
                        .iter()
                        .any(|r| r.control.as_deref() == Some(old.id()))
            }));
            let now = crate::canvas::checklist::save::now_usec().unwrap();
            let p = AnnotationIntent::capture_range(&before, "paragraph", 4, 5)
                .unwrap()
                .prepare_at(&before, now)
                .unwrap();
            let request = wire(
                api::canvas_annotation(
                    &super_platinum_core::slack::client::SlackClient::default(),
                    &workspace(),
                    &before,
                    &p,
                )
                .unwrap(),
            )
            .unwrap();
            r.mode = if lost { 2 } else { 0 };
            let result = complete_anchor(&mut r, &before, p, request);
            assert_eq!(r.requests.len(), 1);
            let after = r.canonical.as_ref().unwrap();
            assert_eq!(after.section(old.id()), Some(&old));
            assert_eq!(before.document, after.document);
            assert_eq!(r.roots.len(), 1);
            assert_eq!(r.replies.len(), 1);
            if lost {
                assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
            } else {
                assert!(result.unwrap().check_confirmation(after).is_ok());
            }
        }
    }
    #[test]
    fn disconnected_owner_matches_actual_initializer_and_refuses_a_foreign_team() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let snapshot = initial();
        assert!(install_disconnected_owner(&mut shell, &snapshot));
        assert_eq!(
            shell.self_account.user_id,
            snapshot.init.options().string(33).unwrap().unwrap()
        );
        assert_eq!(
            shell.core.workspaces.get("T1").unwrap().self_user_id,
            shell.self_account.user_id
        );
        let retained = shell
            .canvas_checklist
            .comment_annotations
            .fixture
            .clone()
            .unwrap();
        shell.core.active_team = Some("TOTHER".into());
        assert!(!install_disconnected_owner(&mut shell, &snapshot));
        assert!(Arc::ptr_eq(
            &retained,
            shell
                .canvas_checklist
                .comment_annotations
                .fixture
                .as_ref()
                .unwrap()
        ));
        assert!(shell.core.session.is_none());
        assert!(shell.core.transport.is_none());
    }
    #[test]
    fn archived_fixture_keeps_prior_thread_and_comment_only_access_through_new_annotation() {
        for lost in [false, true] {
            let base = inline_snapshot(initial());
            let mut record = archived_seed(base);
            let before = record.canonical.clone().unwrap();
            let roots = record.roots.clone();
            let replies = record.replies.clone();
            let seed = super_platinum_core::canvas::edit::annotation::range_seeds(&before)
                .unwrap()
                .into_iter()
                .find(|(id, _)| id == "paragraph")
                .unwrap()
                .1;
            assert_eq!(seed.archived.len(), 1);
            let old = seed.archived[0].clone();
            assert_eq!(before.file.extra["access"], "comment");
            let now = crate::canvas::checklist::save::now_usec().unwrap();
            let prepared = AnnotationIntent::capture_range_with_archived(
                &before,
                "paragraph",
                1,
                4,
                vec![old.clone()],
            )
            .unwrap()
            .prepare_at(&before, now)
            .unwrap();
            let inert = prepared.record().unwrap();
            assert_eq!(serde_json::to_value(&inert).unwrap()["version"], 4);
            let request = wire(
                api::canvas_annotation(
                    &super_platinum_core::slack::client::SlackClient::default(),
                    &workspace(),
                    &before,
                    &prepared,
                )
                .unwrap(),
            )
            .unwrap();
            record.mode = if lost { 2 } else { 0 };
            let result = complete_anchor(&mut record, &before, prepared, request);
            let after = record.canonical.as_ref().unwrap();
            assert_eq!(after.section(&old), before.section(&old));
            assert_eq!(after.document, before.document);
            assert_eq!(record.roots, roots);
            assert_eq!(record.replies, replies);
            assert_eq!(record.requests.len(), 1);
            assert_eq!(record.seed_requests.len(), 2);
            assert!(
                inert
                    .review_current(after)
                    .unwrap()
                    .matches_current(after)
                    .unwrap()
            );
            if lost {
                assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
            } else {
                assert!(result.unwrap().check_confirmation(after).is_ok());
            }
        }
    }

    #[test]
    fn fixture_retained_roots_are_send_without_any_renderer_or_transport_handle() {
        fn send<T: Send>() {}
        send::<Root>();
        send::<State>();
        send::<ShellState>();
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        assert!(path(&shell).is_none());
        assert!(capability(&shell).is_err());
    }
}

pub(in crate::canvas) fn file() -> File {
    initial().file
}
