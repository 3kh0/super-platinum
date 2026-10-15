//! Canonical, single-paragraph authoring. Never derives writes from HTML export.
//!
//! RTML is the stored text representation; Automerge stores its escaped text and
//! UTF-16 marks separately. Unsupported target markup must not be flattened.
use super::{CanonicalTitle, CanvasCollabSnapshot, EditSession, Error, F, RawMessage, Section};
use std::sync::Arc;

pub(super) mod crdt;
mod draft;
mod prepared;
pub(super) mod retained;
mod rich;
pub use draft::{TextDraft, TextRun, TextStyle};
pub use prepared::{AcceptedText, PreparedTextEdit};

pub struct TextIntent {
    key: retained::Key,
    section: Section,
    title: Arc<CanonicalTitle>,
    seed: TextDraft,
}
impl TextIntent {
    pub fn capture(
        snapshot: &CanvasCollabSnapshot,
        section: &str,
        now_usec: u64,
    ) -> Result<Self, Error> {
        Self::capture_batch(snapshot, &[section], now_usec)
            .pop()
            .ok_or(Error::Unsupported)?
    }

    /// One permission/title pass and one ID index for the visible paragraphs.
    pub fn capture_batch(
        snapshot: &CanvasCollabSnapshot,
        ids: &[&str],
        now_usec: u64,
    ) -> Vec<Result<Self, Error>> {
        let shared = super::eligible_document(snapshot)
            .and_then(|_| CanonicalTitle::capture(snapshot))
            .map(Arc::new);
        let lookup: std::collections::HashMap<_, _> =
            snapshot.sections.iter().map(|s| (s.id(), s)).collect();
        ids.iter()
            .map(|id| {
                let title = shared.as_ref().map_err(|error| *error)?;
                let section = lookup.get(id).copied().ok_or(Error::Changed)?;
                eligible(snapshot, section, now_usec)?;
                Ok(Self {
                    key: retained::Key::capture(snapshot, section)?,
                    section: section.clone(),
                    title: Arc::clone(title),
                    seed: canonical_draft(section)?,
                })
            })
            .collect()
    }
    pub fn seed(&self) -> &TextDraft {
        &self.seed
    }
    pub fn draft(&self) -> &TextDraft {
        &self.seed
    }
    pub fn section_id(&self) -> &str {
        self.section.id()
    }
    pub fn target_section(&self) -> &str {
        self.section.id()
    }

    pub fn prepare(
        &self,
        fresh: &CanvasCollabSnapshot,
        session: &mut EditSession,
        desired: &TextDraft,
        now: u64,
    ) -> Result<PreparedTextEdit, Error> {
        super::eligible_document(fresh)?;
        let section = fresh.section(self.section.id()).ok_or(Error::Changed)?;
        if self.key != retained::Key::capture(fresh, section)? || section != &self.section {
            return Err(Error::Changed);
        }
        eligible(fresh, section, now)?;
        self.title.revalidate(fresh)?;
        let desired = TextDraft::from_rtml(&desired.rtml()?)?;
        if desired == self.seed {
            return Err(Error::Unchanged);
        }
        let plan = session.text_plan(&self.key, section, &self.seed, now)?;
        let previous = if plan.rotated { 0 } else { session.sequence };
        let sequence = previous.checked_add(1).ok_or(Error::Expired)?;
        let local = (if plan.rotated {
            0
        } else {
            session.local.get(section.id()).copied().unwrap_or(0)
        })
        .checked_add(1)
        .ok_or(Error::Expired)?;
        let wire = crdt::update(
            &plan.wire,
            section,
            &self.seed,
            &desired,
            &plan.session,
            now,
        )?;
        let content = content(section, &desired)?;
        // Captured top-level text write: CONTENT dirty, empty ancestors; no
        // attributes, style or activity-stack field is added to the projection.
        let update = RawMessage::from_fields(&[
            F::bytes(1, section.id().as_bytes())?,
            F::uint(2, section.sequence()?.ok_or(Error::Unsupported)?)?,
            F::uint(9, 0)?,
            F::message(12, &content)?,
            F::message(13, &RawMessage::decode(&[])?)?,
            F::uint(19, 1)?,
            F::uint(20, local)?,
            F::bytes(33, format!("{}-{previous}", plan.session).as_bytes())?,
            F::message(38, &wire)?,
        ])?;
        let data = RawMessage::from_fields(&[
            F::message(1, &update)?,
            F::message(14, self.title.rich_text())?,
        ])?;
        let prepared = PreparedTextEdit {
            key: self.key.clone(),
            canonical: section.clone(),
            desired,
            title: Arc::clone(&self.title),
            session: plan.session.clone(),
            sequence,
            data,
            expiry: plan.wire.uint(4)?,
        };
        session.commit_text_plan(plan, &prepared, wire);
        session.local.insert(section.id().into(), local);
        Ok(prepared)
    }
}

fn eligible(snapshot: &CanvasCollabSnapshot, section: &Section, now: u64) -> Result<(), Error> {
    if section.section_type()?.unwrap_or(0) != 0
        || section.style()?.unwrap_or(0) != 0
        || section.deleted()? == Some(true)
        || section.read_only()? == Some(true)
        || section.document_id()? != Some(snapshot.document_id())
        || section.thread_id()? != Some(snapshot.thread_id())
        || section
            .sequence()?
            .is_none_or(|n| n == 0 || n > i64::MAX as u64)
        || section.raw().boolean(29)? == Some(true)
        || section.raw().has(15)
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
    {
        return Err(Error::Unsupported);
    }
    let draft = canonical_draft(section)?;
    if let Some(wire) = section.crdt()? {
        crdt::validate(&wire, section, &draft, Some(now))?;
    }
    Ok(())
}

pub(super) fn canonical_draft(section: &Section) -> Result<TextDraft, Error> {
    let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    if content.fields().iter().any(|f| f.tag() != 1) {
        return Err(Error::Unsupported);
    }
    let text = content.message(1)?.ok_or(Error::Unsupported)?;
    if text.fields().iter().any(|f| !matches!(f.tag(), 1 | 2)) {
        return Err(Error::Unsupported);
    }
    text.string(2)?;
    TextDraft::from_rtml(text.string(1)?.unwrap_or(""))
}

/// Replace only ContentText.text; retain the original language and protobuf
/// encodings of every sibling. Eligibility rejects unknown semantic siblings.
pub(super) fn content(section: &Section, draft: &TextDraft) -> Result<RawMessage, Error> {
    let original = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    let text = original.message(1)?.ok_or(Error::Unsupported)?;
    let text = text.replace_fields(1, &[F::bytes(1, draft.rtml()?.as_bytes())?])?;
    Ok(original.replace_fields(1, &[F::message(1, &text)?])?)
}

#[cfg(test)]
pub(crate) mod tests;
