//! Compare poll metadata before cloning retained draft content.
use super::*;

#[derive(PartialEq, Eq)]
pub(crate) struct Signature {
    serial: u64,
    revision: Option<u64>,
    error: Option<Fault>,
    unsaved: bool,
    running: bool,
    blocked: Option<String>,
}
impl From<&Snapshot> for Signature {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            serial: snapshot.serial,
            revision: snapshot.view.stored.as_ref().map(|stored| stored.revision),
            error: snapshot.error,
            unsaved: snapshot.unsaved,
            running: snapshot.running,
            blocked: snapshot
                .view
                .blocked
                .as_ref()
                .map(|blocked| blocked.token.clone()),
        }
    }
}
impl Signature {
    fn matches(&self, state: &State) -> bool {
        self.serial == state.serial
            && self.revision == state.base.stored.as_ref().map(|stored| stored.revision)
            && self.error == state.error
            && self.unsaved == state.pending.is_some()
            && self.running == (state.running || state.settling)
            && self.blocked.as_deref()
                == state
                    .base
                    .blocked
                    .as_ref()
                    .map(|blocked| blocked.token.as_str())
    }
}

impl Editor {
    fn snapshot_if_changed(&self, previous: Option<&Signature>) -> Option<Snapshot> {
        // Compare and copy under the same lock: an async writer cannot change
        // the content between the metadata decision and its owned snapshot.
        let state = self.state.lock().unwrap();
        let retire = state.base.stored.is_none()
            && !state.running
            && !state.settling
            && state.pending.is_none()
            && state.error.is_none();
        // Integration retires released, idle writers BEFORE signature equality.
        // It needs their local content to preserve independent recovery rows.
        if !retire && previous.is_some_and(|previous| previous.matches(&state)) {
            return None;
        }
        Some(Snapshot::from_state(&state))
    }
}

impl Queue {
    pub fn changed_snapshots(
        &self,
        account: &str,
        epoch: u64,
        previous: &BTreeMap<DraftKey, Signature>,
    ) -> Vec<(DraftKey, Snapshot)> {
        if !self
            .scope
            .as_ref()
            .is_some_and(|(owner, visit)| owner == account && *visit == epoch)
        {
            return Vec::new();
        }
        self.editors
            .iter()
            .filter_map(|(key, editor)| {
                editor
                    .snapshot_if_changed(previous.get(key))
                    .map(|snapshot| (key.clone(), snapshot))
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "checkpoint_poll_tests.rs"]
mod tests;
