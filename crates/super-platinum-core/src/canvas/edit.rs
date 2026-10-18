//! A revision-bound update for one existing, ordinary checklist item.
//! The HTML export is never used as an editing model. No transport or retries.
use std::collections::HashMap;
use std::sync::Arc;

use super::wire::{self, EditAck, EncodedField as F, RawMessage, Section};
use crate::slack::api::CanvasCollabSnapshot;

pub mod annotation;
mod batch;
pub mod bootstrap;
mod retained;
pub mod structure;
pub mod text;
mod title;
use title::CanonicalTitle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("The canvas data is invalid")]
    Wire(#[from] wire::Error),
    #[error("This canvas is not editable")]
    ReadOnly,
    #[error("This canvas content needs a newer editor")]
    Unsupported,
    #[error("This canvas content changed; refresh before editing")]
    Changed,
    #[error("The edit session expired; refresh before editing")]
    Expired,
    #[error("The canvas content has not changed")]
    Unchanged,
    #[error("The edit acknowledgement did not confirm this change")]
    Unconfirmed,
}

/// Account/session-owned, never persisted or reused after an account replacement.
pub struct EditSession {
    id: String,
    sequence: u64,
    local: HashMap<String, u64>,
    retained: HashMap<retained::Key, retained::Entry>,
    text_retained: HashMap<text::retained::Key, text::retained::Entry>,
}
impl Default for EditSession {
    fn default() -> Self {
        Self::new()
    }
}
impl EditSession {
    pub fn new() -> Self {
        // Stock Slack concatenates three random four-digit hex strings.
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        Self {
            id,
            sequence: 0,
            local: HashMap::new(),
            retained: HashMap::new(),
            text_retained: HashMap::new(),
        }
    }
}

pub struct CheckboxIntent {
    file: String,
    thread: String,
    document: String,
    section: Section,
    parent: Arc<Section>,
    title: Arc<CanonicalTitle>,
    workspace: String,
    user: String,
    desired: bool,
}
impl CheckboxIntent {
    pub fn capture(
        snapshot: &CanvasCollabSnapshot,
        section: &str,
        desired: bool,
        now_usec: u64,
    ) -> Result<Self, Error> {
        Self::capture_batch(snapshot, &[(section, desired)], now_usec)
            .pop()
            .expect("one target yields one result")
    }

    /// Preflight against a fresh canonical read, before reserving any counters.
    pub fn prepare(
        &self,
        fresh: &CanvasCollabSnapshot,
        session: &mut EditSession,
        now_usec: u64,
    ) -> Result<PreparedCheckboxEdit, Error> {
        if fresh.file_id() != self.file
            || fresh.thread_id() != self.thread
            || fresh.document_id() != self.document
            || fresh.init.workspace_id() != self.workspace
            || fresh.init.options().string(33)? != Some(self.user.as_str())
        {
            return Err(Error::Changed);
        }
        let (section, parent) = eligible(fresh, self.section.id(), now_usec)?;
        self.title.revalidate(fresh)?;
        let title = Arc::clone(&self.title);
        if section.sequence()? != self.section.sequence()?
            || parent.sequence()? != self.parent.sequence()?
            || section.checked()? != self.section.checked()?
        {
            return Err(Error::Changed);
        }
        // A revision alone must not bless inconsistent canonical bytes.
        for tag in [12, 13, 16] {
            if section.raw().bytes(tag)? != self.section.raw().bytes(tag)? {
                return Err(Error::Changed);
            }
        }
        if parent.raw().bytes(13)? != self.parent.raw().bytes(13)? {
            return Err(Error::Changed);
        }
        let plan = session.plan(fresh, section, parent, now_usec)?;
        let previous = if plan.rotated { 0 } else { session.sequence };
        let next = previous.checked_add(1).ok_or(Error::Expired)?;
        let local = if plan.rotated {
            0
        } else {
            session.local.get(section.id()).copied().unwrap_or(0)
        }
        .checked_add(1)
        .ok_or(Error::Expired)?;
        let trace = format!("{}-{}", plan.session, previous);
        let attrs = section
            .attributes()?
            .ok_or(Error::Unsupported)?
            .replace_fields(2, &[F::uint(2, u64::from(self.desired))?])?;
        // Entity.toPb rebuilds Parents from live ancestors; container_id belongs
        // to the canonical model and is not copied into this request shape.
        let parents =
            RawMessage::from_fields(&[F::bytes(1, parent.id().as_bytes())?, F::uint(4, 7)?])?;
        let crdt_expiry = plan.wire().uint(4)?;
        let crdt = plan.wire().remove(4)?.remove(7)?;
        let update = RawMessage::from_fields(&[
            F::bytes(1, section.id().as_bytes())?,
            F::uint(2, section.sequence()?.ok_or(Error::Unsupported)?)?,
            F::uint(9, 0)?,
            F::message(13, &parents)?,
            F::message(16, &attrs)?,
            F::uint(19, 1)?,
            F::uint(20, local)?,
            F::bytes(33, trace.as_bytes())?,
            F::message(38, &crdt)?,
            F::uint(39, if self.desired { 2 } else { 3 })?,
        ])?;
        let data = RawMessage::from_fields(&[
            F::message(1, &update)?,
            F::message(14, title.rich_text())?,
        ])?;
        let prepared_session = plan.session.clone();
        session.commit_plan(plan, next);
        session.local.insert(section.id().to_owned(), local);
        Ok(PreparedCheckboxEdit {
            file: self.file.clone(),
            thread: self.thread.clone(),
            document: self.document.clone(),
            section: section.id().to_owned(),
            base_sequence: section.sequence()?.ok_or(Error::Unsupported)?,
            desired: self.desired,
            session: prepared_session,
            sequence: next,
            data,
            crdt_expiry,
            canonical_section: section.clone(),
            canonical_parent: parent.clone(),
            title,
            workspace: self.workspace.clone(),
            user: self.user.clone(),
        })
    }
}

/// One explicit save attempt. Deliberately not Clone: callers retain an intent,
/// not a replayable request, when delivery is uncertain.
pub struct PreparedCheckboxEdit {
    file: String,
    thread: String,
    document: String,
    section: String,
    base_sequence: u64,
    desired: bool,
    session: String,
    sequence: u64,
    data: RawMessage,
    // Selected server provenance survives the omission of wire expiry from
    // the outgoing update and of CRDT38 from a fresh canonical load.
    crdt_expiry: Option<u64>,
    canonical_section: Section,
    canonical_parent: Section,
    title: Arc<CanonicalTitle>,
    workspace: String,
    user: String,
}
impl PreparedCheckboxEdit {
    pub fn form_title(&self) -> &str {
        self.title.form_title()
    }
    pub fn workspace_id(&self) -> &str {
        &self.workspace
    }
    pub fn user_id(&self) -> &str {
        &self.user
    }
    /// Recheck the bound read immediately before constructing its one-attempt
    /// request. No changed canonical input or authenticated owner may be swapped
    /// in after preparation, even if file and section IDs happen to match.
    pub(crate) fn validate_snapshot(
        &self,
        fresh: &CanvasCollabSnapshot,
        now_usec: u64,
    ) -> Result<(), Error> {
        if fresh.file_id() != self.file
            || fresh.thread_id() != self.thread
            || fresh.document_id() != self.document
            || fresh.init.workspace_id() != self.workspace
            || fresh.init.options().string(33)? != Some(self.user.as_str())
        {
            return Err(Error::Changed);
        }
        if self
            .crdt_expiry
            .is_some_and(|expiry| expiry <= now_usec || expiry > i64::MAX as u64)
        {
            return Err(Error::Expired);
        }
        let (section, parent) = eligible(fresh, &self.section, now_usec)?;
        if section != &self.canonical_section || parent != &self.canonical_parent {
            return Err(Error::Changed);
        }
        self.title.revalidate(fresh)
    }
    pub fn document_data(&self) -> &RawMessage {
        &self.data
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn target_section(&self) -> &str {
        &self.section
    }
    pub fn base_sequence(&self) -> u64 {
        self.base_sequence
    }
    pub fn desired(&self) -> bool {
        self.desired
    }
    pub fn file_id(&self) -> &str {
        &self.file
    }
    pub fn thread_id(&self) -> &str {
        &self.thread
    }
    pub fn document_id(&self) -> &str {
        &self.document
    }

    /// Only use with the direct response to this request. A later read matching
    /// the boolean is not proof that this particular attempt was accepted.
    pub fn validate_ack(&self, ack: &EditAck) -> Result<AcceptedCheckbox, Error> {
        if ack.changed != Some(true)
            || ack.is_new_edit_session == Some(true)
            || ack.cell_data.is_some()
            || ack.import_sections.is_some()
            || ack.mount_points.is_some()
        {
            return Err(Error::Unconfirmed);
        }
        let data = ack.final_data.as_ref().ok_or(Error::Unconfirmed)?;
        if data.sections().len() != 1 {
            // The receipt type below owns one canonical section. Do not silently
            // discard additional server updates while reporting reconciliation.
            return Err(Error::Unconfirmed);
        }
        if data.document_id()?.is_some_and(|id| id != self.document) {
            return Err(Error::Unconfirmed);
        }
        let section = data
            .sections()
            .iter()
            .find(|section| section.id() == self.section)
            .ok_or(Error::Unconfirmed)?;
        if section.document_id()? != Some(self.document.as_str())
            || section.thread_id()? != Some(self.thread.as_str())
            || section.deleted()? == Some(true)
            || section.checked()? != Some(self.desired)
            || section
                .sequence()?
                .is_none_or(|sequence| sequence <= self.base_sequence)
        {
            return Err(Error::Unconfirmed);
        }
        if let Some(crdt) = section.crdt()? {
            if crdt.boolean(7)? == Some(true) {
                return Err(Error::Unconfirmed);
            }
        }
        Ok(AcceptedCheckbox {
            key: retained::Key::prepared(self),
            baseline: self.canonical_section.merge_partial(section)?,
            parent: self.canonical_parent.clone(),
            section: section.clone(),
            session: self.session.clone(),
            sequence: self.sequence,
        })
    }
}
pub struct AcceptedCheckbox {
    key: retained::Key,
    baseline: Section,
    parent: Section,
    section: Section,
    session: String,
    sequence: u64,
}
impl AcceptedCheckbox {
    pub fn section(&self) -> &Section {
        &self.section
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

fn eligible<'a>(
    snapshot: &'a CanvasCollabSnapshot,
    id: &str,
    now: u64,
) -> Result<(&'a Section, &'a Section), Error> {
    eligible_document(snapshot)?;
    eligible_section(snapshot, id, now, |id| snapshot.section(id))
}

fn eligible_document(snapshot: &CanvasCollabSnapshot) -> Result<(), Error> {
    // Thread.readOnlyIgnoringEditsDisabled uses reachable permissions, not
    // merely a direct grant. ALL=0 and EDIT_DOCUMENT=13 in the captured enum.
    let permits = snapshot.thread.uints(23)?;
    if snapshot.thread.boolean(119)? != Some(true)
        || snapshot.thread.boolean(303)? == Some(true)
        || !permits.iter().any(|permit| matches!(permit, 0 | 13))
    {
        return Err(Error::ReadOnly);
    }
    if snapshot
        .file
        .extra
        .get("editable")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || snapshot.document.boolean(3)? == Some(true)
        || snapshot.document.boolean(12)? == Some(false)
        || snapshot.document.boolean(41)? == Some(true)
    {
        return Err(Error::ReadOnly);
    }
    for tag in [9, 26] {
        if snapshot
            .document
            .bytes(tag)?
            .is_some_and(|bytes| !bytes.is_empty())
        {
            return Err(Error::ReadOnly);
        }
    }
    Ok(())
}

fn eligible_section<'a>(
    snapshot: &CanvasCollabSnapshot,
    id: &str,
    now: u64,
    lookup: impl Fn(&str) -> Option<&'a Section>,
) -> Result<(&'a Section, &'a Section), Error> {
    let section = lookup(id).ok_or(Error::Changed)?;
    let seq = section.sequence()?.ok_or(Error::Unsupported)?;
    if seq == 0
        || seq > i64::MAX as u64
        || section.deleted()? == Some(true)
        || section.read_only()? == Some(true)
        || section.document_id()? != Some(snapshot.document_id())
        || section.thread_id()? != Some(snapshot.thread_id())
        || section.section_type()?.unwrap_or(0) != 0
        || section.style()?.unwrap_or(0) != 0
        || section.raw().boolean(29)? == Some(true)
        || section.raw().has(15)
    {
        return Err(Error::Unsupported);
    }
    let parents = section.parents()?.ok_or(Error::Unsupported)?;
    let parent_id = parents.string(1)?.ok_or(Error::Unsupported)?;
    let parent = lookup(parent_id).ok_or(Error::Unsupported)?;
    if parent.style()? != Some(7)
        || parent.document_id()? != Some(snapshot.document_id())
        || parent.thread_id()? != Some(snapshot.thread_id())
        || parent.deleted()? == Some(true)
        || parent.read_only()? == Some(true)
        || parent.parents()?.is_some_and(|parents| parents.has(1))
    {
        return Err(Error::Unsupported);
    }
    let Some(crdt) = section.crdt()? else {
        return if bootstrap::supports(section) {
            Ok((section, parent))
        } else {
            Err(Error::Unsupported)
        };
    };
    validate_server_crdt(&crdt, id, seq, now)?;

    Ok((section, parent))
}

fn validate_server_crdt(crdt: &RawMessage, id: &str, seq: u64, now: u64) -> Result<(), Error> {
    // A present server session is never replaced with locally synthesized
    // state to bypass expiry, validation errors, or unsupported session fields.
    for field in crdt.fields().iter().filter(|field| field.tag() == 6) {
        let head = RawMessage::decode(crdt.encoded_field(field)?)?;
        if head.string(6)?.is_none_or(|value| {
            value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        }) {
            return Err(Error::Unsupported);
        }
    }
    if crdt
        .fields()
        .iter()
        .any(|field| !matches!(field.tag(), 1..=8))
        || crdt.string(1)? != Some(id)
        || crdt.boolean(7)? == Some(true)
        || crdt.uint(2)?.is_none_or(|base| base == 0 || base > seq)
        || crdt.uint(3)?.is_none_or(|saved| saved > seq)
        || crdt.bytes(5)?.is_none_or(|doc| doc.is_empty())
        || !crdt.has(6)
        || crdt.string(8)?.is_none_or(str::is_empty)
    {
        return Err(Error::Unsupported);
    }
    if crdt
        .uint(4)?
        .is_none_or(|expiry| expiry <= now || expiry > i64::MAX as u64)
    {
        return Err(Error::Expired);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
