//! Hosted content belongs to the existing Editor actor and Account inventory.
use super::owned_route::{Held, o, ui};
use super::ownership::Tracked;
use super::*;
pub(crate) use durable::owned_store::HostedSelection as Selection;
use serde_json::Value;
type Saved = Arc<Held<o::HostedDraftInput, Option<o::HostedDraft>>>;
mod flight;
mod retirement;
mod retirement_retry;
#[derive(Default)]
pub(crate) struct Composer {
    state: Tracked<State>,
    gate: tokio::sync::Mutex<()>,
}
#[derive(Default)]
struct State {
    loaded: bool,
    load: Option<Arc<Held<String, Arc<durable::Loaded>>>>,
    historical: Vec<o::HostedRecord>,
    completed: Vec<o::HostedReceipt>,
    retirement: Option<Arc<Held<o::HostedRetirement, o::HostedRetired>>>,
    retirement_operations: Option<o::Operations>,
    retirement_retry: retirement_retry::Retry,
    current: Option<o::HostedDraft>,
    selection: Selection,
    version: u64,
    persisted: u64,
    save: Option<(u64, Saved)>,
    flight: Option<flight::Flight>,
}
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub selection: Selection,
    pub version: u64,
    pub saved: bool,
    pub loaded: bool,
    pub sending: bool,
    pub completed: Vec<o::HostedReceipt>,
    pub can_retire: bool,
    pub retry_retirement: bool,
    pub retry_load: bool,
    pub retry_saved: bool,
    pub retry_result: bool,
}
impl Composer {
    pub(super) fn ownership_stamp(&self) -> Result<ownership::Stamp, ownership::Error> {
        self.state.observe(|_| ()).map(|(stamp, ())| stamp)
    }
    pub(super) fn validate_ownership(
        &self,
        stamp: &ownership::Stamp,
    ) -> Result<(), ownership::Error> {
        self.state.validate(stamp)
    }
    pub(crate) fn snapshot(&self) -> Result<Snapshot, durable::Error> {
        let s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        Ok(Snapshot {
            selection: s.selection.clone(),
            version: s.version,
            saved: s.loaded && s.version == s.persisted && s.save.is_none(),
            loaded: s.loaded,
            sending: s.flight.is_some()
                || !s.historical.is_empty()
                || !s.completed.is_empty()
                || s.retirement.is_some(),
            completed: s.completed.clone(),
            can_retire: s.loaded
                && s.save.is_none()
                && s.historical.is_empty()
                && s.retirement.is_none()
                && s.flight.as_ref().is_none_or(|f| f.acknowledged()),
            retry_retirement: s.retirement_retry.available(s.retirement.as_ref()),
            retry_load: s.load.as_ref().is_some_and(|j| {
                j.job()
                    .is_ok_and(|j| j.state() == o::JobState::Failed(durable::Error::Storage))
            }),
            retry_saved: s.save.as_ref().is_some_and(|(_, j)| {
                j.job()
                    .is_ok_and(|j| j.state() == o::JobState::Failed(durable::Error::Storage))
            }),
            retry_result: s.flight.as_ref().is_some_and(|f| f.storage_failed()),
        })
    }
    pub(crate) fn insert(&self, expected: u64, raw: Value) -> Result<u64, durable::Error> {
        self.change(expected, |set| set.insert(raw))
    }
    pub(crate) fn remove(&self, expected: u64, id: &str) -> Result<u64, durable::Error> {
        self.change(expected, |set| set.remove(id))
    }
    pub(crate) fn reorder(&self, expected: u64, ids: &[String]) -> Result<u64, durable::Error> {
        self.change(expected, |set| set.reorder(ids))
    }
    fn change(
        &self,
        expected: u64,
        edit: impl FnOnce(&Selection) -> Result<Selection, durable::Error>,
    ) -> Result<u64, durable::Error> {
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !s.loaded
            || s.version != expected
            || s.save.is_some()
            || s.flight.is_some()
            || !s.historical.is_empty()
            || !s.completed.is_empty()
            || s.retirement.is_some()
        {
            return Err(durable::Error::Pending);
        }
        let next = edit(&s.selection)?;
        if next == s.selection {
            return Ok(s.version);
        }
        let version = s
            .version
            .checked_add(1)
            .ok_or(durable::Error::Unsupported)?;
        s.selection = next;
        s.version = version;
        Ok(version)
    }
    pub(crate) async fn load(&self, editor: &editor::Editor) -> Result<Snapshot, durable::Error> {
        if !std::ptr::eq(self, editor.hosted.as_ref()) {
            return Err(durable::Error::Stale);
        }
        let _gate = self.gate.lock().await;
        let route = editor
            .owner
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        let job = {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if s.loaded {
                return self.loaded_snapshot(&s);
            }
            s.load
                .get_or_insert_with(|| Held::new(route.operations.inventory()))
                .clone()
        };
        self.load_result(editor, job).await
    }
    pub(crate) async fn retry_load(
        &self,
        editor: &editor::Editor,
    ) -> Result<Snapshot, durable::Error> {
        if !std::ptr::eq(self, editor.hosted.as_ref()) {
            return Err(durable::Error::Stale);
        }
        let _gate = self.gate.lock().await;
        let job = {
            let s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if s.loaded {
                return Err(durable::Error::Stale);
            }
            s.load.clone().ok_or(durable::Error::Pending)?
        };
        job.job()
            .map_err(ui)?
            .retry_inventory_storage()
            .map_err(ui)?;
        self.load_result(editor, job).await
    }
    async fn load_result(
        &self,
        editor: &editor::Editor,
        job: Arc<Held<String, Arc<durable::Loaded>>>,
    ) -> Result<Snapshot, durable::Error> {
        let inventory = job.value().await.map_err(ui)??;
        let value = inventory
            .hosted()
            .drafts()
            .iter()
            .find(|d| d.scope() == editor.scope())
            .cloned();
        // Publish the actual returned draft before acknowledging its worker slot.
        {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            s.current = value.clone();
            s.completed = inventory
                .hosted()
                .completed()
                .iter()
                .filter(|r| r.record().scope() == editor.scope())
                .cloned()
                .collect();
            s.historical = inventory
                .hosted()
                .claims()
                .iter()
                .filter(|r| r.scope() == editor.scope())
                .cloned()
                .collect();
            s.selection = value
                .as_ref()
                .map(|d| Selection::from_input(d.input()))
                .or_else(|| {
                    s.historical
                        .first()
                        .map(|r| Selection::from_input(r.input()))
                })
                .or_else(|| {
                    s.completed
                        .first()
                        .map(|r| Selection::from_input(r.record().input()))
                })
                .unwrap_or_default();
        }
        let (_, ack) = job.job().map_err(ui)?.observe_inventory().map_err(ui)?;
        ack.acknowledge().map_err(|(e, _)| ui(e))?;
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        s.loaded = true;
        s.load = None;
        self.loaded_snapshot(&s)
    }
    fn loaded_snapshot(&self, s: &State) -> Result<Snapshot, durable::Error> {
        Ok(Snapshot {
            selection: s.selection.clone(),
            version: s.version,
            saved: s.loaded && s.version == s.persisted && s.save.is_none(),
            loaded: s.loaded,
            sending: s.flight.is_some()
                || !s.historical.is_empty()
                || !s.completed.is_empty()
                || s.retirement.is_some(),
            completed: s.completed.clone(),
            can_retire: s.loaded
                && s.save.is_none()
                && s.historical.is_empty()
                && s.retirement.is_none()
                && s.flight.as_ref().is_none_or(|f| f.acknowledged()),
            retry_retirement: s.retirement_retry.available(s.retirement.as_ref()),
            retry_load: s.load.as_ref().is_some_and(|j| {
                j.job()
                    .is_ok_and(|j| j.state() == o::JobState::Failed(durable::Error::Storage))
            }),
            retry_saved: s.save.as_ref().is_some_and(|(_, j)| {
                j.job()
                    .is_ok_and(|j| j.state() == o::JobState::Failed(durable::Error::Storage))
            }),
            retry_result: s.flight.as_ref().is_some_and(|f| f.storage_failed()),
        })
    }
    pub(crate) async fn persist(
        &self,
        editor: &Arc<editor::Editor>,
        stream: durable::Stream,
    ) -> Result<Snapshot, durable::Error> {
        let _gate = self.gate.lock().await;
        let route = editor
            .owner
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        let existing = {
            let s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if !s.loaded
                || s.flight.is_some()
                || !s.historical.is_empty()
                || !s.completed.is_empty()
                || s.retirement.is_some()
            {
                return Err(durable::Error::Pending);
            }
            s.save.clone()
        };
        let (version, job) = if let Some(existing) = existing {
            existing
        } else {
            // Give even an initially empty file-only caption a real accepted
            // writer serial. This is the same actor, never a second writer.
            if editor.snapshot().serial == 0 {
                let local = editor.snapshot().view.local().clone();
                editor.submit(&local)?;
            }
            editor.flush().await?;
            let _editor_gate = editor.gate.lock().await;
            editor.owner.check_stream(&stream)?;
            let capture = editor.capture(Visit::new())?;
            if capture.pending.is_some() {
                return Err(durable::Error::Pending);
            }
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            let clear = s.selection.sources().is_empty();
            let set = if clear {
                s.current
                    .as_ref()
                    .map(|d| Selection::from_input(d.input()))
                    .ok_or(durable::Error::Stale)?
            } else {
                s.selection.clone()
            };
            let input = set.input(&capture.review)?;
            let input = Arc::new(o::HostedPrepared::new(
                stream,
                capture.review,
                capture.writer,
                capture.serial,
                input,
            )?);
            let job = Held::new(route.operations.save_hosted_draft(o::HostedDraftInput {
                input,
                prior: s.current.clone(),
                clear,
            }));
            let version = s.version;
            s.save = Some((version, job.clone()));
            (version, job)
        };
        self.saved_result(version, job).await
    }
    pub(crate) async fn retry_saved(&self) -> Result<Snapshot, durable::Error> {
        let _gate = self.gate.lock().await;
        let (version, job) = self
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .save
            .clone()
            .ok_or(durable::Error::Pending)?;
        job.job().map_err(ui)?.retry_hosted_storage().map_err(ui)?;
        self.saved_result(version, job).await
    }
    async fn saved_result(&self, version: u64, job: Saved) -> Result<Snapshot, durable::Error> {
        let actual = job.value().await.map_err(ui)??;
        {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            s.current = actual.clone();
        }
        let (_, ack) = job.job().map_err(ui)?.observe_hosted_saved().map_err(ui)?;
        ack.acknowledge().map_err(|(e, _)| ui(e))?;
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if s.version != version {
            return Err(durable::Error::Stale);
        }
        s.persisted = version;
        s.save = None;
        self.loaded_snapshot(&s)
    }
}
impl editor::Editor {
    pub(crate) fn hosted(&self) -> Arc<Composer> {
        self.hosted.clone()
    }
}

#[cfg(test)]
mod tests;
