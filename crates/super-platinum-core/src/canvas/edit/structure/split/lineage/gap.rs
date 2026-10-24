//! Independent ordinary rows between one saved named owner/child pair.
//! Their order is visual; they never become members of split lineage.
use super::*;
mod history;

pub(super) fn between(
    rows: &[&Section],
    owner: &Section,
    child: &Section,
    now: u64,
) -> Result<(), Error> {
    let left = owner.raw().string(8)?.ok_or(Error::Unsupported)?;
    let right = child.raw().string(8)?.ok_or(Error::Unsupported)?;
    position::canonical(left)?;
    position::canonical(right)?;
    if left >= right || rows.len() > crate::canvas::wire::MAX_SECTIONS {
        return Err(Error::Unsupported);
    }
    let mut positions = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    let mut count = 0usize;
    for &row in rows {
        bytes = bytes
            .checked_add(row.raw().raw().len())
            .ok_or(Error::Unsupported)?;
        let pos = row.raw().string(8)?.ok_or(Error::Unsupported)?;
        position::canonical(pos)?;
        if bytes > crate::canvas::wire::MAX_BYTES
            || !positions.insert(pos)
            || !ids.insert(row.id())
            || row.document_id()? != owner.document_id()?
            || row.thread_id()? != owner.thread_id()?
        {
            return Err(Error::Unsupported);
        }
        if left < pos && pos < right {
            history::ordinary(row, now)?;
            count += 1;
        }
    }
    if count == 0 {
        return Err(Error::Unsupported);
    }
    Ok(())
}

/// Ordinary deleted rows are trusted only inside a separately proved isolated
/// pair interval. Keep their raw identities/positions reserved in Inventory.
pub(in crate::canvas::edit::structure) fn tombstone<'a>(
    rows: impl IntoIterator<Item = &'a Section>,
    target: &Section,
) -> Result<(), Error> {
    if target.deleted()? != Some(true) || target.crdt()?.is_some_and(|w| w.has(10)) {
        return Err(Error::Unsupported);
    }
    let rows = rows.into_iter().collect::<Vec<_>>();
    let pos = target.raw().string(8)?.ok_or(Error::Unsupported)?;
    let mut proved = 0usize;
    for &owner in &rows {
        let Some(wire) = owner.crdt()?.filter(|w| w.has(10)) else {
            continue;
        };
        let infos = wire.messages(10)?;
        let [info] = infos.as_slice() else { continue };
        let child_id = info.string(3)?.ok_or(Error::Unsupported)?;
        let members = rows
            .iter()
            .copied()
            .filter(|r| r.id() == child_id)
            .collect::<Vec<_>>();
        let [child] = members.as_slice() else {
            continue;
        };
        if owner.raw().string(8)?.ok_or(Error::Unsupported)? >= pos
            || child.raw().string(8)?.ok_or(Error::Unsupported)? <= pos
        {
            continue;
        }
        // Neither path calls Inventory: the live path proves every gap through
        // between(), while the historical path already proves its exact pair.
        if owner.deleted()? == Some(true) {
            let peer = super::super::super::owner_delete::tombstone(rows.iter().copied(), owner)?;
            if &peer != *child {
                return Err(Error::Unsupported);
            }
            between(&rows, owner, child, 0)?;
        } else {
            let peer =
                associated_child(rows.iter().copied(), owner, 0)?.ok_or(Error::Unsupported)?;
            if &peer != *child {
                return Err(Error::Unsupported);
            }
        }
        proved += 1;
    }
    if proved != 1 || !rows.iter().any(|r| *r == target) {
        return Err(Error::Unsupported);
    }
    Ok(())
}
