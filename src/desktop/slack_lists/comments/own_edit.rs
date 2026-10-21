//! A mounted edit owns one original reply and never shares its new-reply journal.
use super::*;
use crate::view::slack_lists::comments::own_edit::bridge;
use serde_json::json;
use std::sync::Mutex;
use super_platinum_core::{
    slack::models::Message,
    slack_lists::comments::{OwnEditRecord, OwnEditSeed},
};
mod durable;
mod native;
mod publish;
mod run;
mod source;
#[cfg(test)]
mod tests;
pub(crate) use durable::persist_capture;
pub(crate) use native::{install_native, native, native_done, retain_capture, retry_capture};
pub(crate) use run::{cancel, check_result, open, save};
use source::Source;

#[derive(Clone, Default)]
pub(crate) struct State {
    pending: Option<Arc<Source>>,
    active: Option<Session>,
    // Mounted buffers remain bound to their original owner after forced
    // retirement, including an exact-node final capture still in flight.
    retired: Vec<Session>,
    attempts: Vec<Arc<Mutex<Attempt>>>,
    events: u64,
}
#[derive(Clone)]
pub(crate) struct Binding {
    source: Arc<Source>,
    token: u64,
}
impl Binding {
    pub(crate) fn scope(&self) -> String {
        format!("list-edit:{}:{}", self.source.owner.serial, self.token)
    }
    pub(crate) fn generation(&self) -> u64 {
        self.token
    }
    pub(crate) fn owner_team(&self) -> &str {
        &self.source.owner.scope.team
    }
    pub(crate) fn visible_mount(&self, shell: &ShellState) -> bool {
        self.source.owner.active(shell) && session(shell, self).is_some()
    }
    pub(crate) fn retained_mount(&self, shell: &ShellState) -> bool {
        session(shell, self).is_some_and(|s| s.native.as_ref().is_some_and(|m| m.matches(self)))
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.visible_mount(shell) && self.source.current(shell)
    }
}
#[derive(Clone)]
struct Session {
    binding: Binding,
    seed: Arc<OwnEditSeed>,
    blocks: Vec<Value>,
    revision: u64,
    native: Option<bridge::Mounted>,
    native_sequence: u64,
    native_ready: bool,
    composing: bool,
    capture_failed: bool,
    busy: bool,
    finishing: bool,
    durable: Option<Arc<durable::Durable>>,
    local_retry_at: i64,
    notice: Option<String>,
    suspension: Arc<Mutex<Option<draft::ReplySuspension>>>,
    attempt: Option<Arc<Mutex<Attempt>>>,
}
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Preparing,
    Sent,
    NotSent,
    Rejected,
    Unknown,
    Confirmed,
    Observed,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Sent => "sent",
            Self::NotSent => "not-sent",
            Self::Rejected => "rejected",
            Self::Unknown => "unknown",
            Self::Confirmed => "confirmed",
            Self::Observed => "observed",
        }
    }
    fn blocks(self) -> bool {
        matches!(self, Self::Preparing | Self::Sent | Self::Unknown)
    }
}
struct Attempt {
    id: u64,
    disk_id: Option<String>,
    account: Option<String>,
    record: OwnEditRecord,
    phase: Phase,
    receipt: Option<Value>,
    retry_at: i64,
}
#[derive(Clone)]
pub(crate) struct Presentation {
    pub binding: Binding,
    pub blocks: String,
    pub revision: u64,
    pub busy: bool,
    pub finishing: bool,
    pub notice: Option<String>,
    pub can_save: bool,
    pub can_cancel: bool,
    pub can_check: bool,
    pub can_retry_capture: bool,
}
fn session<'a>(shell: &'a ShellState, binding: &Binding) -> Option<&'a Session> {
    shell
        .slack_lists
        .comments
        .own_edit
        .active
        .iter()
        .chain(shell.slack_lists.comments.own_edit.retired.iter())
        .find(|s| {
            s.binding.token == binding.token && Arc::ptr_eq(&s.binding.source, &binding.source)
        })
}
fn session_mut<'a>(shell: &'a mut ShellState, binding: &Binding) -> Option<&'a mut Session> {
    let state = &mut shell.slack_lists.comments.own_edit;
    state
        .active
        .iter_mut()
        .chain(state.retired.iter_mut())
        .find(|s| {
            s.binding.token == binding.token && Arc::ptr_eq(&s.binding.source, &binding.source)
        })
}
fn attempt_phase(s: &Session) -> Option<Phase> {
    s.attempt.as_ref().map(|a| a.lock().unwrap().phase)
}
fn is_attempt_buffer(s: &Session) -> bool {
    let Some(a) = &s.attempt else {
        return false;
    };
    let a = a.lock().unwrap();
    let mut full = s.blocks.clone();
    full.extend_from_slice(s.seed.preserved());
    full == a.record.blocks()
}
fn retry_deadline(error: &Error) -> i64 {
    match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
            ..
        } => {
            now().saturating_add(i64::try_from(retry_after_secs.unwrap_or(30)).unwrap_or(i64::MAX))
        }
        _ => 0,
    }
}
fn blocked_attempt(shell: &ShellState, source: &Source) -> bool {
    shell
        .slack_lists
        .comments
        .own_edit
        .attempts
        .iter()
        .any(|a| {
            let a = a.lock().unwrap();
            a.phase.blocks()
                && a.disk_id.is_none()
                && a.account == shell.core.active_account
                && a.record.workspace() == source.owner.scope.team
                && a.record.ts() == source.ts
                && a.record.target().list_id() == source.target.list_id()
                && a.record.target().record_id() == source.target.record_id()
                && a.record.target().root_ts() == source.target.root_ts()
        })
}
pub(crate) fn available(shell: &ShellState, generation: u64, ts: &str) -> bool {
    if shell.slack_lists.comments.own_edit.retired.len() >= 32
        || blocks_departure(shell, generation)
        || super::own_delete::blocks_departure(shell, generation)
        || (crate::fixture::is_fixture() && !fixture::own_edit::enabled(shell))
    {
        return false;
    }
    Source::capture(shell, generation, ts).is_some_and(|s| !blocked_attempt(shell, &s))
}
pub(crate) fn blocks_departure(shell: &ShellState, generation: u64) -> bool {
    let s = &shell.slack_lists.comments.own_edit;
    s.pending
        .as_ref()
        .is_some_and(|p| p.owner.serial == generation && p.owner.active(shell))
        || s.active.as_ref().is_some_and(|s| {
            s.binding.source.owner.serial == generation && s.binding.source.owner.active(shell)
        })
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let s = shell.slack_lists.comments.own_edit.active.as_ref()?;
    if !s.binding.visible_mount(shell) {
        return None;
    }
    let phase = attempt_phase(s);
    let unknown = phase == Some(Phase::Unknown);
    let terminal = matches!(phase, Some(Phase::Confirmed | Phase::Observed));
    let retry_at = s
        .attempt
        .as_ref()
        .map_or(0, |a| a.lock().unwrap().retry_at)
        .max(s.local_retry_at);
    let native_ready = s.native_ready
        && !s.capture_failed
        && s.native
            .as_ref()
            .is_some_and(|m| m.matches(&s.binding) && !m.failed());
    let original_current = s.binding.current(shell)
        && shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .and_then(|e| source::message(e, &s.binding.source.ts))
            .is_some_and(|m| s.seed.current(m));
    let can_edit = native_ready && original_current && !s.composing && !s.finishing;
    Some(Presentation {
        binding: s.binding.clone(),
        blocks: serde_json::to_string(&s.blocks).unwrap_or_else(|_| "[]".into()),
        revision: s.revision,
        busy: s.busy,
        finishing: s.finishing,
        notice: s.notice.clone().or_else(|| {
            (!original_current).then(|| {
                "The original comment changed. Your edit and reply draft are retained.".into()
            })
        }),
        can_save: can_edit && !s.busy && !unknown && !terminal && retry_at <= now(),
        can_cancel: (native_ready || s.finishing) && !s.busy && !unknown,
        can_check: native_ready && unknown && !s.busy && retry_at <= now(),
        can_retry_capture: native::can_retry_capture(shell, &s.binding),
    })
}
pub(crate) fn observes_preferences(shell: &ShellState, team: &str, generation: u64) -> bool {
    let state = &shell.slack_lists.comments.own_edit;
    state
        .pending
        .iter()
        .map(Arc::as_ref)
        .chain(state.active.iter().map(|s| s.binding.source.as_ref()))
        .any(|s| s.owner.active(shell) && s.owner.scope.team == team && s.socket == generation)
}
/// Body-bearing original frames invalidate an edit even when their values are
/// equal now, or a later frame restores the old body. Reaction-only frames do
/// not change this witness. Call before the ordinary comment reducers.
pub(crate) fn observe_event(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &super_platinum_core::slack::events::RtEvent,
) -> bool {
    use super_platinum_core::slack::events::{MessageUpdateKind, RtEvent};
    let state = &shell.slack_lists.comments.own_edit;
    let source = state
        .pending
        .as_deref()
        .or_else(|| state.active.as_ref().map(|s| s.binding.source.as_ref()));
    let Some(source) = source else {
        return false;
    };
    if source.owner.scope.team != team
        || source.socket != generation
        || !source.owner_current(shell)
    {
        return false;
    }
    let touched = match event {
        RtEvent::MessageDeleted {
            channel,
            deleted_ts,
        } => {
            channel == source.target.channel_id()
                && (deleted_ts == &source.ts
                    || Some(deleted_ts.as_str()) == source.target.root_ts())
        }
        RtEvent::MessageChanged {
            channel,
            message,
            original_update: Some(update),
        } => {
            channel == source.target.channel_id()
                && message.ts.as_deref() == Some(source.ts.as_str())
                && update.kind() == MessageUpdateKind::Changed
                && update.matches_identity(channel, message)
                && [
                    "text",
                    "blocks",
                    "edited",
                    "files",
                    "attachments",
                    "markdown_text",
                    "is_deleted",
                    "is_tombstoned",
                    "no_display",
                    "_hidden_stub",
                    "subtype",
                    "user",
                ]
                .iter()
                .any(|key| update.raw().get(*key).is_some())
        }
        _ => false,
    };
    if touched {
        shell.slack_lists.comments.own_edit.events =
            shell.slack_lists.comments.own_edit.events.wrapping_add(1);
    }
    touched
}
/// Forced retirement only. Ordinary departures must first Save or Cancel.
/// Issued credential-free records survive; no callback may update a successor.
pub(crate) fn retire(shell: &mut ShellState) {
    let stale = shell
        .slack_lists
        .comments
        .own_edit
        .active
        .as_ref()
        .is_some_and(|s| !s.binding.source.owner.active(shell));
    if stale {
        let state = &mut shell.slack_lists.comments.own_edit;
        if let Some(s) = state.active.take() {
            // Release only this original hidden reply holder. Its edit native
            // holder remains reachable for a detached final capture.
            s.suspension.lock().unwrap().take();
            state.retired.push(s);
        }
    }
    let stale = shell
        .slack_lists
        .comments
        .own_edit
        .pending
        .as_ref()
        .is_some_and(|s| !s.owner.active(shell));
    if stale {
        shell.slack_lists.comments.own_edit.pending = None;
    }
}
pub(crate) fn carry_pending(previous: &ShellState, loaded: &mut ShellState) {
    loaded.slack_lists.comments.own_edit.attempts =
        previous.slack_lists.comments.own_edit.attempts.clone();
    loaded.slack_lists.comments.own_edit.retired =
        previous.slack_lists.comments.own_edit.retired.clone();
    if let Some(s) = &previous.slack_lists.comments.own_edit.active {
        loaded.slack_lists.comments.own_edit.retired.push(s.clone());
    }
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    if !cfg!(test) && !fixture::own_edit::enabled(shell) {
        return Value::Null;
    }
    let state = &shell.slack_lists.comments.own_edit;
    json!({"opening":state.pending.is_some(),"events":state.events,"retired":state.retired.iter().map(|s|json!({"scope":s.binding.scope(),"ts":s.binding.source.ts,"blocks":s.blocks,"revision":s.revision})).collect::<Vec<_>>(),"active":state.active.as_ref().map(|s|json!({
        "scope":s.binding.scope(),"generation":s.binding.generation(),"ts":s.binding.source.ts,"current":s.binding.current(shell),
        "blocks":s.blocks,"revision":s.revision,"busy":s.busy,"notice":s.notice,"composing":s.composing,"nativeReady":s.native_ready,"captureFailed":s.capture_failed,
        "source":s.seed.original_raw(),"preserved":s.seed.preserved(),"attempt":s.attempt.as_ref().map(|a|a.lock().unwrap().id)})),
        "attempts":state.attempts.iter().map(|a|{let a=a.lock().unwrap();json!({"id":a.id,"ts":a.record.ts(),"blocks":a.record.blocks(),"phase":a.phase.name(),"receipt":a.receipt,"retryAt":a.retry_at})}).collect::<Vec<_>>()})
}
