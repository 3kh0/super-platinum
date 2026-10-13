//! Genuine local CRDT initialization for existing, unmarked text sections.
//!
//! Captured editor `startClientSession_` creates one Automerge TextV2 change,
//! appends an RTML end sentinel, and treats its initial head as already processed.
//! This local provenance is deliberately distinct from a server-expiring session.
use automerge::{
    ActorId, Automerge, ObjType, ROOT,
    transaction::{CommitOptions, Transactable},
};

use super::{Error, F, RawMessage, Section};

/// Bound both work and output for this deliberately small checkbox bootstrap.
const MAX_TEXT_BYTES: usize = 64 * 1024;

/// Created only by the genuine Automerge bootstrap, never by decoding wire data.
/// It cannot be persisted or moved to another canonical section/session.
pub struct LocalCrdt {
    source: Section,
    session: String,
    wire: RawMessage,
}
impl LocalCrdt {
    pub fn bootstrap(section: &Section, session_id: &str, now_usec: u64) -> Result<Self, Error> {
        let text = plain_rtml(section)?;
        if session_id.len() != 12 || !session_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Unsupported);
        }
        // Q vy7t.generateWindowSessionId() with no window argument.
        let actor = format!("{session_id}00000000");
        let actor: ActorId = actor.parse().map_err(|_| Error::Unsupported)?;
        let mut document = Automerge::new().with_actor(actor);
        let mut transaction = document.transaction();
        let object = transaction
            .put_object(ROOT, "text", ObjType::Text)
            .map_err(|_| Error::Unsupported)?;
        transaction
            .splice_text(&object, 0, 0, &format!("{text}\u{feff}"))
            .map_err(|_| Error::Unsupported)?;
        transaction
            .put(ROOT, "deleted", section.deleted()?.unwrap_or(false))
            .map_err(|_| Error::Unsupported)?;
        // Slack change() defaults to Math.floor(Date.now()/1000). Keep the
        // supplied preflight clock deterministic, with checked signed conversion.
        let seconds = i64::try_from(now_usec / 1_000_000).map_err(|_| Error::Unsupported)?;
        transaction.commit_with(CommitOptions::default().with_time(seconds));
        let heads = document.get_heads();
        let [head] = heads.as_slice() else {
            return Err(Error::Unsupported);
        };
        let head = head.to_string();
        let sequence = section.sequence()?.ok_or(Error::Unsupported)?;
        let wire = RawMessage::from_fields(&[
            F::bytes(1, section.id().as_bytes())?,
            F::uint(2, sequence)?,
            F::uint(3, sequence - sequence % 1000)?,
            F::bytes(5, &document.save())?,
            F::bytes(6, head.as_bytes())?,
            F::bytes(8, head.as_bytes())?,
        ])?;
        Ok(Self {
            source: section.clone(),
            session: session_id.to_owned(),
            wire,
        })
    }

    /// Borrow the update only while its exact canonical source and owner match.
    /// The caller must independently enforce document/account/edit permissions.
    pub fn for_section(&self, section: &Section, session_id: &str) -> Result<&RawMessage, Error> {
        if self.source != *section || self.session != session_id {
            return Err(Error::Changed);
        }
        Ok(&self.wire)
    }
}

/// Cheap eligibility check; does not create or serialize an Automerge document.
pub fn supports(section: &Section) -> bool {
    plain_rtml(section).is_ok()
}

fn plain_rtml(section: &Section) -> Result<String, Error> {
    if section.raw().has(38)
        || section.deleted()? == Some(true)
        || section.section_type()?.unwrap_or(0) != 0
        || section.style()?.unwrap_or(0) != 0
        || section
            .sequence()?
            .is_none_or(|seq| seq == 0 || seq > i64::MAX as u64)
    {
        return Err(Error::Unsupported);
    }
    // Section.content(12).text(1).text(1), not HTML export or message fallback.
    let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    if content.fields().iter().any(|field| field.tag() != 1) {
        return Err(Error::Unsupported);
    }
    let text = content.message(1)?.ok_or(Error::Unsupported)?;
    if text
        .fields()
        .iter()
        .any(|field| !matches!(field.tag(), 1 | 2))
    {
        return Err(Error::Unsupported);
    }
    text.string(2)?; // known optional language; retained in the untouched source
    let value = text.string(1)?.unwrap_or("");
    // GoiO parses RTML and escapes text; unmarked Unicode round-trips directly.
    // Entities, tags and normalized controls need that full parser, not stripping.
    if value.len() > MAX_TEXT_BYTES
        || value
            .chars()
            .any(|c| matches!(c, '<' | '>' | '&') || c.is_control())
    {
        return Err(Error::Unsupported);
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests;
