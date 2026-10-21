//! Explicit own-reply deletion. Reply/edit buffers never become delete receipts.
use super::*;
use std::sync::Mutex;
use super_platinum_core::{
    slack::models::Message,
    slack_lists::comments::{OwnDeleteIntent, OwnDeleteRecord, journal::own_delete::DeletePhase},
};
mod durable;
mod run;
mod source;
#[cfg(test)]
mod tests;
pub(crate) use run::{cancel, check_result, confirm, open, retry_local};
use source::Source;

#[derive(Clone, Default)]
pub(crate) struct State {
    active: Option<Session>,
    retained: Vec<Arc<durable::Recovery>>,
    events: u64,
}
#[derive(Clone)]
struct Session {
    id: u64,
    source: Arc<Source>,
    preview: crate::model::MessageVm,
    busy: bool,
    notice: Option<String>,
    record: Option<OwnDeleteRecord>,
    phase: Option<DeletePhase>,
    retry_at: u64,
    local_pending: bool,
    recovery: Option<Arc<durable::Recovery>>,
    suspension: Arc<Mutex<Option<draft::ReplySuspension>>>,
}
#[derive(Clone)]
pub(crate) struct Presentation {
    pub id: u64,
    pub generation: u64,
    pub channel: String,
    pub preview: crate::model::MessageVm,
    pub busy: bool,
    pub can_confirm: bool,
    pub can_check: bool,
    pub can_retry_local: bool,
    pub notice: Option<String>,
    pub terminal: bool,
    pub secondary_note: Option<String>,
}
fn session(shell: &ShellState, id: u64) -> Option<&Session> {
    shell
        .slack_lists
        .comments
        .own_delete
        .active
        .as_ref()
        .filter(|s| s.id == id)
}
fn session_mut(shell: &mut ShellState, id: u64) -> Option<&mut Session> {
    shell
        .slack_lists
        .comments
        .own_delete
        .active
        .as_mut()
        .filter(|s| s.id == id)
}
pub(crate) fn blocks_departure(shell: &ShellState, generation: u64) -> bool {
    shell
        .slack_lists
        .comments
        .own_delete
        .active
        .as_ref()
        .is_some_and(|s| s.source.owner.serial == generation && s.source.owner.active(shell))
}
pub(crate) fn hides_reply(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .comments
        .own_delete
        .active
        .as_ref()
        .is_some_and(|s| s.source.owner.active(shell) && s.suspension.lock().unwrap().is_some())
}
pub(crate) fn available(shell: &ShellState, generation: u64, ts: &str) -> bool {
    !blocks_departure(shell, generation)
        && !own_edit::blocks_departure(shell, generation)
        && (!crate::fixture::is_fixture() || fixture::own_delete::enabled(shell))
        && Source::capture(shell, generation, ts).is_some()
}
/// Reopening retained local recovery does not authorize another network call.
/// Keep it reachable during cooldown, bound to the same account/item/database.
pub(crate) fn retained_for(shell: &ShellState, generation: u64, ts: &str) -> bool {
    let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
        return false;
    };
    if entry.owner.serial != generation {
        return false;
    }
    let Ok(disk) = storage::Disk::capture(shell, &entry.owner, &entry.target) else {
        return false;
    };
    shell
        .slack_lists
        .comments
        .own_delete
        .retained
        .iter()
        .any(|recovery| recovery.disk == disk && recovery.ts == ts)
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let s = shell.slack_lists.comments.own_delete.active.as_ref()?;
    if !s.source.owner.active(shell) {
        return None;
    }
    let terminal = matches!(
        s.phase,
        Some(DeletePhase::Confirmed | DeletePhase::Observed)
    );
    let locked = s.phase.is_some_and(DeletePhase::locked);
    let ready = !s.busy
        && !s.local_pending
        && s.retry_at <= now().max(0) as u64
        && cooldown(shell) <= now()
        && shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .is_some_and(|e| e.retry_at <= now());
    Some(Presentation {
        id:s.id, generation:s.source.owner.serial, channel:s.source.target.channel_id().into(),
        preview:s.preview.clone(), busy:s.busy, terminal,
        secondary_note: (s.source.original.subtype.as_deref()==Some("file_share") && !s.source.original.files.is_empty())
            .then(|| "Note that deleting this message will not unshare the file.".into()),
        can_confirm:ready && !terminal && !locked && s.record.is_some() && s.source.current(shell),
        can_check:ready && locked && s.source.owner_current(shell),
        can_retry_local:!s.busy && s.local_pending,
        notice:s.notice.clone().or_else(|| (!terminal && !s.source.current(shell)).then(||
            "This comment changed. Cancel returns to your saved reply; no new deletion is allowed.".into())),
    })
}
pub(crate) fn observe_event(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &super_platinum_core::slack::events::RtEvent,
) {
    use super_platinum_core::slack::events::RtEvent;
    let Some(s) = shell.slack_lists.comments.own_delete.active.as_ref() else {
        return;
    };
    if s.source.owner.scope.team != team
        || !s.source.owner_current(shell)
        || shell
            .core
            .workspaces
            .get(team)
            .is_none_or(|w| w.rt_generation != generation)
    {
        return;
    }
    let changed = match event {
        RtEvent::MessageDeleted {
            channel,
            deleted_ts,
        } => {
            channel == s.source.target.channel_id()
                && (deleted_ts == &s.source.ts
                    || Some(deleted_ts.as_str()) == s.source.target.root_ts())
        }
        RtEvent::MessageChanged {
            channel,
            message,
            original_update: Some(update),
        } => {
            channel == s.source.target.channel_id()
                && message.ts.as_deref() == Some(s.source.ts.as_str())
                && update.matches_identity(channel, message)
                && [
                    "text",
                    "blocks",
                    "files",
                    "attachments",
                    "edited",
                    "markdown_text",
                    "subtype",
                    "user",
                    "is_deleted",
                    "is_tombstoned",
                    "no_display",
                    "_hidden_stub",
                ]
                .iter()
                .any(|k| update.raw().get(*k).is_some())
        }
        _ => false,
    };
    if changed {
        shell.slack_lists.comments.own_delete.events =
            shell.slack_lists.comments.own_delete.events.wrapping_add(1);
    }
}
pub(crate) fn carry_pending(previous: &ShellState, loaded: &mut ShellState) {
    loaded.slack_lists.comments.own_delete.retained =
        previous.slack_lists.comments.own_delete.retained.clone();
}
fn suspended(shell: &ShellState, id: u64) -> bool {
    session(shell, id).is_some_and(|s| {
        s.suspension
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|v| v.current(shell))
    })
}
fn notice(mut state: Signal<ShellState>, id: u64, text: &str) {
    if let Some(s) = session_mut(&mut state.write(), id) {
        s.busy = false;
        s.notice = Some(text.into());
    }
}
fn retry_deadline(error: &Error) -> i64 {
    let seconds = match error {
        Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(30)),
        Error::HttpStatus {
            retry_after_secs: Some(seconds),
            ..
        } => Some(*seconds),
        Error::HttpStatus { status: 429, .. } => Some(30),
        _ => None,
    };
    seconds.map_or(0, |seconds| {
        now().saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX))
    })
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let active = shell
        .slack_lists
        .comments
        .own_delete
        .active
        .as_ref()
        .map(|s| {
            let view = presentation(shell);
            serde_json::json!({"id":s.id,"generation":s.source.owner.serial,"ts":s.source.ts,
            "busy":s.busy,"phase":s.phase,"localPending":s.local_pending,
            "sourceCurrent":s.source.current(shell),"record":s.record,
            "canConfirm":view.as_ref().is_some_and(|p|p.can_confirm),
            "canCheck":view.as_ref().is_some_and(|p|p.can_check),
            "canRetryLocal":view.as_ref().is_some_and(|p|p.can_retry_local)})
        });
    serde_json::json!({"active":active,"retainedCount":shell.slack_lists.comments.own_delete.retained.len()})
}
pub(crate) async fn journal_snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    if !crate::fixture::is_fixture() {
        return Err("Closed deletion fixture required".into());
    }
    let recovery = {
        let shell = state.peek();
        shell
            .slack_lists
            .comments
            .own_delete
            .active
            .as_ref()
            .and_then(|s| s.recovery.clone())
            .or_else(|| {
                shell
                    .slack_lists
                    .comments
                    .own_delete
                    .retained
                    .last()
                    .cloned()
            })
            .ok_or("No deletion recovery")?
    };
    let scope = recovery.disk.scope.clone();
    let ts = recovery.ts.clone();
    recovery
        .disk
        .work(move |j| j.delete_inventory(&scope, &ts))
        .await
        .map_err(|_| "Deletion journal unavailable".to_string())
        .and_then(|inventory| serde_json::to_value(inventory).map_err(|e| e.to_string()))
}
