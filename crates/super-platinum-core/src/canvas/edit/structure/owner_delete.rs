//! Saved single-split owner deletion. A tombstone is evidence, never a visible
//! sibling or permission to discard lineage. Ordinary deletion stays separate.
use super::*;
use automerge::{Automerge, PatchAction, ROOT, ReadDoc, Value};
mod retained_pair;

pub(super) fn live_pair<'a>(
    sections: impl IntoIterator<Item = &'a Section>,
    parent: &Section,
    now: u64,
) -> Result<Section, Error> {
    let rows = sections.into_iter().collect::<Vec<_>>();
    if retained_pair::has_deleted_child(&rows, parent)? {
        return retained_pair::live_owner(&rows, parent, now);
    }
    pair(&rows, parent, now, false)
}
fn pair(rows: &[&Section], parent: &Section, now: u64, historical: bool) -> Result<Section, Error> {
    let wire = parent.crdt()?.ok_or(Error::Unsupported)?;
    let infos = wire.messages(10)?;
    let [info] = infos.as_slice() else {
        return Err(Error::Unsupported);
    };
    let child_id = info.string(3)?.ok_or(Error::Unsupported)?;
    let parent_position = parent.raw().string(8)?.ok_or(Error::Unsupported)?;
    let mut previous: Option<&Section> = None;
    for row in rows {
        // The canonical title is not a reachable paragraph. Keep its raw row
        // and position in Inventory, but do not mistake it for a merge neighbor.
        if row.deleted()? == Some(true) || row.section_type()? == Some(64) {
            continue;
        }
        let p = row.raw().string(8)?.ok_or(Error::Unsupported)?;
        if p < parent_position
            && previous.is_none_or(|old| old.raw().string(8).ok().flatten().is_some_and(|v| v < p))
        {
            previous = Some(row);
        }
    }
    if let Some(previous) = previous.filter(|_| !historical) {
        if previous.section_type()?.unwrap_or(0) != 0
            || !matches!(previous.style()?.unwrap_or(0), 0..=3)
            || previous.parents()?.is_some_and(|p| !p.fields().is_empty())
        {
            return Err(Error::Unsupported);
        }
    }
    // Exactly one edge in this component. A single-info owner may itself be
    // somebody else's child; counting only its own SplitInfo is insufficient.
    for row in rows {
        if let Some(other) = row.crdt()? {
            for edge in other.messages(10)? {
                let child = edge.string(3)?.ok_or(Error::Unsupported)?;
                if (row.id() == parent.id()
                    || row.id() == child_id
                    || child == parent.id()
                    || child == child_id)
                    && (row.id() != parent.id() || edge != *info)
                {
                    return Err(Error::Unsupported);
                }
            }
        }
    }
    if parent.deleted()? == Some(true)
        || parent.section_type()?.unwrap_or(0) != 0
        || parent.style()?.unwrap_or(0) != 0
        || parent.read_only()? == Some(true)
        || parent.parents()?.is_some_and(|p| !p.fields().is_empty())
        || wire.uint(3)? != parent.sequence()?
        || wire.has(12)
    {
        return Err(Error::Unsupported);
    }
    let draft = text::canonical_draft(parent)?;
    let doc = text::crdt::validate(&wire, parent, &draft, Some(now))?;
    require_processed(&wire, &doc)?;
    let children = if historical {
        let members = rows
            .iter()
            .filter(|s| s.id() == child_id)
            .copied()
            .collect::<Vec<_>>();
        let [child] = members.as_slice() else {
            return Err(Error::Changed);
        };
        // Adjacency was required when the owner was live. After deletion,
        // independent insertion may occupy the gap. Prove the named child's
        // exact causal origin, keeping every intervening raw row in Inventory.
        vec![split::retained_owner_child(parent, child, now)?]
    } else {
        split_child(rows.iter().copied(), parent, now)?
    };
    let [child] = children.as_slice() else {
        return Err(Error::Unsupported);
    };
    let cw = child.crdt()?.ok_or(Error::Unsupported)?;
    if cw.has(10) || cw.has(12) || cw.uint(3)? != child.sequence()? {
        return Err(Error::Unsupported);
    }
    require_processed(
        &cw,
        &text::crdt::validate(&cw, &child, &text::canonical_draft(&child)?, Some(now))?,
    )?;
    Ok(child.clone())
}

fn require_processed(wire: &RawMessage, doc: &Automerge) -> Result<(), Error> {
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    let current = head.to_string();
    for field in wire.fields().iter().filter(|f| f.tag() == 6) {
        if RawMessage::decode(wire.encoded_field(field)?)?.string(6)? == Some(&current) {
            return Ok(());
        }
    }
    Err(Error::Unsupported)
}

/// Recover the live causal prefix only after proving one final, one-operation
/// deletion change. Equal text/deleted projections alone never establish it.
fn live_prefix(wire: &RawMessage, section: &Section, draft: &TextDraft) -> Result<Section, Error> {
    if wire.messages(10)?.len() != 1 {
        return Err(Error::Unsupported);
    }
    deletion_prefix(wire, section, draft)
}

// Shared deletion-only proof; callers separately prove owner or child lineage.
pub(super) fn deletion_prefix(
    wire: &RawMessage,
    section: &Section,
    draft: &TextDraft,
) -> Result<Section, Error> {
    let bytes = wire.bytes(5)?.ok_or(Error::Unsupported)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Unsupported);
    }
    let doc = Automerge::load(bytes).map_err(|_| Error::Unsupported)?;
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    let change = doc.get_change_by_hash(head).ok_or(Error::Unsupported)?;
    // Original sessions include processed ancestors alongside the current
    // head in the dependency list. Admit that redundant ancestry only when
    // the complete causal prefix reduces to one head, never a merge branch.
    if change.deps().is_empty() || change.deps().len() > 1024 {
        return Err(Error::Unsupported);
    }
    // Reject a hidden write before asking the history machinery for its prefix.
    let decoded = change.decode();
    let [operation] = decoded.operations.as_slice() else {
        return Err(Error::Unsupported);
    };
    if !matches!(
        (&operation.obj, &operation.key, &operation.action, operation.insert),
        (automerge::legacy::ObjectId::Root, automerge::legacy::Key::Map(key),
         automerge::legacy::OpType::Put(automerge::ScalarValue::Boolean(true)), false)
            if key == "deleted"
    ) {
        return Err(Error::Unsupported);
    }
    // Pinned Automerge 0.7.4 fork_at reconstructs a selected hash traversal;
    // redundant ancestor heads can make that collector panic (MissingOps or
    // MissingDep). Replay the complete bounded history except the unique final
    // change instead. Do not normalize/drop raw dependencies or catch a panic.
    let changes = doc.get_changes(&[]);
    let prefix_changes = changes
        .iter()
        .filter(|c| c.hash() != *head)
        .cloned()
        .collect::<Vec<_>>();
    let known = prefix_changes
        .iter()
        .map(|c| (c.hash(), c.raw_bytes()))
        .collect::<std::collections::HashMap<_, _>>();
    if prefix_changes.len() + 1 != changes.len()
        || known.len() != prefix_changes.len()
        || change.deps().iter().any(|dep| !known.contains_key(dep))
    {
        return Err(Error::Unsupported);
    }
    let mut prefix = Automerge::new();
    prefix
        .apply_changes(prefix_changes.clone())
        .map_err(|_| Error::Unsupported)?;
    let prefix_heads = prefix.get_heads();
    let [previous] = prefix_heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    if !change.deps().contains(previous) {
        return Err(Error::Unsupported);
    }
    let replayed = prefix.get_changes(&[]);
    if replayed.len() != prefix_changes.len()
        || replayed
            .iter()
            .any(|c| known.get(&c.hash()).is_none_or(|raw| *raw != c.raw_bytes()))
    {
        return Err(Error::Unsupported);
    }
    let actor = change.actor_id().to_hex_string();
    let patches = doc.diff(&[*previous], &[*head]);
    if change.len() != 1
        || actor.len() != 20
        || !actor.ends_with("00000000")
        || patches.len() != 1
        || patches[0].obj != ROOT
        || !matches!(&patches[0].action, PatchAction::PutMap {key, value, conflict: false}
            if key == "deleted" && value.0 == Value::from(true))
    {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(&doc, draft, true)?;
    let actor_sequence = replayed
        .iter()
        .filter(|c| c.actor_id() == change.actor_id())
        .map(|c| c.seq())
        .max()
        .unwrap_or(0);
    if change.seq() != actor_sequence + 1 {
        return Err(Error::Unsupported);
    }
    if replayed.len() + 1 != changes.len() {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(&prefix, draft, false)?;
    let deleted_head = head.to_string();
    let mut fields = Vec::new();
    for f in wire.fields() {
        let encoded = RawMessage::decode(wire.encoded_field(f)?)?;
        let discard = match f.tag() {
            5 | 12 => true,
            6 => encoded.string(6)? == Some(&deleted_head),
            11 => encoded.message(11)?.ok_or(Error::Unsupported)?.string(1)? == Some(&deleted_head),
            _ => false,
        };
        if !discard {
            fields.extend_from_slice(wire.encoded_field(f)?);
        }
    }
    let prefix_wire =
        RawMessage::decode(&fields)?.replace_fields(5, &[F::bytes(5, &prefix.save())?])?;
    let live = Section::from_raw(
        section
            .raw()
            .replace_fields(3, &[F::uint(3, 0)?])?
            .replace_fields(38, &[F::message(38, &prefix_wire)?])?,
    )?;
    // This proves the historical split and live prefix including all saved
    // text descendants; no blanket removal of SplitInfo is used for admission.
    text::crdt::validate(&prefix_wire, &live, draft, None)?;
    require_processed(&prefix_wire, &prefix)?;
    Ok(live)
}

pub(super) fn history(
    wire: &RawMessage,
    section: &Section,
    draft: &TextDraft,
    deleted: bool,
    now: Option<u64>,
    allow_zero_base: bool,
) -> Result<Automerge, Error> {
    if !deleted {
        if wire.messages(10)?.len() != 1 {
            return Err(Error::Unsupported);
        }
        return text::crdt::validate(wire, section, draft, now);
    }
    live_prefix(wire, section, draft)?;
    // SplitInfo was independently proved above. The existing envelope validator
    // now checks the remaining canonical lease/heads/deleted state unchanged.
    let doc = crdt::validate(
        &wire.remove(10)?,
        section,
        draft,
        true,
        now,
        allow_zero_base,
    )?;
    require_processed(wire, &doc)?;
    Ok(doc)
}

/// Prove a retained owner's deletion before projecting its original live prefix.
/// The caller must bind that prefix to the complete isolated current component.
pub(super) fn retained_owner_prefix(section: &Section, now: u64) -> Result<Section, Error> {
    if section.deleted()? != Some(true) {
        return Err(Error::Unsupported);
    }
    let wire = section.crdt()?.ok_or(Error::Unsupported)?;
    if wire.has(12) || wire.uint(3)? != section.sequence()? {
        return Err(Error::Unsupported);
    }
    let draft = text::canonical_draft(section)?;
    history(
        &wire,
        section,
        &draft,
        true,
        Some(now),
        position::modern_id(section.id()),
    )?;
    live_prefix(&wire, section, &draft)
}

pub(super) fn tombstone<'a>(
    sections: impl IntoIterator<Item = &'a Section>,
    section: &Section,
) -> Result<Section, Error> {
    let rows = sections.into_iter().collect::<Vec<_>>();
    if section.deleted()? != Some(true) {
        return Err(Error::Unsupported);
    }
    if retained_pair::has_deleted_child(&rows, section)? {
        return retained_pair::prove(&rows, section, 0);
    }
    let wire = section.crdt()?.ok_or(Error::Unsupported)?;
    let draft = text::canonical_draft(section)?;
    history(
        &wire,
        section,
        &draft,
        true,
        Some(0),
        position::modern_id(section.id()),
    )?;
    if wire.uint(3)? != section.sequence()? || wire.has(12) {
        return Err(Error::Unsupported);
    }
    let prefix = live_prefix(&wire, section, &draft)?;
    pair(&rows, &prefix, 0, true)
}

pub(super) fn transition(
    original: &Section,
    wire: &RawMessage,
    section: &Section,
    draft: &TextDraft,
) -> Result<(), Error> {
    let source = original.crdt()?.ok_or(Error::Unsupported)?;
    if wire.messages(10)? != source.messages(10)?
        || wire.uint(2)? != source.uint(2)?
        || wire.string(8)? != source.string(8)?
    {
        return Err(Error::Unconfirmed);
    }
    let prefix = live_prefix(wire, section, draft)?;
    let before = Automerge::load(source.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let prefix_wire = prefix.crdt()?.ok_or(Error::Unsupported)?;
    let prefix_doc = Automerge::load(prefix_wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    if before.get_heads() != prefix_doc.get_heads()
        || before.get_changes(&[]).len() != prefix_doc.get_changes(&[]).len()
        || before
            .get_changes(&[])
            .iter()
            .any(|c| prefix_doc.get_change_by_hash(&c.hash()).is_none())
    {
        return Err(Error::Unconfirmed);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
