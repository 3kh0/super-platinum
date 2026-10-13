//! Per-client CRDT provenance. An omitted load field is not a new text history.
use super::*;
const MAX_SECTIONS: usize = 64;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct Key {
    workspace: String,
    user: String,
    file: String,
    thread: String,
    document: String,
    section: String,
}
impl Key {
    fn snapshot(snapshot: &CanvasCollabSnapshot, section: &Section) -> Result<Self, Error> {
        Ok(Self {
            workspace: snapshot.init.workspace_id().into(),
            user: snapshot
                .init
                .options()
                .string(33)?
                .ok_or(Error::Changed)?
                .into(),
            file: snapshot.file_id().into(),
            thread: snapshot.thread_id().into(),
            document: snapshot.document_id().into(),
            section: section.id().into(),
        })
    }
    pub(super) fn prepared(prepared: &PreparedCheckboxEdit) -> Self {
        Self {
            workspace: prepared.workspace.clone(),
            user: prepared.user.clone(),
            file: prepared.file.clone(),
            thread: prepared.thread.clone(),
            document: prepared.document.clone(),
            section: prepared.section.clone(),
        }
    }
}
#[derive(Clone)]
pub(super) struct Entry {
    source: Section,
    parent: Section,
    wire: RawMessage,
    server: bool,
    request: u64,
    usable: bool,
}
impl Entry {
    fn bytes(&self) -> usize {
        self.source.raw().raw().len() + self.parent.raw().raw().len() + self.wire.raw().len()
    }
    fn matches(&self, section: &Section, parent: &Section, now: u64) -> Result<bool, Error> {
        if !self.usable || self.parent != *parent {
            return Ok(false);
        }
        if self.server {
            validate_server_crdt(
                &self.wire,
                section.id(),
                section.sequence()?.ok_or(Error::Unsupported)?,
                now,
            )?;
            Ok(association(&self.source)? == association(section)?)
        } else {
            Ok(self.source == *section)
        }
    }
}
/// ACK→canonical fetch captures differ only in 18/103/104, plus load omission
/// of CRDT38. Preserve every other unknown field and exact canonical revision.
fn association(section: &Section) -> Result<RawMessage, Error> {
    let mut raw = section.raw().clone();
    for tag in [18, 38, 103, 104] {
        raw = raw.remove(tag)?;
    }
    Ok(raw)
}
pub(super) struct Plan {
    key: Key,
    entry: Entry,
    pub session: String,
    pub rotated: bool,
}
impl Plan {
    pub fn wire(&self) -> &RawMessage {
        &self.entry.wire
    }
}
impl EditSession {
    pub(super) fn plan(
        &self,
        snapshot: &CanvasCollabSnapshot,
        section: &Section,
        parent: &Section,
        now: u64,
    ) -> Result<Plan, Error> {
        let key = Key::snapshot(snapshot, section)?;
        let existing = self.retained.get(&key);
        let mut rotate = false;
        let candidate = if let Some(server) = section.crdt()? {
            Some((server, true))
        } else if let Some(existing) = existing {
            if existing.matches(section, parent, now).unwrap_or(false) {
                Some((existing.wire.clone(), existing.server))
            } else {
                rotate = true;
                None
            }
        } else {
            None
        };
        let session = if rotate {
            EditSession::new().id
        } else {
            self.id.clone()
        };
        let (wire, server) = match candidate {
            Some(candidate) => candidate,
            None => {
                let local = bootstrap::LocalCrdt::bootstrap(section, &session, now)?;
                (local.for_section(section, &session)?.clone(), false)
            }
        };
        let entry = Entry {
            source: section.clone(),
            parent: parent.clone(),
            wire,
            server,
            request: 0,
            usable: true,
        };
        if entry.bytes() > MAX_BYTES {
            return Err(Error::Unsupported);
        }
        let retained_bytes: usize = self
            .retained
            .iter()
            .filter(|(old, _)| **old != key)
            .map(|(_, entry)| entry.bytes())
            .sum();
        let capacity = !self.retained.contains_key(&key) && self.retained.len() >= MAX_SECTIONS
            || retained_bytes.saturating_add(entry.bytes()) > MAX_BYTES;
        if capacity && !rotate {
            if section.raw().has(38) {
                return Err(Error::Unsupported);
            }
            // A new client identity is a real reset boundary. Never evict a
            // history and bootstrap sequence one under its previously used actor.
            let reset = EditSession::new();
            let mut plan = reset.plan(snapshot, section, parent, now)?;
            plan.rotated = true;
            // No old pending receipt can be adopted into this new identity.
            return Ok(plan);
        }
        Ok(Plan {
            key,
            entry,
            session,
            rotated: rotate,
        })
    }
    pub(super) fn commit_plan(&mut self, mut plan: Plan, sequence: u64) {
        if plan.rotated {
            self.id = plan.session;
            self.sequence = 0;
            self.local.clear();
            self.retained.clear();
            self.text_retained.clear();
        }
        plan.entry.request = sequence;
        self.retained.insert(plan.key, plan.entry);
        self.sequence = sequence;
    }

    /// Adopt only a direct, sealed receipt for this client's reserved request.
    /// A late receipt still proves acceptance to its UI owner after rotation,
    /// but cannot replace the new client's CRDT state.
    pub fn remember_accepted(&mut self, accepted: &AcceptedCheckbox) -> Result<(), Error> {
        if accepted.session != self.id {
            return Err(Error::Changed);
        }
        let occupied: usize = self
            .retained
            .iter()
            .filter(|(key, _)| **key != accepted.key)
            .map(|(_, entry)| entry.bytes())
            .sum();
        let entry = self.retained.get_mut(&accepted.key).ok_or(Error::Changed)?;
        if entry.request != accepted.sequence {
            return Err(Error::Changed);
        }
        // Keep the used-history identity even if its receipt cannot supply a
        // supported session. The next fresh explicit intent must rotate actors.
        entry.usable = false;
        let crdt = accepted.section.crdt()?.ok_or(Error::Unsupported)?;
        let sequence = accepted.section.sequence()?.ok_or(Error::Unsupported)?;
        validate_server_crdt(&crdt, accepted.section.id(), sequence, 0)?;
        if crdt.uint(3)? != Some(sequence) {
            return Err(Error::Unsupported);
        }
        // This bounded attribute-only path supports unchanged text CRDT. An
        // acknowledgement introducing merges/heads needs its dedicated reducer.
        for tag in [1, 2, 5, 6, 8] {
            if encoded(&crdt, tag)? != encoded(&entry.wire, tag)? {
                return Err(Error::Unsupported);
            }
        }
        let updated = Entry {
            source: accepted.baseline.clone(),
            parent: accepted.parent.clone(),
            wire: crdt,
            server: true,
            request: accepted.sequence,
            usable: true,
        };
        if occupied.saturating_add(updated.bytes()) > MAX_BYTES {
            return Err(Error::Unsupported);
        }
        *entry = updated;
        Ok(())
    }
}

fn encoded(raw: &RawMessage, tag: u32) -> Result<Vec<&[u8]>, Error> {
    raw.fields()
        .iter()
        .filter(|field| field.tag() == tag)
        .map(|field| raw.encoded_field(field).map_err(Error::from))
        .collect()
}

#[cfg(test)]
mod tests;
