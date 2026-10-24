//! Two retained tombstones preserve one isolated historical split component.
//! Live projections exist only while proving history, never in stored inventory.
use super::*;

fn child<'a>(rows: &[&'a Section], owner: &Section) -> Result<&'a Section, Error> {
    let wire = owner.crdt()?.ok_or(Error::Unsupported)?;
    let infos = wire.messages(10)?;
    let [info] = infos.as_slice() else {
        return Err(Error::Unsupported);
    };
    let id = info.string(3)?.ok_or(Error::Unsupported)?;
    let matches = rows
        .iter()
        .copied()
        .filter(|r| r.id() == id)
        .collect::<Vec<_>>();
    let [child] = matches.as_slice() else {
        return Err(Error::Unsupported);
    };
    Ok(child)
}

pub(super) fn has_deleted_child(rows: &[&Section], owner: &Section) -> Result<bool, Error> {
    Ok(child(rows, owner)?.deleted()? == Some(true))
}

pub(super) fn live_owner(rows: &[&Section], owner: &Section, now: u64) -> Result<Section, Error> {
    if owner.deleted()? == Some(true) {
        return Err(Error::Unsupported);
    }
    let peer = prove(rows, owner, now)?;
    // Actual reachable neighbors decide last-paragraph replacement and merging.
    // The derived live child below must not count as a surviving paragraph.
    let mut reachable = Vec::new();
    for &row in rows {
        if row.deleted()? != Some(true) && row.section_type()? != Some(64) {
            reachable.push(row);
        }
    }
    reachable.sort_by_key(|r| r.raw().string(8).ok().flatten().unwrap_or(""));
    let index = reachable
        .iter()
        .position(|r| r.id() == owner.id())
        .ok_or(Error::Changed)?;
    let known = |r: &Section| -> Result<bool, Error> {
        Ok(r.section_type()?.unwrap_or(0) == 0
            && matches!(r.style()?.unwrap_or(0), 0..=3)
            && r.parents()?.is_none_or(|p| p.fields().is_empty()))
    };
    if !reachable
        .iter()
        .filter(|r| r.id() != owner.id())
        .map(|r| known(r))
        .collect::<Result<Vec<_>, _>>()?
        .contains(&true)
    {
        return Err(Error::Unsupported);
    }
    if index > 0
        && index + 1 < reachable.len()
        && (!known(reachable[index - 1])? || !known(reachable[index + 1])?)
    {
        return Err(Error::Unsupported);
    }
    Ok(peer)
}

pub(super) fn prove(rows: &[&Section], owner: &Section, now: u64) -> Result<Section, Error> {
    if rows.iter().filter(|r| r.id() == owner.id()).count() != 1
        || !rows.iter().any(|r| *r == owner)
    {
        return Err(Error::Unsupported);
    }
    let child = child(rows, owner)?;
    if child.deleted()? != Some(true) {
        return Err(Error::Unsupported);
    }
    if owner.deleted()? == Some(true) {
        // The bounded command leaves another reachable ordinary paragraph.
        // A component with no live survivor may require replacement semantics.
        let mut survivor = false;
        for &row in rows {
            survivor |= row.id() != owner.id()
                && row.id() != child.id()
                && row.deleted()? != Some(true)
                && row.section_type()?.unwrap_or(0) == 0
                && matches!(row.style()?.unwrap_or(0), 0..=3)
                && row.parents()?.is_none_or(|p| p.fields().is_empty());
        }
        if !survivor {
            return Err(Error::Unsupported);
        }
    }
    let cw = child.crdt()?.ok_or(Error::Unsupported)?;
    if cw.has(10) || cw.has(12) || cw.uint(3)? != child.sequence()? {
        return Err(Error::Unsupported);
    }
    let cd = text::canonical_draft(child)?;
    let doc = crdt::validate(
        &cw,
        child,
        &cd,
        true,
        Some(now),
        position::modern_id(child.id()),
    )?;
    require_processed(&cw, &doc)?;
    let live_child = deletion_prefix(&cw, child, &cd)?;
    let live_owner = if owner.deleted()? == Some(true) {
        let ow = owner.crdt()?.ok_or(Error::Unsupported)?;
        if ow.has(12) || ow.uint(3)? != owner.sequence()? {
            return Err(Error::Unsupported);
        }
        let draft = text::canonical_draft(owner)?;
        history(
            &ow,
            owner,
            &draft,
            true,
            Some(now),
            position::modern_id(owner.id()),
        )?;
        live_prefix(&ow, owner, &draft)?
    } else {
        owner.clone()
    };
    let projected = rows.iter().map(|r| {
        if r.id() == owner.id() {
            &live_owner
        } else if r.id() == child.id() {
            &live_child
        } else {
            *r
        }
    });
    // This proves identities/scope/positions, the single raw edge, complete
    // source and supported CONTENT descendants plus all independent gaps.
    let proved_owner =
        split::child_delete_owner(projected, &live_child, now)?.ok_or(Error::Unsupported)?;
    if proved_owner != live_owner {
        return Err(Error::Unsupported);
    }
    Ok(child.clone())
}
