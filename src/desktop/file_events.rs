//! Resolve ID-only file_change events without letting older reads undo newer edits.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};
use super_platinum_core::{
    CoreAppState,
    config::WorkspaceSession,
    file_actions as policy,
    slack::{Error, SlackClient, Transport, api, events::RtEvent, models::File},
};

#[derive(Clone)]
struct Scope {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    epoch: u64,
    generation: u64,
}
impl Scope {
    fn capture(core: &CoreAppState, team: &str, generation: u64) -> Option<Self> {
        if core.workspaces.get(team)?.rt_generation != generation {
            return None;
        }
        Some(Self {
            transport: core.transport.clone()?,
            client: core.client.clone(),
            session: core.session.as_ref()?.workspaces.get(team)?.clone(),
            epoch: core.account_epoch,
            generation,
        })
    }
    fn current(&self, core: &CoreAppState) -> bool {
        core.account_epoch == self.epoch
            && core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| w.rt_generation == self.generation)
    }
}
struct Pending {
    scope: Scope,
    worker: u64,
    revision: u64,
}
#[derive(Default)]
pub(crate) struct Tracker {
    serial: u64,
    scope: Option<(u64, Weak<Transport>)>,
    latest_events: HashMap<(String, String), u64>,
    pending: HashMap<(String, String), Pending>,
}
pub(crate) struct Read {
    scope: Scope,
    id: String,
    worker: u64,
    revision: u64,
}

fn changed_id(event: &RtEvent) -> Option<&str> {
    let RtEvent::Unknown(raw) = event else {
        return None;
    };
    if raw.kind != "file_change" {
        return None;
    }
    let top = raw.rest.get("file_id").and_then(|v| v.as_str());
    let nested = raw
        .rest
        .get("file")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str());
    if top.zip(nested).is_some_and(|(a, b)| a != b) {
        return None;
    }
    top.or(nested).filter(|id| !id.trim().is_empty())
}

/// Capture before a local network read; completed realtime reads still count.
pub(crate) fn snapshot_revision(shell: &ShellState) -> u64 {
    shell.file_events.serial
}
pub(crate) fn changed_since(shell: &ShellState, team: &str, id: &str, revision: u64) -> bool {
    let tracker = &shell.file_events;
    tracker.scope.as_ref().is_some_and(|(epoch, transport)| {
        *epoch == shell.core.account_epoch
            && transport.upgrade().is_some_and(|old| {
                shell
                    .core
                    .transport
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &old))
            })
    }) && tracker
        .latest_events
        .get(&(team.into(), id.into()))
        .is_some_and(|latest| *latest > revision)
}

/// A local verification may itself have begun before a newer remote event.
/// Keep pending work, but require a fresh read after this local snapshot lands.
pub(crate) fn invalidate(shell: &mut ShellState, team: &str, id: &str) {
    let tracker = &mut shell.file_events;
    if let Some(pending) = tracker.pending.get_mut(&(team.into(), id.into())) {
        tracker.serial = tracker.serial.wrapping_add(1);
        pending.revision = tracker.serial;
    }
}

pub(crate) fn prepare(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<Read> {
    if !matches!(event, RtEvent::Unknown(raw) if matches!(raw.kind.as_str(), "file_change" | "file_deleted"))
    {
        return None;
    }
    let scope = Scope::capture(&shell.core, team, generation)?;
    let deleted_id = match event {
        RtEvent::Unknown(raw) => policy::deleted_event(raw),
        _ => None,
    };
    let id = deleted_id.or_else(|| changed_id(event))?;
    // Retire canonical/preflight reads before applying this validated event.
    crate::canvas::checklist::invalidate(shell, team, id);
    let tracker = &mut shell.file_events;
    if tracker.scope.as_ref().is_none_or(|(epoch, transport)| {
        *epoch != scope.epoch
            || transport
                .upgrade()
                .is_none_or(|old| !Arc::ptr_eq(&old, &scope.transport))
    }) {
        tracker.latest_events.clear();
        tracker.pending.clear();
        tracker.scope = Some((scope.epoch, Arc::downgrade(&scope.transport)));
    }
    // Pending work from a replaced socket cannot suppress fresh reads.
    tracker.pending.retain(|_, p| p.scope.current(&shell.core));
    tracker.serial = tracker.serial.wrapping_add(1);
    let revision = tracker.serial;
    let key = (team.into(), id.into());
    tracker.latest_events.insert(key.clone(), revision);
    if deleted_id.is_some() {
        tracker.pending.remove(&key);
        return None;
    }
    if shell.core.workspaces.get(team)?.deleted_files.contains(id) {
        return None;
    }
    if let Some(pending) = tracker.pending.get_mut(&key) {
        pending.revision = revision;
        return None;
    }
    tracker.pending.insert(
        key,
        Pending {
            scope: scope.clone(),
            worker: revision,
            revision,
        },
    );
    Some(Read {
        scope,
        id: id.into(),
        worker: revision,
        revision,
    })
}

/// Merge verified names, size and type label only. Media/content/access metadata
/// and the open preview element remain unchanged by a partial response.
pub(crate) fn reconcile_metadata(shell: &mut ShellState, team: &str, file: &File) -> bool {
    let Some(id) = file.id.as_deref().filter(|id| !id.is_empty()) else {
        return false;
    };
    if policy::deleted(file)
        || shell
            .core
            .workspaces
            .get(team)
            .is_none_or(|w| w.deleted_files.contains(id))
    {
        return false;
    }
    if !shell.core.merge_file_display_metadata(team, file) {
        return false;
    }
    // A size/type-only response must not rename the viewer to its fallback ID.
    let title = file
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .or(file.name.as_deref().filter(|name| !name.trim().is_empty()))
        .map(|_| super_platinum_core::state::file_title(file));

    if shell.core.active_team.as_deref() == Some(team) {
        if let Some(title) = title
            && let Some(viewer) = &mut shell.viewer
            && viewer
                .file
                .as_ref()
                .is_some_and(|identity| identity.id == id)
        {
            viewer.name = title;
            // Missing optional metadata must not erase the known download name.
            if file
                .name
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty())
            {
                viewer.download_name = super_platinum_core::state::file_download_name(file);
            }
        }
        shell.refresh_from_core();
    }
    true
}

impl Read {
    fn owns(&self, shell: &ShellState) -> bool {
        self.scope.current(&shell.core)
            && shell
                .file_events
                .pending
                .get(&(self.scope.session.team_id.clone(), self.id.clone()))
                .is_some_and(|p| p.worker == self.worker)
    }

    /// Returns the single coalesced follow-up read, if a newer event arrived.
    pub(crate) fn finish(
        self,
        shell: &mut ShellState,
        result: Result<File, Error>,
    ) -> (Option<Self>, bool) {
        if !self.owns(shell) {
            return (None, false);
        }
        let key = (self.scope.session.team_id.clone(), self.id.clone());
        if shell.core.workspaces[&self.scope.session.team_id]
            .deleted_files
            .contains(&self.id)
        {
            shell.file_events.pending.remove(&key);
            return (None, false);
        }
        let pending = shell.file_events.pending.get(&key).unwrap();
        if pending.revision != self.revision {
            let revision = pending.revision;
            return (Some(Self { revision, ..self }), false);
        }
        shell.file_events.pending.remove(&key);
        let deleted = matches!(&result, Err(Error::Api(code)) if code == "file_deleted")
            || matches!(&result, Ok(file) if file.id.as_deref() == Some(&self.id) && policy::deleted(file));
        if deleted {
            shell
                .core
                .remove_file(&self.scope.session.team_id, &self.id);
            if shell.core.active_team.as_deref() == Some(&self.scope.session.team_id) {
                shell.refresh_from_core();
            }
            return (None, true);
        }
        let changed = match result {
            Ok(file) if file.id.as_deref() == Some(&self.id) => {
                reconcile_metadata(shell, &self.scope.session.team_id, &file)
            }
            // Missing access is not proof of deletion. Retain the warm cache.
            _ => false,
        };
        (None, changed)
    }

    pub(crate) async fn run(mut self, mut state: Signal<ShellState>) {
        loop {
            // Coalesce the initial event burst before spending a network call.
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            if !self.owns(&state.peek()) {
                return;
            }
            {
                let shell = state.peek();
                self.revision = shell.file_events.pending
                    [&(self.scope.session.team_id.clone(), self.id.clone())]
                    .revision;
            }
            let result = api::fetch_file(
                &self.scope.transport,
                &self.scope.client,
                &self.scope.session,
                &self.id,
            )
            .await;
            // Transport already retries safe reads. If those attempts exhausted
            // a rate limit, queued events still honor the final Retry-After.
            if let Err(Error::RateLimited { retry_after_secs }) = &result {
                tokio::time::sleep(std::time::Duration::from_secs(
                    retry_after_secs.unwrap_or(30).max(1),
                ))
                .await;
            }
            let team = self.scope.session.team_id.clone();
            let (next, changed) = self.finish(&mut state.write(), result);
            if changed {
                crate::bootstrap::persist_workspace(&state, &team);
            }
            let Some(next) = next else {
                return;
            };
            self = next;
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "file_events/display_metadata_tests.rs"]
mod display_metadata_tests;
