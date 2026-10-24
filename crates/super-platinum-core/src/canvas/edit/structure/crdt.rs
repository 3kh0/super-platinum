//! Bounded clean-history construction. Full multiplayer dependency admission is
//! deliberately not approximated: ambiguous heads/conflicts are refused.
use super::*;
use automerge::{
    ActorId, Automerge, ObjType, ROOT, ReadDoc,
    marks::ExpandMark,
    transaction::{CommitOptions, Transactable},
};

fn actor(session: &str) -> Result<ActorId, Error> {
    if session.len() != 12 || !session.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Unsupported);
    }
    format!("{session}00000000")
        .parse()
        .map_err(|_| Error::Unsupported)
}
fn time(now: u64) -> Result<i64, Error> {
    i64::try_from(now / 1_000_000).map_err(|_| Error::Unsupported)
}
fn head(doc: &Automerge) -> Result<String, Error> {
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    Ok(head.to_string())
}
fn processed(wire: &RawMessage) -> Result<Vec<String>, Error> {
    wire.fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            let raw = RawMessage::decode(wire.encoded_field(f)?)?;
            Ok(raw.string(6)?.ok_or(Error::Unsupported)?.to_owned())
        })
        .collect()
}

pub(super) fn insert(
    id: &str,
    draft: &TextDraft,
    session: &str,
    now: u64,
) -> Result<RawMessage, Error> {
    let mut doc = Automerge::new().with_actor(actor(session)?);
    let mut tx = doc.transaction();
    let object = tx
        .put_object(ROOT, "text", ObjType::Text)
        .map_err(|_| Error::Unsupported)?;
    tx.splice_text(&object, 0, 0, "\u{feff}")
        .map_err(|_| Error::Unsupported)?;
    tx.put(ROOT, "deleted", false)
        .map_err(|_| Error::Unsupported)?;
    tx.commit_with(CommitOptions::default().with_time(time(now)?));
    let base = head(&doc)?;
    let model = draft.model()?;
    if !model.text.is_empty() {
        let mut tx = doc.transaction();
        tx.update_text(&object, format!("{}\u{feff}", model.text))
            .map_err(|_| Error::Unsupported)?;
        for mark in model.marks {
            tx.mark(&object, mark, ExpandMark::After)
                .map_err(|_| Error::Unsupported)?;
        }
        tx.commit_with(CommitOptions::default().with_time(time(now)?));
    }
    text::crdt::check_document_state(&doc, draft, false)?;
    let current = head(&doc)?;
    // Dirty new bootstrap is not processed. FOR_SERVER appends only the current
    // queued snapshot head, not its initial base head as an extra processed head.
    Ok(RawMessage::from_fields(&[
        F::bytes(1, id.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(5, &doc.save())?,
        F::bytes(6, current.as_bytes())?,
        F::bytes(8, base.as_bytes())?,
        F::bytes(12, current.as_bytes())?,
    ])?)
}

pub(super) fn delete(
    section: &Section,
    draft: &TextDraft,
    session: &str,
    now: u64,
) -> Result<(RawMessage, Option<u64>), Error> {
    let allow_zero_base = position::modern_id(section.id())
        && section.sequence()?.is_some_and(|sequence| sequence > 0);
    let (seed, expiry) = match section.crdt()? {
        Some(seed) => {
            validate(&seed, section, draft, false, Some(now), allow_zero_base)?;
            let expiry = seed.uint(4)?;
            (seed, expiry)
        }
        None => (text::crdt::bootstrap(section, draft, session, now)?, None),
    };
    let mut doc =
        validate(&seed, section, draft, false, None, allow_zero_base)?.with_actor(actor(session)?);
    let old_processed = processed(&seed)?;
    // Every visible history head must already be processed. The mutation can
    // therefore depend on current history without synthesizing merge/replay.
    if !old_processed.contains(&head(&doc)?) {
        return Err(Error::Unsupported);
    }
    let mut tx = doc.transaction();
    tx.put(ROOT, "deleted", true)
        .map_err(|_| Error::Unsupported)?;
    tx.commit_with(CommitOptions::default().with_time(time(now)?));
    text::crdt::check_document_state(&doc, draft, true)?;
    let current = head(&doc)?;
    // E getSnapshotToSave concatenates queued head + processed heads, then Q
    // FOR_SERVER appends those to existing processed heads, preserving repeats.
    let mut outgoing = old_processed
        .iter()
        .map(|h| F::bytes(6, h.as_bytes()))
        .collect::<Result<Vec<_>, _>>()?;
    outgoing.push(F::bytes(6, current.as_bytes())?);
    for value in &old_processed {
        outgoing.push(F::bytes(6, value.as_bytes())?);
    }
    let wire = seed
        .remove(4)?
        .remove(7)?
        .replace_fields(5, &[F::bytes(5, &doc.save())?])?
        .replace_fields(6, &outgoing)?
        .replace_fields(12, &[F::bytes(12, current.as_bytes())?])?;
    Ok((wire, expiry))
}

pub(super) fn validate(
    wire: &RawMessage,
    section: &Section,
    draft: &TextDraft,
    deleted: bool,
    now: Option<u64>,
    allow_zero_base: bool,
) -> Result<Automerge, Error> {
    if wire.has(10) {
        return owner_delete::history(wire, section, draft, deleted, now, allow_zero_base);
    }
    let sequence = section.sequence()?.ok_or(Error::Unsupported)?;
    if wire
        .fields()
        .iter()
        .any(|f| !matches!(f.tag(), 1..=8 | 11 | 12))
        || wire.string(1)? != Some(section.id())
        || wire.boolean(7)? == Some(true)
        || wire
            .uint(2)?
            .is_none_or(|n| (!allow_zero_base && n == 0) || n > sequence)
        || wire.uint(3)?.is_none_or(|n| n > sequence)
    {
        return Err(Error::Unsupported);
    }
    if let Some(now) = now {
        if wire
            .uint(4)?
            .is_none_or(|expires| expires <= now || expires > i64::MAX as u64)
        {
            return Err(Error::Expired);
        }
    }
    let bytes = wire.bytes(5)?.ok_or(Error::Unsupported)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Unsupported);
    }
    let doc = Automerge::load(bytes).map_err(|_| Error::Unsupported)?;
    if doc.get_heads().len() != 1
        || !doc.get_missing_deps(&[]).is_empty()
        || doc
            .get_all(ROOT, "text")
            .map_err(|_| Error::Unsupported)?
            .len()
            != 1
        || doc
            .get_all(ROOT, "deleted")
            .map_err(|_| Error::Unsupported)?
            .len()
            != 1
    {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(&doc, draft, deleted)?;
    let known = |value: &str| -> Result<(), Error> {
        let hash = value.parse().map_err(|_| Error::Unsupported)?;
        if doc.get_change_by_hash(&hash).is_none() {
            return Err(Error::Unsupported);
        }
        Ok(())
    };
    let heads = processed(wire)?;
    if heads.is_empty() || heads.len() > 1024 {
        return Err(Error::Unsupported);
    }
    for value in &heads {
        known(value)?;
    }
    known(wire.string(8)?.ok_or(Error::Unsupported)?)?;
    if let Some(current) = wire.string(12)? {
        known(current)?;
    }
    for item in wire.messages(11)? {
        if item.fields().iter().any(|f| !matches!(f.tag(), 1 | 2))
            || item.uint(2)?.is_none_or(|n| n > sequence)
        {
            return Err(Error::Unsupported);
        }
        known(item.string(1)?.ok_or(Error::Unsupported)?)?;
    }
    Ok(doc)
}

// The captured update/server-update methods preserve the zero base of an
// inserted modern section after its canonical sequence advances. Share the
// existing strict structural validator with subsequent content edits.
pub(in crate::canvas::edit) fn canonical_zero_base_history(
    wire: &RawMessage,
    section: &Section,
    draft: &TextDraft,
    now: Option<u64>,
) -> Result<Automerge, Error> {
    if !position::modern_id(section.id())
        || section
            .sequence()?
            .is_none_or(|n| n == 0 || n > i64::MAX as u64)
        || wire.uint(2)? != Some(0)
        || wire.uint(3)? != section.sequence()?
    {
        return Err(Error::Unsupported);
    }
    let doc = validate(wire, section, draft, false, now, true)?;
    if !processed(wire)?.contains(&head(&doc)?) {
        return Err(Error::Unsupported);
    }
    Ok(doc)
}
