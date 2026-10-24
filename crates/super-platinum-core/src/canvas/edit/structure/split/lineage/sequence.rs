//! Bounded three-row historical association, never inferred from shared base heads.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn parent_history(
    wire: &RawMessage,
    section: &Section,
    current: &TextDraft,
    infos: &[RawMessage],
) -> Result<(), Error> {
    let first = evidence_at(wire, section, current, &infos[0], 0)?;
    let second = evidence_at(wire, section, current, &infos[1], 1)?;
    if first.child == second.child || first.actor != second.actor {
        return Err(Error::Unsupported);
    }
    let before = Automerge::load(infos[0].bytes(7)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let next = Automerge::load(infos[1].bytes(7)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let doc = Automerge::load(wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let first_heads = before.get_heads();
    let changes = next.get_changes(&first_heads);
    if changes.is_empty()
        || next
            .fork_at(&first_heads)
            .map_err(|_| Error::Unsupported)?
            .save()
            != before.save()
        || doc
            .fork_at(&next.get_heads())
            .map_err(|_| Error::Unsupported)?
            .save()
            != next.save()
    {
        return Err(Error::Unsupported);
    }
    let first_change = &changes[0];
    let fork = next
        .fork_at(&[first_change.hash()])
        .map_err(|_| Error::Unsupported)?;
    let (_, obj) = before
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let text = before.text(&obj).map_err(|_| Error::Unsupported)?;
    let source = text.strip_suffix('\u{feff}').ok_or(Error::Unsupported)?;
    let offset = usize::try_from(infos[0].uint(4)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let boundary = utf16_boundary(source, offset).ok_or(Error::Unsupported)?;
    text::crdt::check_document_state(&fork, &TextDraft::from_rtml(&source[..boundary])?, false)?;
    Ok(())
}
pub(super) fn associated(
    sections: &[&Section],
    target: &Section,
    now: u64,
) -> Result<Option<Vec<Section>>, Error> {
    let mut edges = Vec::new();
    for section in sections {
        if let Ok(Some(wire)) = section.crdt() {
            for info in wire.messages(10).unwrap_or_default() {
                if let Ok(Some(child)) = info.string(3) {
                    edges.push((section.id().to_owned(), child.to_owned()));
                }
            }
        }
    }
    let mut component = BTreeSet::from([target.id().to_owned()]);
    loop {
        let count = component.len();
        for (parent, child) in &edges {
            if component.contains(parent) || component.contains(child) {
                component.insert(parent.clone());
                component.insert(child.clone());
            }
        }
        if component.len() > 3 {
            return Err(Error::Unsupported);
        }
        if count == component.len() {
            break;
        }
    }
    if component.len() < 3 {
        return Ok(None);
    }
    let edges = edges
        .iter()
        .filter(|(p, _)| component.contains(p))
        .collect::<Vec<_>>();
    if edges.len() != 2 {
        return Err(Error::Unsupported);
    }
    let roots = component
        .iter()
        .filter(|id| !edges.iter().any(|(_, c)| c == *id))
        .collect::<Vec<_>>();
    let [root] = roots.as_slice() else {
        return Err(Error::Unsupported);
    };
    let rows = sections
        .iter()
        .filter(|s| component.contains(s.id()))
        .map(|s| (s.id().to_owned(), *s))
        .collect::<BTreeMap<_, _>>();
    if rows.len() != 3
        || sections
            .iter()
            .filter(|s| component.contains(s.id()))
            .count()
            != 3
    {
        return Err(Error::Changed);
    }
    for row in rows.values() {
        if row.thread_id()? != target.thread_id()?
            || row.document_id()? != target.document_id()?
            || row.section_type()?.unwrap_or(0) != 0
            || row.style()?.unwrap_or(0) != 0
            || row
                .sequence()?
                .is_none_or(|s| s == 0 || s > i64::MAX as u64)
            || row.deleted()? == Some(true)
            || row.read_only()? == Some(true)
            || row.raw().uint(11)?.unwrap_or(0) != 0
            || row.raw().boolean(29)? == Some(true)
            || row.raw().has(15)
            || row.parents()?.is_some_and(|p| !p.fields().is_empty())
        {
            return Err(Error::Changed);
        }
    }
    let root_section = rows.get(root.as_str()).ok_or(Error::Changed)?;
    let root_wire = root_section.crdt()?.ok_or(Error::Unsupported)?;
    let infos = root_wire.messages(10)?;
    let mut ordered = Vec::new();
    for s in sections {
        let pos = s.raw().string(8)?.ok_or(Error::Unsupported)?;
        position::canonical(pos)?;
        ordered.push((pos, s.id()));
    }
    ordered.sort();
    let start = ordered
        .iter()
        .position(|(_, id)| *id == root_section.id())
        .ok_or(Error::Changed)?;
    let expected = if infos.len() == 2 {
        vec![
            root_section.id().to_owned(),
            infos[1].string(3)?.ok_or(Error::Unsupported)?.to_owned(),
            infos[0].string(3)?.ok_or(Error::Unsupported)?.to_owned(),
        ]
    } else if infos.len() == 1 {
        let child = infos[0].string(3)?.ok_or(Error::Unsupported)?;
        let child_wire = rows
            .get(child)
            .ok_or(Error::Changed)?
            .crdt()?
            .ok_or(Error::Unsupported)?;
        let child_infos = child_wire.messages(10)?;
        if child_infos.len() != 1 || child_infos[0].string(2)? != infos[0].string(2)? {
            return Err(Error::Unsupported);
        }
        vec![
            root_section.id().into(),
            child.into(),
            child_infos[0].string(3)?.ok_or(Error::Unsupported)?.into(),
        ]
    } else {
        return Err(Error::Unsupported);
    };
    if ordered.get(start..start + 3).is_none_or(|slice| {
        slice
            .iter()
            .map(|(_, id)| *id)
            .ne(expected.iter().map(String::as_str))
    }) {
        return Err(Error::Changed);
    }
    for (parent_id, child_id) in edges {
        let parent = rows.get(parent_id).ok_or(Error::Changed)?;
        let child = rows.get(child_id).ok_or(Error::Changed)?;
        if child.thread_id()? != parent.thread_id()?
            || child.document_id()? != parent.document_id()?
            || child.section_type()?.unwrap_or(0) != 0
            || child.style()?.unwrap_or(0) != 0
            || child.deleted()? == Some(true)
            || child.read_only()? == Some(true)
            || child.parents()?.is_some_and(|p| !p.fields().is_empty())
        {
            return Err(Error::Changed);
        }
        let wire = parent.crdt()?.ok_or(Error::Unsupported)?;
        let draft = text::canonical_draft(parent)?;
        text::crdt::validate(&wire, parent, &draft, Some(now))?;
        let infos = wire.messages(10)?;
        let index = infos
            .iter()
            .position(|i| i.string(3).ok().flatten() == Some(child_id.as_str()))
            .ok_or(Error::Unsupported)?;
        let ev = evidence_at(&wire, parent, &draft, &infos[index], index as u64)?;
        child_history(child, &ev, now, true)?;
    }
    Ok(Some(
        rows.values()
            .filter(|s| s.id() != target.id())
            .map(|s| (*s).clone())
            .collect(),
    ))
}
