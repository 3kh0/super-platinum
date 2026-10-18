//! Retained Control/Entity association removal. Original bytes, topology, actual
//! ACKs and canonical observations remain separate throughout journal recovery.
use super::{Error, F, RawMessage, Section};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ControlUpdate {
    original: Vec<u8>,
    cleared: Vec<u8>,
    metadata: Option<Vec<u8>>,
    // Captured renderer topology determines the Control's positionPath after
    // clearHighlightSections_. It must never be copied from the old wire path.
    after_path: String,
}

impl ControlUpdate {
    pub(super) fn capture(
        original: &Section,
        metadata: Option<&RawMessage>,
        anchor: &Section,
        after_path: String,
    ) -> Result<Self, Error> {
        let content = original.raw().message(12)?.ok_or(Error::Unsupported)?;
        let highlight = content.message(7)?.ok_or(Error::Unsupported)?;
        let ids = super::strings(&highlight, 3)?;
        if ids.is_empty()
            || !ids.iter().any(|id| id == anchor.id())
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
            || ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
            || original.id() == anchor.id()
            || original.document_id()? != anchor.document_id()?
            || original.thread_id()? != anchor.thread_id()?
            || original.section_type()? != Some(9)
            || original.raw().uint(11)? != Some(1)
            || original.deleted()? == Some(true)
            || original.read_only()? == Some(true)
            || original.parents()?.is_some_and(|p| !p.fields().is_empty())
            || original.crdt()?.is_some()
            || original
                .sequence()?
                .is_none_or(|n| n == 0 || n >= i64::MAX as u64)
            || metadata.is_some_and(|m| m.string(1).ok().flatten() != Some(original.id()))
        {
            return Err(Error::Unsupported);
        }
        // Caller binds this to complete original body topology. A flat body
        // reference retains its containing section interval after clear.
        if !valid_after_path(&after_path) {
            return Err(Error::Unsupported);
        }
        let cleared = cleared_content(&content)?;
        let value = Self {
            original: original.raw().raw().to_vec(),
            cleared: cleared.raw().to_vec(),
            metadata: metadata.map(|m| m.raw().to_vec()),
            after_path,
        };
        value.validate()?;
        Ok(value)
    }

    pub(super) fn original(&self) -> Result<Section, Error> {
        Ok(Section::decode(&self.original)?)
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        if self
            .original
            .len()
            .checked_add(self.cleared.len())
            .and_then(|n| n.checked_add(self.metadata.as_ref().map_or(0, Vec::len)))
            .is_none_or(|n| n > 512 * 1024)
            || !valid_after_path(&self.after_path)
        {
            return Err(Error::Unsupported);
        }
        let original = self.original()?;
        if original.section_type()? != Some(9)
            || original.raw().uint(11)? != Some(1)
            || original.deleted()? == Some(true)
            || original.read_only()? == Some(true)
            || original.parents()?.is_some_and(|p| !p.fields().is_empty())
            || original.crdt()?.is_some()
            || original
                .sequence()?
                .is_none_or(|n| n == 0 || n >= i64::MAX as u64)
            || original.document_id()?.is_none()
            || original.thread_id()?.is_none()
        {
            return Err(Error::Unsupported);
        }
        let content = original.raw().message(12)?.ok_or(Error::Unsupported)?;
        let highlight = content.message(7)?.ok_or(Error::Unsupported)?;
        let ids = super::strings(&highlight, 3)?;
        if ids.is_empty()
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
            || ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
        {
            return Err(Error::Unsupported);
        }
        let cleared = cleared_content(&content)?;
        if !highlight.has(3) || cleared.raw() != self.cleared {
            return Err(Error::Unsupported);
        }
        if let Some(raw) = &self.metadata {
            let meta = RawMessage::decode(raw)?;
            if meta.string(1)? != Some(original.id()) {
                return Err(Error::Unsupported);
            }
        }
        Ok(())
    }

    pub(super) fn current_metadata(&self, current: Option<&RawMessage>) -> Result<(), Error> {
        if current.map(RawMessage::raw) != self.metadata.as_deref() {
            return Err(Error::Changed);
        }
        Ok(())
    }

    /// Exact retained Entity.toUpdateBundle with dirty CONTENT+ALT_RTML,
    /// isWeb()==true, fresh local Entity counter, and Control position override.
    /// The caller chooses a fresh operation session; deserialization is inert.
    pub(super) fn request(&self, session: &str) -> Result<RawMessage, Error> {
        self.validate()?;
        if session.len() != 12 || !session.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Unsupported);
        }
        let original = self.original()?;
        RawMessage::from_fields(&[
            F::bytes(1, original.id().as_bytes())?,
            F::uint(2, original.sequence()?.ok_or(Error::Unsupported)?)?,
            F::uint(9, 9)?,
            F::message(12, &RawMessage::decode(&self.cleared)?)?,
            F::bytes(14, b"")?,
            F::uint(19, 1)?,
            F::uint(20, 1)?,
            F::bytes(21, self.after_path.as_bytes())?,
            F::bytes(33, format!("{session}-0").as_bytes())?,
        ])
        .map_err(Into::into)
    }

    /// Validate one actual updated control, never a derived/fabricated ACK.
    /// Partial ACKs may omit original unrelated fields; the subsequent fresh
    /// canonical read must independently retain those fields exactly.
    pub(super) fn check_received(&self, received: &Section, direct_ack: bool) -> Result<(), Error> {
        self.validate()?;
        let original = self.original()?;
        if received.id() != original.id()
            || received.document_id()? != original.document_id()?
            || received.thread_id()? != original.thread_id()?
            || received.sequence()?.is_none_or(|n| {
                n <= original.sequence().ok().flatten().unwrap_or(u64::MAX) || n > i64::MAX as u64
            })
            || received.section_type()? != Some(9)
            || received.deleted()? == Some(true)
            || received.read_only()? == Some(true)
            || received.raw().message(12)?.as_ref().map(RawMessage::raw)
                != Some(self.cleared.as_slice())
            || received.raw().string(14)?.is_some_and(|v| !v.is_empty())
            || canonical_path(received.raw().string(21)?.ok_or(Error::Unconfirmed)?)?
                != canonical_path(&self.after_path)?
            || direct_ack && received.raw().uint(20)? != Some(1)
        {
            return Err(Error::Unconfirmed);
        }
        let expected = original
            .raw()
            .replace_fields(12, &[F::message(12, &RawMessage::decode(&self.cleared)?)?])?
            .replace_fields(14, &[F::bytes(14, b"")?])?
            .replace_fields(21, &[F::bytes(21, self.after_path.as_bytes())?])?;
        let actual = if direct_ack {
            original.raw().merge_partial(received.raw())?
        } else {
            received.raw().clone()
        };
        if stable(&expected)? != stable(&actual)? {
            return Err(Error::Unconfirmed);
        }
        Ok(())
    }
}

/// Actual Highlight.renderToDom copies a closed set of fields into a fresh
/// content_ object and calls the default-valued setters. Removing associations
/// then serializes that object; simply removing tag 3 from original wire bytes
/// misses those defaults. Refuse content fields that this renderer would drop,
/// instead of silently deleting unknown/control-copy information.
fn cleared_content(content: &RawMessage) -> Result<RawMessage, Error> {
    if content.fields().len() != 1 || content.fields()[0].tag() != 7 {
        return Err(Error::Unsupported);
    }
    let highlight = content.message(7)?.ok_or(Error::Unsupported)?;
    if highlight
        .fields()
        .iter()
        .any(|field| !matches!(field.tag(), 1 | 3 | 4 | 5 | 6 | 7 | 9 | 10 | 11 | 13))
        || highlight.has(6) != highlight.has(7)
    {
        return Err(Error::Unsupported);
    }
    let creator = highlight.string(1)?.unwrap_or("");
    let mut fields = Vec::new();
    // setCreator does not insert an empty default. The remaining setters do.
    if !creator.is_empty() {
        fields.push(F::bytes(1, creator.as_bytes())?);
    }
    fields.push(F::uint(
        4,
        u64::from(highlight.boolean(4)?.unwrap_or(false)),
    )?);
    fields.push(F::uint(5, highlight.uint(5)?.unwrap_or(0))?);
    if highlight.has(6) {
        for tag in [6, 7] {
            fields.push(F::bytes(
                tag,
                highlight.string(tag)?.ok_or(Error::Unsupported)?.as_bytes(),
            )?);
        }
    }
    fields.push(F::uint(9, highlight.uint(9)?.unwrap_or(2))?);
    fields.push(F::bytes(
        10,
        highlight.string(10)?.unwrap_or("").as_bytes(),
    )?);
    fields.push(F::message(
        11,
        &highlight.message(11)?.unwrap_or(RawMessage::decode(&[])?),
    )?);
    fields.push(F::uint(13, highlight.uint(13)?.unwrap_or(0))?);
    RawMessage::from_fields(&[F::message(7, &RawMessage::from_fields(&fields)?)?])
        .map_err(Into::into)
}

fn stable(raw: &RawMessage) -> Result<Vec<u8>, Error> {
    let mut fields = raw
        .fields()
        .iter()
        .filter(|f| !matches!(f.tag(), 2 | 18 | 19 | 20 | 33 | 103 | 104))
        .collect::<Vec<_>>();
    fields.sort_by_key(|f| f.tag());
    let mut bytes = Vec::new();
    for field in fields {
        // Empty alt RTML is the retained Highlight default; server omission is
        // equivalent only for this known default, never for unknown fields.
        if field.tag() == 14 && raw.string(14)? == Some("") {
            continue;
        }
        if field.tag() == 21 {
            bytes.extend_from_slice(
                F::bytes(
                    21,
                    canonical_path(raw.string(21)?.ok_or(Error::Unsupported)?)?.as_bytes(),
                )?
                .raw(),
            );
        } else {
            bytes.extend_from_slice(raw.encoded_field(field)?);
        }
    }
    Ok(bytes)
}

fn valid_after_path(value: &str) -> bool {
    value == "zzzzzz-orphaned-m" || (value.ends_with(":temp") && canonical_path(value).is_ok())
}

fn canonical_path(value: &str) -> Result<&str, Error> {
    if value == "zzzzzz-orphaned-m" {
        Ok(value)
    } else {
        let base = value.strip_suffix(":temp").unwrap_or(value);
        if base.len() > 513 {
            return Err(Error::Unsupported);
        }
        let parts = base.split('-').collect::<Vec<_>>();
        if !(1..=2).contains(&parts.len()) {
            return Err(Error::Unsupported);
        }
        for part in parts {
            super::position::canonical(part)?;
        }
        Ok(base)
    }
}
