//! Eligibility fence for rich descendants of one completely saved live pair.
//! Three-row association continues to use the separate plain-history policy.
use super::*;

pub(super) fn isolated_pair(
    sections: &[&Section],
    owner: &Section,
    child: &Section,
    now: u64,
) -> Result<(), Error> {
    if sections.len() > crate::canvas::wire::MAX_SECTIONS || owner.id() == child.id() {
        return Err(Error::Unsupported);
    }
    let mut identities = std::collections::BTreeSet::new();
    let mut owner_found = false;
    let mut child_found = false;
    let mut edges = 0usize;
    let mut bytes = 0usize;
    for &row in sections {
        bytes = bytes
            .checked_add(row.raw().raw().len())
            .ok_or(Error::Unsupported)?;
        if bytes > crate::canvas::wire::MAX_BYTES || !identities.insert(row.id()) {
            return Err(Error::Unsupported);
        }
        if row.id() == owner.id() {
            if row != owner {
                return Err(Error::Changed);
            }
            owner_found = true;
        }
        if row.id() == child.id() {
            if row != child {
                return Err(Error::Changed);
            }
            child_found = true;
        }
        // Parse raw lineage rather than the permissive graph-discovery view:
        // an unrecognized edge cannot establish that this pair is isolated.
        if let Some(wire) = row.crdt()? {
            for info in wire.messages(10)? {
                let target = info.string(3)?.ok_or(Error::Unsupported)?;
                if [owner.id(), child.id()].contains(&row.id())
                    || [owner.id(), child.id()].contains(&target)
                {
                    if row.id() != owner.id() || target != child.id() {
                        return Err(Error::Unsupported);
                    }
                    edges += 1;
                }
            }
        }
    }
    if !owner_found || !child_found || edges != 1 {
        return Err(Error::Unsupported);
    }
    for row in [owner, child] {
        if row.deleted()? == Some(true)
            || row.read_only()? == Some(true)
            || row.section_type()?.unwrap_or(0) != 0
            || row.style()?.unwrap_or(0) != 0
            || row.parents()?.is_some_and(|p| !p.fields().is_empty())
            || row.thread_id()? != owner.thread_id()?
            || row.document_id()? != owner.document_id()?
        {
            return Err(Error::Unsupported);
        }
        let wire = row.crdt()?.ok_or(Error::Unsupported)?;
        if wire.has(12) || wire.uint(3)? != row.sequence()? {
            return Err(Error::Unsupported);
        }
        let doc = text::crdt::validate(&wire, row, &text::canonical_draft(row)?, Some(now))?;
        let heads = doc.get_heads();
        if heads.len() != 1 || !validation::processed(&wire, &heads[0].to_string())? {
            return Err(Error::Unsupported);
        }
    }
    if child.crdt()?.ok_or(Error::Unsupported)?.has(10) {
        return Err(Error::Unsupported);
    }
    Ok(())
}
