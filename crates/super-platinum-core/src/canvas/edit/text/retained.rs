use super::*;
use automerge::ReadDoc;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub workspace: String,
    pub user: String,
    pub file: String,
    pub thread: String,
    pub document: String,
    pub section: String,
}
impl Key {
    pub(super) fn capture(
        snapshot: &CanvasCollabSnapshot,
        section: &Section,
    ) -> Result<Self, Error> {
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
}
pub(crate) struct Entry {
    pub source: Section,
    pub wire: RawMessage,
    pub request: u64,
    pub usable: bool,
}
pub(super) struct Plan {
    pub key: Key,
    pub source: Section,
    pub wire: RawMessage,
    pub session: String,
    pub rotated: bool,
}

pub(super) fn association(section: &Section) -> Result<RawMessage, Error> {
    let mut raw = section.raw().clone();
    for tag in [18, 38, 103, 104] {
        raw = raw.remove(tag)?;
    }
    // Captured text ACK→full-load differs only here after the response fields
    // above: an explicit empty Parents message becomes absent. Both represent
    // the same top-level section; retain every nonempty/unknown parent field.
    if section
        .parents()?
        .is_some_and(|parents| parents.fields().is_empty())
    {
        raw = raw.remove(13)?;
    }
    Ok(raw)
}

impl EditSession {
    pub(super) fn text_plan(
        &self,
        key: &Key,
        section: &Section,
        seed: &TextDraft,
        now: u64,
    ) -> Result<Plan, Error> {
        let retained = self.text_retained.get(key);
        let mut rotated = false;
        let wire = if let Some(wire) = section.crdt()? {
            crdt::validate(&wire, section, seed, Some(now))?;
            Some(wire)
        } else if let Some(entry) = retained {
            // Omitted-38 association is not established for the newly admitted
            // modern/base0 cohort. Never replace its known history by bootstrap.
            if entry.wire.uint(2)? == Some(0) || entry.wire.has(10) {
                return Err(Error::Unsupported);
            }
            if entry.usable
                && association(&entry.source)? == association(section)?
                && crdt::validate(&entry.wire, section, seed, Some(now)).is_ok()
            {
                Some(entry.wire.clone())
            } else {
                rotated = true;
                None
            }
        } else {
            None
        };
        if self.text_retained.len() >= 64 && retained.is_none()
            || self
                .text_retained
                .values()
                .map(|e| e.wire.raw().len() + e.source.raw().raw().len())
                .sum::<usize>()
                >= 8 * 1024 * 1024
        {
            if section.raw().has(38) {
                return Err(Error::Unsupported);
            }
            rotated = true;
        }
        let session = if rotated {
            EditSession::new().id
        } else {
            self.id.clone()
        };
        let wire = match wire {
            Some(wire) if !rotated => wire,
            _ => crdt::bootstrap(section, seed, &session, now)?,
        };
        Ok(Plan {
            key: key.clone(),
            source: section.clone(),
            wire,
            session,
            rotated,
        })
    }
    pub(super) fn commit_text_plan(
        &mut self,
        plan: Plan,
        prepared: &PreparedTextEdit,
        wire: RawMessage,
    ) {
        if plan.rotated {
            self.id = plan.session;
            self.local.clear();
            self.retained.clear();
            self.text_retained.clear();
        }
        self.sequence = prepared.sequence;
        // A used local actor must not bootstrap the same section again if the
        // outcome or its canonical CRDT is unavailable. Only sealed ACK adoption
        // makes the entry reusable; the next fresh intent otherwise rotates.
        self.text_retained.insert(
            plan.key,
            Entry {
                source: plan.source,
                wire,
                request: prepared.sequence,
                usable: false,
            },
        );
    }
    pub fn remember_text_accepted(&mut self, accepted: &AcceptedText) -> Result<(), Error> {
        if self.id != accepted.session {
            return Err(Error::Changed);
        }
        let entry = self
            .text_retained
            .get_mut(&accepted.key)
            .ok_or(Error::Changed)?;
        if entry.request != accepted.sequence {
            return Err(Error::Changed);
        }
        entry.usable = false;
        let wire = accepted.section.crdt()?.ok_or(Error::Unsupported)?;
        let document = crdt::validate(&wire, &accepted.section, &accepted.desired, Some(0))?;
        if wire.uint(3)? != accepted.section.sequence()? {
            return Err(Error::Unsupported);
        }
        let sent_head = entry
            .wire
            .string(12)?
            .ok_or(Error::Unsupported)?
            .parse()
            .map_err(|_| Error::Unsupported)?;
        if document.get_change_by_hash(&sent_head).is_none() {
            return Err(Error::Unconfirmed);
        }
        *entry = Entry {
            source: accepted.baseline.clone(),
            wire,
            request: accepted.sequence,
            usable: true,
        };
        Ok(())
    }
}
