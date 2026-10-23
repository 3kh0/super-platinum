//! Dedicated split receipt checks; global CRDT schemas remain unchanged.
use super::*;
use automerge::ReadDoc;

pub(in crate::canvas::edit::structure) fn child_content(
    draft: &TextDraft,
) -> Result<RawMessage, Error> {
    Ok(RawMessage::from_fields(&[F::message(
        1,
        &RawMessage::from_fields(&[F::bytes(1, draft.rtml()?.as_bytes())?])?,
    )?])?)
}
pub(in crate::canvas::edit::structure) fn content_equal(
    section: &Section,
    expected: &RawMessage,
) -> Result<bool, Error> {
    text::canonical_draft(section)?; // strict singular known text/language schema
    let actual = section
        .raw()
        .message(12)?
        .ok_or(Error::Unconfirmed)?
        .message(1)?
        .ok_or(Error::Unconfirmed)?;
    let expected = expected.message(1)?.ok_or(Error::Unconfirmed)?;
    Ok(actual.bytes(1)? == expected.bytes(1)? && actual.bytes(2)? == expected.bytes(2)?)
}
pub(in crate::canvas::edit::structure) fn processed(
    wire: &RawMessage,
    head: &str,
) -> Result<bool, Error> {
    for field in wire.fields().iter().filter(|f| f.tag() == 6) {
        let value = RawMessage::decode(wire.encoded_field(field)?)?;
        if value.string(6)? == Some(head) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn lineage_matches(actual: &RawMessage, expected: &RawMessage) -> Result<bool, Error> {
    if actual.fields().len() != 7 || actual.fields().iter().any(|f| !(1..=7).contains(&f.tag())) {
        return Ok(false);
    }
    for tag in [1, 4] {
        if actual.uint(tag)? != expected.uint(tag)? {
            return Ok(false);
        }
    }
    for tag in [2, 3, 5, 6] {
        if actual.string(tag)? != expected.string(tag)? {
            return Ok(false);
        }
    }
    Ok(actual.bytes(7)? == expected.bytes(7)?)
}
pub(super) fn contains_history(section: &Section, expected: &RawMessage) -> Result<bool, Error> {
    let wire = section.crdt()?.ok_or(Error::Unconfirmed)?;
    let actual = automerge::Automerge::load(wire.bytes(5)?.ok_or(Error::Unconfirmed)?)
        .map_err(|_| Error::Unconfirmed)?;
    let sent = automerge::Automerge::load(expected.bytes(5)?.ok_or(Error::Unconfirmed)?)
        .map_err(|_| Error::Unconfirmed)?;
    Ok(sent
        .get_changes(&[])
        .iter()
        .all(|change| actual.get_change_by_hash(&change.hash()).is_some()))
}
pub(super) fn history(
    section: &Section,
    draft: &TextDraft,
    sent: &RawMessage,
    lineage: Option<&RawMessage>,
) -> Result<(), Error> {
    let wire = section.crdt()?.ok_or(Error::Unconfirmed)?;
    if let Some(expected) = lineage {
        let infos = wire.messages(10)?;
        if infos.len() != 1 || !lineage_matches(&infos[0], expected)? {
            return Err(Error::Unconfirmed);
        }
    } else if wire.has(10) {
        return Err(Error::Unconfirmed);
    }
    // Remove only the exact separately validated lineage. No new acceptance is
    // granted to merge_info9 or any other unknown semantic CRDT field.
    let ordinary = wire.remove(10)?;
    crdt::validate(
        &ordinary,
        section,
        draft,
        false,
        Some(0),
        sent.uint(2)? == Some(0),
    )?;
    let head = sent.string(12)?.ok_or(Error::Unconfirmed)?;
    if wire.uint(2)? != sent.uint(2)?
        || wire.uint(3)? != section.sequence()?
        || wire.string(8)? != sent.string(8)?
        || !processed(&wire, head)?
        || !contains_history(section, sent)?
    {
        return Err(Error::Unconfirmed);
    }
    Ok(())
}
fn common(prepared: &PreparedSplitEdit, section: &Section, base: u64) -> Result<(), Error> {
    if section.document_id()? != Some(prepared.document_id())
        || section.thread_id()? != Some(prepared.thread_id())
        || section
            .sequence()?
            .is_none_or(|n| n <= base || n > i64::MAX as u64)
        || section.raw().uint(20)? != Some(1)
        || section.raw().string(33)? != Some(format!("{}-0", prepared.session_id()).as_str())
        || section.deleted()? == Some(true)
        || section.read_only()? == Some(true)
        || section.section_type()?.unwrap_or(0) != 0
        || section.style()?.unwrap_or(0) != 0
        || section.raw().uint(11)?.unwrap_or(0) != 0
        || section.raw().boolean(29)? == Some(true)
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        || section.raw().has(17)
    {
        return Err(Error::Unconfirmed);
    }
    Ok(())
}
pub(super) fn parent(prepared: &PreparedSplitEdit, section: &Section) -> Result<(), Error> {
    let original = &prepared.intent.original;
    common(
        prepared,
        section,
        original.sequence()?.ok_or(Error::Unconfirmed)?,
    )?;
    if !content_equal(section, &text::content(original, &prepared.witness.parent)?)?
        || stable_parent(&original.merge_partial(section)?)? != stable_parent(original)?
    {
        return Err(Error::Unconfirmed);
    }
    history(
        section,
        &prepared.witness.parent,
        &prepared.parent_wire,
        Some(&prepared.lineage),
    )
}
pub(super) fn child(prepared: &PreparedSplitEdit, section: &Section) -> Result<(), Error> {
    common(prepared, section, 0)?;
    if section.raw().fields().iter().any(|f| {
        !matches!(
            f.tag(),
            1 | 2
                | 3
                | 6
                | 7
                | 8
                | 9
                | 10
                | 11
                | 12
                | 13
                | 16
                | 18
                | 19
                | 20
                | 21
                | 29
                | 33
                | 35
                | 38
                | 103
                | 104
        )
    }) || section.raw().string(8)? != Some(position::base(&prepared.intent.position)?)
        || section
            .raw()
            .string(21)?
            .is_some_and(|p| Some(p) != section.raw().string(8).ok().flatten())
        || section.attributes()? != Some(RawMessage::from_fields(&[F::uint(4, 0)?])?)
        || !content_equal(section, &child_content(prepared.witness.child_draft())?)?
    {
        return Err(Error::Unconfirmed);
    }
    history(
        section,
        prepared.witness.child_draft(),
        &prepared.child_wire,
        None,
    )
}
pub(in crate::canvas::edit::structure) fn stable_parent(
    section: &Section,
) -> Result<RawMessage, Error> {
    let mut raw = section.raw().clone();
    for tag in [2, 3, 9, 12, 13, 18, 19, 20, 33, 38, 103, 104] {
        raw = raw.remove(tag)?;
    }
    // Partial merge moves present identities to the tail; normalize only those
    // reviewed identity fields, preserving relative order of all unknown bytes.
    let mut fields = raw.fields().iter().collect::<Vec<_>>();
    fields.sort_by_key(|f| {
        if matches!(f.tag(), 1 | 6 | 7) {
            f.tag()
        } else {
            0
        }
    });
    let mut bytes = Vec::new();
    for field in fields {
        bytes.extend_from_slice(raw.encoded_field(field)?);
    }
    Ok(RawMessage::decode(&bytes)?)
}
pub(in crate::canvas::edit::structure) fn readback_stable(
    section: &Section,
) -> Result<RawMessage, Error> {
    let mut raw = section.raw().clone();
    // Content is checked by exact text/language payloads and history separately.
    for tag in [12, 18, 38, 103, 104] {
        raw = raw.remove(tag)?;
    }
    if section.parents()?.is_some_and(|p| p.fields().is_empty()) {
        raw = raw.remove(13)?;
    }
    Ok(raw)
}
