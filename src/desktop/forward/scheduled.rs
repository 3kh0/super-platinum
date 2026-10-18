//! Detached scheduled-forward ownership. The app root, not the dialog, keeps
//! unsaved input, in-flight reservations and storage-only completion retries.
use super::*;
use std::{collections::BTreeMap, sync::Mutex};
use super_platinum_core::forwarding::scheduled as model;
mod capture;
mod controller;
pub(crate) mod fixture;
mod storage;
#[cfg(test)]
mod tests;
mod view;
pub(crate) use storage::allow_existing;
pub(super) use view::Controls;
pub(crate) use view::Inventory;

#[derive(Default)]
pub(crate) struct Registry {
    entries: BTreeMap<String, Entry>,
}
struct Entry {
    storage: storage::Storage,
    intent: model::Intent,
    record: Option<model::Record>,
    completion: Option<Arc<Mutex<model::Completion>>>,
    unsent: Option<Arc<Mutex<model::Unsent>>>,
    local: Option<storage::Local>,
    run: Option<crate::scheduled::mutation::Run>,
    busy: bool,
    capture: Option<String>,
    pending_edit: Option<(Vec<serde_json::Value>, i64)>,
    discovery: Option<model::discovery::Scan>,
    error: String,
}
#[derive(Clone)]
struct Snapshot {
    id: String,
    storage: storage::Storage,
    intent: model::Intent,
    record: Option<model::Record>,
    busy: bool,
    needs_storage: bool,
    continuing: bool,
    error: String,
}
impl Registry {
    fn snapshots(&self, storage: &storage::Storage) -> Vec<Snapshot> {
        self.entries
            .iter()
            .filter(|(_, entry)| &entry.storage == storage)
            .map(|(id, entry)| Snapshot {
                id: id.clone(),
                storage: entry.storage.clone(),
                intent: entry.intent.clone(),
                record: entry.record.clone(),
                busy: entry.busy,
                continuing: entry.discovery.as_ref().is_some_and(|scan| {
                    entry
                        .record
                        .as_ref()
                        .is_some_and(|record| scan.matches(record))
                }),
                needs_storage: entry.completion.is_some()
                    || entry.unsent.is_some()
                    || entry.local.is_some()
                    || entry.record.is_none(),
                error: entry.error.clone(),
            })
            .collect()
    }
    fn adopt(&mut self, storage: storage::Storage, records: Vec<model::Record>) {
        // Retain only one ordinary history page plus actual in-flight/unsaved
        // custody. Older durable rows remain reachable through cursor navigation.
        let ids = records
            .iter()
            .map(|r| r.id())
            .collect::<std::collections::BTreeSet<_>>();
        self.entries.retain(|id, entry| {
            entry.storage != storage
                || ids.contains(id.as_str())
                || entry.busy
                || entry.completion.is_some()
                || entry.unsent.is_some()
                || entry.local.is_some()
                || entry.run.is_some()
                || entry.record.is_none()
                || entry.pending_edit.is_some()
        });
        for record in records {
            if record.intent().owner() != &storage.owner {
                continue;
            }
            let id = record.id().to_owned();
            match self.entries.get_mut(&id) {
                Some(entry)
                    if entry.storage == storage
                        && !entry.busy
                        && entry.completion.is_none()
                        && entry.unsent.is_none()
                        && entry.local.is_none()
                        && entry.pending_edit.is_none() =>
                {
                    if entry
                        .record
                        .as_ref()
                        .is_none_or(|old| old.revision() <= record.revision())
                    {
                        entry.intent = record.intent().clone();
                        entry.record = Some(record);
                    }
                }
                None => {
                    self.entries.insert(
                        id,
                        Entry {
                            storage: storage.clone(),
                            intent: record.intent().clone(),
                            record: Some(record),
                            completion: None,
                            unsent: None,
                            local: None,
                            run: None,
                            busy: false,
                            capture: None,
                            pending_edit: None,
                            discovery: None,
                            error: String::new(),
                        },
                    );
                }
                _ => {}
            }
        }
    }
}

struct Owner {
    storage: storage::Storage,
    context: crate::scheduled::Context,
}
impl Owner {
    fn capture(state: Signal<ShellState>) -> Result<Self, String> {
        let shell = state.peek();
        let storage = storage::Storage::capture(&shell)
            .ok_or("Connect to this account before scheduling.")?;
        let context = crate::scheduled::Context::capture(&shell)
            .ok_or("Connect to this workspace before scheduling.")?;
        Ok(Self { storage, context })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.storage.current(shell)
            && self.context.current(shell)
            && shell
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(&self.storage.owner.team))
                .is_some_and(|current| {
                    let captured = &self.context.session;
                    current.team_id == captured.team_id
                        && current.user_id == captured.user_id
                        && current.token == captured.token
                        && current.url == captured.url
                        && current.enterprise_id == captured.enterprise_id
                })
    }

    /// Revalidate the same ordinary picker facts before dispatch. This is not a
    /// new permission inference: Slack remains authoritative for access policy.
    fn eligible(&self, shell: &ShellState, intent: &model::Intent) -> bool {
        if !self.current(shell) || intent.owner() != &self.storage.owner {
            return false;
        }
        let Some(workspace) = shell.core.workspaces.get(&self.storage.owner.team) else {
            return false;
        };
        let Some(source) = workspace.channels.get(&intent.source().channel) else {
            return false;
        };
        if (source.is_private || source.is_group || source.is_im || source.is_mpim)
            != intent.source().private
        {
            return false;
        }
        match intent.destination() {
            model::Destination::Conversation(id) => workspace
                .channels
                .get(id)
                .is_some_and(|channel| !channel.is_archived),
            model::Destination::Person(id) => workspace
                .users
                .get(id)
                .is_some_and(|user| !user.deleted && user.id != workspace.self_user_id),
        }
    }
}

fn now() -> u64 {
    super::delivery::now()
}
fn fail(mut book: Signal<Registry>, id: &str, message: String) {
    if let Some(entry) = book.write().entries.get_mut(id) {
        entry.busy = false;
        entry.error = message;
    }
}
fn accept(mut state: Signal<ShellState>, owner: &Owner, record: &model::Record) {
    if !owner.current(&state.peek()) {
        return;
    }
    if let Some(raw) = record
        .reviewed()
        .filter(|_| record.phase() == model::Phase::Confirmed)
    {
        if let Ok(draft) = serde_json::from_value::<super_platinum_core::scheduled::ScheduledMessage>(
            raw["draft"].clone(),
        ) {
            crate::scheduled::saved(state, &owner.context, draft);
        }
    } else if record.phase() == model::Phase::Deleted {
        if let Some(id) = record.server_id() {
            state.write().core.scheduled.items.remove(id);
        }
    }
}

// Layout fixtures are explicitly disconnected and cannot open a user journal.
// They do not claim scheduled write/readback or restart verification.
pub(super) fn layout_fixture() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("forward-scheduled" | "forward-scheduled-narrow")
    )
}

pub(super) fn next_editor_visit() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_update(
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
        |value| value.checked_add(1),
    )
    .expect("scheduled forward editor visit exhausted")
}
