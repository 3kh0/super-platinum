//! Disconnected saved split-owner fixture; real proofs and custody, no HTTP.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use super_platinum_core::{
    canvas::{
        edit::{
            structure::PreparedStructureEdit,
            text::{PreparedTextEdit, TextDraft},
        },
        wire::{EditAck, EncodedField as F, RawMessage, Section},
    },
    slack::api::CanvasCollabSnapshot,
};
pub(crate) mod check;
mod child_delete;
mod completion;
pub(super) mod gap;
mod live_rich;
mod owner_first;
mod owner_rich;
mod recovery;
mod retained_pair;
mod seed;
pub(super) mod worker_fixture;
pub(super) const ACCOUNT: &str = "00000000-0000-4000-8000-000000009993";
#[derive(Default)]
pub(in crate::canvas) struct State {
    pending: Option<(checklist::Attempt, PreparedStructureEdit)>,
    pending_text: Option<(
        checklist::Attempt,
        PreparedTextEdit,
        checklist::session_worker::Lease,
    )>,
    completing: Option<Arc<()>>,
    pub(super) writes: Vec<Value>,
    pub(super) acknowledgements: Vec<Value>,
    pub(super) reservations: usize,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-owner-delete"
                | "canvas-owner-delete-narrow"
                | "canvas-live-split-rich"
                | "canvas-live-split-rich-narrow"
                | "canvas-split-gap"
                | "canvas-split-gap-narrow"
                | "canvas-coalesced-owner-rich"
                | "canvas-coalesced-owner-rich-narrow"
                | "canvas-live-split-child-delete"
                | "canvas-live-split-child-delete-narrow"
                | "canvas-retained-pair-delete"
                | "canvas-retained-pair-delete-narrow"
                | "canvas-owner-first-pair-delete"
                | "canvas-owner-first-pair-delete-narrow")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.self_account.user_id == "U1"
        && shell
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .is_some_and(|f| f.owner_delete.is_some())
}
fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}
pub(super) fn phase() -> String {
    env("SUPER_PLATINUM_CANVAS_OWNER_DELETE_PHASE", "success")
}
pub(super) fn corpus() -> String {
    env("SUPER_PLATINUM_CANVAS_OWNER_DELETE_CORPUS", "ascii")
}
pub(super) fn mode() -> String {
    env("SUPER_PLATINUM_CANVAS_OWNER_DELETE_HISTORY", "bootstrap")
}
pub(super) fn coalesced() -> bool {
    owner_first::coalesced()
        || retained_pair::coalesced()
        || child_delete::coalesced()
        || owner_rich::requested()
        || gap::requested()
        || live_rich::requested()
        || env("SUPER_PLATINUM_CANVAS_OWNER_DELETE_COALESCED", "0") == "1"
}
pub(super) fn parent(mode: &str) -> &'static str {
    if mode == "retained-zero" {
        "temp:C:Dte0000000000000000000000001"
    } else {
        "paragraph"
    }
}
pub(super) fn texts(corpus: &str) -> (&'static str, &'static str) {
    if corpus == "unicode" {
        (
            "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}",
            "Second \u{65e5}\u{672c}\u{1f600} & paragraph",
        )
    } else {
        ("Keep this", "Second paragraph")
    }
}
fn escaped(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn n(tag: u32, v: u64) -> F {
    F::uint(tag, v).unwrap()
}
fn t(tag: u32, v: &str) -> F {
    F::bytes(tag, v.as_bytes()).unwrap()
}
fn m(tag: u32, v: &RawMessage) -> F {
    F::message(tag, v).unwrap()
}
fn msg(v: &[F]) -> RawMessage {
    RawMessage::from_fields(v).unwrap()
}
fn body(v: &str, language: bool) -> RawMessage {
    let mut f = vec![t(1, &escaped(v))];
    if language {
        f.push(t(2, "en"));
    }
    msg(&[m(1, &msg(&f))])
}
fn snapshot(shell: &ShellState) -> Arc<CanvasCollabSnapshot> {
    shell
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap()
}
fn fixture(shell: &mut ShellState) -> &mut State {
    shell
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .owner_delete
        .as_mut()
        .unwrap()
}
fn same_source(shell: &ShellState, source: &Arc<CanvasCollabSnapshot>) -> bool {
    enabled(shell) && Arc::ptr_eq(&snapshot(shell), source)
}
fn snapshot_path() -> std::path::PathBuf {
    std::env::var_os("SUPER_PLATINUM_CANVAS_OWNER_DELETE_SNAPSHOT")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .expect("isolated absolute canonical fixture path")
}
pub(in crate::canvas) async fn install(mut state: Signal<ShellState>, mut reader: Signal<Reader>) {
    let captured = {
        let mut shell = state.write();
        if !requested()
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.core.active_team.as_deref() != Some("T1")
        {
            return;
        }
        if shell.canvas_checklist.structure_fixture.is_some() {
            return;
        }
        shell.core.active_account = Some(ACCOUNT.into());
        shell.self_account.user_id = "U1".into();
        shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U1".into();
        (
            shell.canvas.as_ref().map(|c| c.id.clone()),
            shell.core.active_team.clone(),
            shell.core.account_epoch,
            reader.peek().generation,
        )
    };
    let c = corpus();
    let mode = mode();
    let phase = phase();
    let path = snapshot_path();
    if !matches!(c.as_str(), "ascii" | "unicode")
        || !matches!(mode.as_str(), "bootstrap" | "retained" | "retained-zero")
    {
        return;
    }
    let result = tokio::task::spawn_blocking(move || {
        if matches!(
            phase.as_str(),
            "restart" | "review" | "storage-failure" | "reviewed"
        ) {
            seed::restored(&serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap())
        } else {
            let s = if owner_first::requested() {
                owner_first::seed(&c, &mode)
            } else if retained_pair::requested() {
                retained_pair::seed(&c, &mode)
            } else if child_delete::requested() {
                child_delete::seed(&c, &mode)
            } else if owner_rich::requested() {
                owner_rich::seed(&c, &mode)
            } else if gap::requested() {
                gap::seed(&c, &mode)
            } else if live_rich::requested() {
                seed::seed_cadence(&c, &mode, &live_rich::cadence())
            } else {
                seed::seed_variant(&c, &mode, coalesced())
            };
            let bytes = serde_json::to_vec(&seed::saved(&s)).unwrap();
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap()
                .write_all(&bytes)
                .unwrap();
            s
        }
    })
    .await;
    let Ok(source) = result else {
        return;
    };
    {
        let mut shell = state.write();
        if shell.canvas.as_ref().map(|c| c.id.clone()) != captured.0
            || shell.core.active_team != captured.1
            || shell.core.account_epoch != captured.2
            || reader.peek().generation != captured.3
            || shell.core.active_account.as_deref() != Some(ACCOUNT)
            || shell.canvas_checklist.structure_fixture.is_some()
        {
            return;
        }
        checklist::open_visit(&mut shell);
        shell.canvas_checklist.structure_fixture =
            Some(structure_fixture::owner_delete_state(source));
    }
    reload(state).await;
    checklist::reader::update_mapping(&state.peek(), &mut reader.write());
}
pub(in crate::canvas) async fn reload(mut state: Signal<ShellState>) {
    if !enabled(&state.peek()) {
        return;
    }
    let source = snapshot(&state.peek());
    let Some(read) = checklist::begin_read(&mut state.write()) else {
        return;
    };
    checklist::recovery::load(state, &read).await;
    checklist::finish_read(&mut state.write(), &read, Ok(source));
}
pub(super) async fn dispatch(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
) -> Result<(), &'static str> {
    if !enabled(&state.peek())
        || !checklist::structure_ops::preparation_current(&state.peek(), attempt)
    {
        return Err("This canvas changed before saving.");
    }
    let source = snapshot(&state.peek());
    let fresh = if phase() == "stale" {
        let mut s = (*source).clone();
        let r = s
            .sections
            .iter_mut()
            .find(|s| s.id() == "neighbor")
            .unwrap();
        *r = Section::from_raw(r.raw().replace_fields(2, &[n(2, 2000)]).unwrap()).unwrap();
        Arc::new(s)
    } else {
        source.clone()
    };
    let (prepared, checkpoint) =
        super::owner_delete_save::prepare(attempt, fresh, checklist::save::now_usec().unwrap())
            .await
            .map_err(|_| "This paragraph or your access changed. Cancel and refresh.")?;
    let write = json!({"session":prepared.session_id(),"sequence":prepared.sequence(),"documentData":STANDARD.encode(prepared.document_data().raw()),"checkpoint":checkpoint,"httpFactoryExecuted":false});
    if !same_source(&state.peek(), &source)
        || !checklist::structure_ops::preparation_current(&state.peek(), attempt)
    {
        return Err("This canvas changed before saving.");
    }
    let ticket = checklist::recovery::reserve_owner_delete(state, attempt, checkpoint).await?;
    if !same_source(&state.peek(), &source)
        || !checklist::structure_ops::preparation_current(&state.peek(), attempt)
        || !checklist::structure_ops::mark_dispatched(&mut state.write(), attempt, &prepared)
    {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return Err("This canvas changed before saving.");
    }
    let mut shell = state.write();
    let f = fixture(&mut shell);
    f.reservations += 1;
    f.writes.push(write);
    f.pending = Some((attempt.clone(), prepared));
    Ok(())
}
pub(super) async fn dispatch_text(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    intent: Arc<TextIntent>,
) -> Result<(), &'static str> {
    if !enabled(&state.peek()) || !checklist::dispatchable(&state.peek(), attempt) {
        return Err("This canvas changed before saving.");
    }
    let source = snapshot(&state.peek());
    let mut lease = checklist::session_worker::acquire(&mut state.write(), attempt)?;
    let claim = lease.claim();
    let a = attempt.clone();
    let fresh = if (live_rich::requested() || owner_rich::requested()) && phase() == "stale" {
        let mut s = (*source).clone();
        let row = s
            .sections
            .iter_mut()
            .find(|s| s.id() == attempt.section)
            .unwrap();
        *row = Section::from_raw(
            row.raw()
                .replace_fields(2, &[n(2, row.sequence().unwrap().unwrap() + 1)])
                .unwrap(),
        )
        .unwrap();
        Arc::new(s)
    } else {
        source.clone()
    };
    let result = tokio::task::spawn_blocking(move || {
        let p = lease.prepare(
            &intent,
            &fresh,
            checklist::text_ops::draft(&a).unwrap(),
            checklist::save::now_usec().unwrap(),
        );
        (lease, p)
    })
    .await;
    let (lease, p) = match result {
        Ok(v) => v,
        Err(_) => {
            claim.poison(&mut state.write());
            return Err("Canvas text worker failed.");
        }
    };
    let mut shell = state.write();
    if !same_source(&shell, &source) || !claim.preparing(&shell, attempt) {
        lease.restore(&mut shell);
        return Err("This canvas changed before saving.");
    }
    let p = match p {
        Ok(p) => p,
        Err(_) => {
            lease.restore(&mut shell);
            return Err("This text changed before saving.");
        }
    };
    if !checklist::text_ops::mark_dispatched(&mut shell, attempt, &p) {
        lease.restore(&mut shell);
        return Err("This canvas changed before saving.");
    }
    let f = fixture(&mut shell);
    f.writes.push(json!({"kind":"text","session":p.session_id(),"sequence":p.sequence(),"documentData":STANDARD.encode(p.document_data().raw()),"httpFactoryExecuted":false}));
    f.pending_text = Some((attempt.clone(), p, lease));
    Ok(())
}

#[cfg(test)]
pub(in crate::canvas) fn test_source() -> Arc<CanvasCollabSnapshot> {
    seed::seed("ascii", "bootstrap")
}
#[cfg(test)]
pub(in crate::canvas) fn test_delete(
    p: PreparedStructureEdit,
    source: Arc<CanvasCollabSnapshot>,
) -> (
    super_platinum_core::canvas::edit::structure::AcceptedStructure,
    Arc<CanvasCollabSnapshot>,
) {
    let (a, s, _) = completion::delete(p, source);
    (a, s)
}
