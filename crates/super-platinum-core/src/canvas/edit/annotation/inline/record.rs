//! Inert two-section payload checks. These helpers never create a send permit
//! or reconstruct the direct ACK required to bootstrap a comment thread.
use super::*;
use automerge::ReadDoc;

pub(in crate::canvas::edit::annotation) struct Checked {
    model: range::RangeModel,
    wire: RawMessage,
    pub rtml: String,
}

pub(in crate::canvas::edit::annotation) fn payload(
    anchor: &super::super::Section,
    request: &RawMessage,
    selection: Selection,
    control: &str,
    session: &str,
    removal: Option<&Removal>,
) -> Result<Checked, Error> {
    let updates = request.messages(1)?;
    let [_, parent] = updates.as_slice() else {
        return Err(Error::Unsupported);
    };
    let original = super::existing::parse(anchor)?;
    let model =
        range::capture_removing(&original, selection.start, selection.end, control, removal)?;
    payload_model(anchor, parent, model, session)
}

pub(super) fn payload_model(
    anchor: &super::super::Section,
    parent: &RawMessage,
    model: range::RangeModel,
    session: &str,
) -> Result<Checked, Error> {
    let mut tags = parent.fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
    tags.sort_unstable();
    if tags != [1, 2, 9, 12, 13, 19, 20, 33, 38]
        || parent.string(1)? != Some(anchor.id())
        || parent.uint(2)? != anchor.sequence()?
        || parent.uint(9)? != Some(0)
        || parent.message(13)?.is_none_or(|p| !p.fields().is_empty())
        || parent.boolean(19)? != Some(true)
        || parent.uint(20)? != Some(1)
        || parent.string(33)? != Some(format!("{session}-0").as_str())
    {
        return Err(Error::Unsupported);
    }
    let wire = parent.message(38)?.ok_or(Error::Unsupported)?;
    let document = history::validate(&wire, anchor, &model, None)?;
    let (_, object) = document
        .get(automerge::ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let marks = document.marks(&object).map_err(|_| Error::Unsupported)?;
    let rtml = compact::render(&model.text, &marks)?;
    if parent.message(12)? != Some(content(anchor, &rtml)?)
        || wire.uint(4)?.is_some()
        || wire.boolean(7)?.is_some()
    {
        return Err(Error::Unsupported);
    }
    let sent: automerge::ChangeHash = wire
        .string(12)?
        .ok_or(Error::Unsupported)?
        .parse()
        .map_err(|_| Error::Unsupported)?;
    if document.get_heads().as_slice() != [sent] {
        return Err(Error::Unsupported);
    }
    if let Some(original) = anchor.crdt()? {
        let original_document =
            history::validate_existing(&original, anchor, &super::existing::parse(anchor)?, None)?;
        if wire.uint(2)? != original.uint(2)?
            || wire.string(8)? != original.string(8)?
            || original_document
                .get_changes(&[])
                .iter()
                .any(|c| document.get_change_by_hash(&c.hash()).is_none())
        {
            return Err(Error::Unsupported);
        }
    }
    Ok(Checked { model, wire, rtml })
}

pub(in crate::canvas::edit::annotation) fn parent(
    original: &super::super::Section,
    expected: &Checked,
    received: &super::super::Section,
    direct_ack: bool,
) -> Result<(), Error> {
    parent_mode(original, expected, received, direct_ack, false)
}

pub(super) fn parent_mode(
    original: &super::super::Section,
    expected: &Checked,
    received: &super::super::Section,
    direct_ack: bool,
    whole_section: bool,
) -> Result<(), Error> {
    if whole_section {
        super::super::section_body_shape(original)?;
    }
    if received.id() != original.id()
        || received.document_id()? != original.document_id()?
        || received.thread_id()? != original.thread_id()?
        || received.deleted()? == Some(true)
        || received.read_only()? == Some(true)
        || received.section_type()?.unwrap_or(0) != 0
        || received.style()?.unwrap_or(0)
            != if whole_section {
                original.style()?.unwrap_or(0)
            } else {
                0
            }
        || received.raw().uint(11)?.unwrap_or(0) != 0
        // Whole-section v10 validates table ownership in SectionPlan. The
        // received body must retain the exact layout flag; the ordinary range
        // path continues to refuse layout children entirely.
        || if whole_section {
            received.raw().boolean(29)?.unwrap_or(false)
                != original.raw().boolean(29)?.unwrap_or(false)
        } else {
            received.raw().boolean(29)? == Some(true)
        }
        || received.raw().has(15)
        || received.parents()?.is_some_and(|p| !p.fields().is_empty())
        || received.sequence()?.is_none_or(|seq| {
            seq <= original.sequence().ok().flatten().unwrap_or(u64::MAX) || seq > i64::MAX as u64
        })
        || received.raw().message(12)? != Some(content(original, &expected.rtml)?)
        || direct_ack && received.raw().uint(20)? != Some(1)
    {
        return Err(Error::Unconfirmed);
    }
    if expected.wire.uint(2)? == Some(0)
        && stable(original)? != stable(&original.merge_partial(received)?)?
    {
        return Err(Error::Unconfirmed);
    }
    let Some(wire) = received.crdt()? else {
        return if direct_ack || expected.wire.uint(2)? == Some(0) {
            Err(Error::Unconfirmed)
        } else {
            Ok(())
        };
    };
    let document = history::validate(&wire, received, &expected.model, Some(0))?;
    if wire.uint(2)? != expected.wire.uint(2)?
        || wire.uint(3)? != received.sequence()?
        || wire.string(8)? != expected.wire.string(8)?
    {
        return Err(Error::Unconfirmed);
    }
    let sent: automerge::ChangeHash = expected
        .wire
        .string(12)?
        .ok_or(Error::Unsupported)?
        .parse()
        .map_err(|_| Error::Unsupported)?;
    let outgoing = automerge::Automerge::load(expected.wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    if expected.wire.uint(2)? == Some(0)
        && !super::super::strings(&wire, 6)?.contains(&sent.to_string())
        || document.get_change_by_hash(&sent).is_none()
        || outgoing
            .get_changes(&[])
            .iter()
            .any(|c| document.get_change_by_hash(&c.hash()).is_none())
    {
        return Err(Error::Unconfirmed);
    }
    Ok(())
}

// Same zero-base association contract as the existing text ACK path. A fresh
// zero-base history cannot bless unrelated parent/position/unknown-field edits.
fn stable(section: &super::super::Section) -> Result<RawMessage, Error> {
    let mut raw = section.raw().clone();
    for tag in [18, 38, 103, 104, 2, 9, 12, 19, 20, 33] {
        raw = raw.remove(tag)?;
    }
    if section.parents()?.is_some_and(|p| p.fields().is_empty()) {
        raw = raw.remove(13)?;
    }
    let mut fields = raw.fields().iter().collect::<Vec<_>>();
    fields.sort_by_key(|field| match field.tag() {
        1 | 6 | 7 => field.tag(),
        _ => 0,
    });
    let mut bytes = Vec::new();
    for field in fields {
        bytes.extend_from_slice(raw.encoded_field(field)?);
    }
    Ok(RawMessage::decode(&bytes)?)
}
