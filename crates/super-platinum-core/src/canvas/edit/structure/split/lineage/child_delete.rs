//! A deleted named child remains inert lineage evidence for one live owner.
use super::*;

/// Reverse-edge admission. No incoming edge means ordinary standalone delete.
pub(in crate::canvas::edit::structure) fn owner<'a>(
    sections: impl IntoIterator<Item = &'a Section>,
    child: &Section,
    now: u64,
) -> Result<Option<Section>, Error> {
    let rows = sections.into_iter().collect::<Vec<_>>();
    let mut found = None;
    for &row in &rows {
        if let Some(wire) = row.crdt()? {
            for info in wire.messages(10)? {
                if info.string(3)? == Some(child.id()) && found.replace(row).is_some() {
                    return Err(Error::Unsupported);
                }
            }
        }
    }
    let Some(parent) = found else { return Ok(None) };
    if parent.deleted()? == Some(true) {
        let prefix = owner_delete::retained_owner_prefix(parent, now)?;
        let projected = rows
            .iter()
            .map(|r| if r.id() == parent.id() { &prefix } else { *r })
            .collect::<Vec<_>>();
        association(&projected, &prefix, child, now)?;
    } else {
        association(&rows, parent, child, now)?;
    }
    // Deleting a paragraph may inspect both reachable neighbors for merging.
    // Original plain/H1-H3 neighbors are no-ops; list/code/layout are unproved.
    let mut reachable = Vec::new();
    for &row in &rows {
        if row.deleted()? != Some(true) && row.section_type()? != Some(64) {
            reachable.push(row);
        }
    }
    reachable.sort_by_key(|r| r.raw().string(8).ok().flatten().unwrap_or(""));
    let index = reachable
        .iter()
        .position(|r| r.id() == child.id())
        .ok_or(Error::Changed)?;
    // Historical live projections are never witnesses for last-live deletion.
    if !reachable
        .iter()
        .filter(|r| r.id() != child.id())
        .map(|r| {
            Ok(r.section_type()?.unwrap_or(0) == 0
                && matches!(r.style()?.unwrap_or(0), 0..=3)
                && r.parents()?.is_none_or(|p| p.fields().is_empty()))
        })
        .collect::<Result<Vec<_>, Error>>()?
        .contains(&true)
    {
        return Err(Error::Unsupported);
    }
    if index > 0 && index + 1 < reachable.len() {
        for neighbor in [reachable[index - 1], reachable[index + 1]] {
            if neighbor.section_type()?.unwrap_or(0) != 0
                || !matches!(neighbor.style()?.unwrap_or(0), 0..=3)
                || neighbor.parents()?.is_some_and(|p| !p.fields().is_empty())
            {
                return Err(Error::Unsupported);
            }
        }
    }
    Ok(Some(parent.clone()))
}

fn association(
    rows: &[&Section],
    parent: &Section,
    child: &Section,
    now: u64,
) -> Result<(), Error> {
    live::isolated_pair(rows, parent, child, now)?;
    let wire = parent.crdt()?.ok_or(Error::Unsupported)?;
    let ev = evidence(&wire, parent, &text::canonical_draft(parent)?)?;
    if ev.child != child.id() {
        return Err(Error::Unsupported);
    }
    child_history_policy(child, &ev, now, false, true)?;
    let left = parent.raw().string(8)?.ok_or(Error::Unsupported)?;
    let right = child.raw().string(8)?.ok_or(Error::Unsupported)?;
    position::canonical(left)?;
    position::canonical(right)?;
    if left >= right {
        return Err(Error::Unsupported);
    }
    let mut positions = std::collections::BTreeSet::new();
    let mut gap = false;
    for &row in rows {
        let p = row.raw().string(8)?.ok_or(Error::Unsupported)?;
        position::canonical(p)?;
        if !positions.insert(p)
            || row.thread_id()? != parent.thread_id()?
            || row.document_id()? != parent.document_id()?
        {
            return Err(Error::Unsupported);
        }
        gap |= left < p && p < right;
    }
    if gap {
        gap::between(rows, parent, child, now)?;
    }
    Ok(())
}

/// Validate the present endpoint, then prove its independently derived live
/// prefix against the exact owner. Never replace the stored deleted row.
pub(in crate::canvas::edit::structure) fn tombstone<'a>(
    sections: impl IntoIterator<Item = &'a Section>,
    child: &Section,
    now: u64,
) -> Result<Section, Error> {
    let rows = sections.into_iter().collect::<Vec<_>>();
    if child.deleted()? != Some(true)
        || rows.iter().filter(|r| r.id() == child.id()).count() != 1
        || !rows.iter().any(|r| *r == child)
    {
        return Err(Error::Unsupported);
    }
    // A retained deleted owner has its own exact deletion suffix. Its bounded
    // pair proof derives both prefixes without recursively invoking this path.
    for &parent in &rows {
        if parent.deleted()? == Some(true) {
            if let Some(wire) = parent.crdt()? {
                if wire
                    .messages(10)?
                    .iter()
                    .any(|info| info.string(3).ok().flatten() == Some(child.id()))
                {
                    let peer = owner_delete::tombstone(rows.iter().copied(), parent)?;
                    if &peer != child {
                        return Err(Error::Unsupported);
                    }
                    return Ok(parent.clone());
                }
            }
        }
    }
    let wire = child.crdt()?.ok_or(Error::Unsupported)?;
    if wire.has(10) || wire.has(12) || wire.uint(3)? != child.sequence()? {
        return Err(Error::Unsupported);
    }
    let draft = text::canonical_draft(child)?;
    let doc = crdt::validate(&wire, child, &draft, true, Some(now), true)?;
    let heads = doc.get_heads();
    if heads.len() != 1 || !validation::processed(&wire, &heads[0].to_string())? {
        return Err(Error::Unsupported);
    }
    let live = owner_delete::deletion_prefix(&wire, child, &draft)?;
    let projected = rows
        .iter()
        .map(|r| if r.id() == child.id() { &live } else { *r });
    owner(projected, &live, now)?.ok_or(Error::Unsupported)
}

/// Bind one deletion to the exact admitted source, not equal visible text.
pub(in crate::canvas::edit::structure) fn transition(
    original: &Section,
    wire: &RawMessage,
    canonical: &Section,
) -> Result<(), Error> {
    let source = original.crdt()?.ok_or(Error::Unsupported)?;
    if source.has(10)
        || wire.has(10)
        || source.uint(2)? != wire.uint(2)?
        || source.string(8)? != wire.string(8)?
    {
        return Err(Error::Unconfirmed);
    }
    let draft = text::canonical_draft(original)?;
    let prefix = owner_delete::deletion_prefix(wire, canonical, &draft)?;
    let before = Automerge::load(source.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let prefix_wire = prefix.crdt()?.ok_or(Error::Unsupported)?;
    let after = Automerge::load(prefix_wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    if before.get_heads() != after.get_heads()
        || before.get_changes(&[]).len() != after.get_changes(&[]).len()
        || before.get_changes(&[]).iter().any(|c| {
            after
                .get_change_by_hash(&c.hash())
                .is_none_or(|a| a.raw_bytes() != c.raw_bytes())
        })
    {
        return Err(Error::Unconfirmed);
    }
    Ok(())
}
