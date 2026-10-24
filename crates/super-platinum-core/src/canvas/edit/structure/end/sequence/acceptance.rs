//! Exact all-member ACK and independent full canonical readback.
use super::*;
use crate::canvas::wire::EditAck;

pub(in crate::canvas::edit::structure::end) fn validate_ack(
    p: &PreparedEndEdit,
    received: &EditAck,
) -> Result<AcceptedEnd, Error> {
    let ack = EditAck::decode(received.raw().raw())?;
    if ack.changed != Some(true)
        || ack.is_new_edit_session == Some(true)
        || ack
            .raw()
            .fields()
            .iter()
            .any(|f| !matches!(f.tag(), 1 | 2 | 6))
    {
        return Err(Error::Unconfirmed);
    }
    let data = ack.final_data.as_ref().ok_or(Error::Unconfirmed)?;
    if data
        .raw()
        .fields()
        .iter()
        .any(|f| !matches!(f.tag(), 1 | 15))
        || data.document_id()?.is_some_and(|id| id != p.document_id())
        || data.sections().len() != p.sequence_rows.len() + 1
    {
        return Err(Error::Unconfirmed);
    }
    let mut by_id = std::collections::BTreeMap::new();
    for s in data.sections() {
        if by_id.insert(s.id(), s).is_some() {
            return Err(Error::Unconfirmed);
        }
    }
    let parent = by_id
        .get(p.target_section())
        .copied()
        .ok_or(Error::Unconfirmed)?;
    if p.parent_updates().is_empty() {
        validation::parent(p, parent)?;
    } else {
        // CONTENT is required even for empty/exact-revert final text.
        if !parent.raw().has(12) {
            return Err(Error::Unconfirmed);
        }
        validation::existing(
            p,
            parent,
            &p.intent.original,
            p.parent_draft(),
            &p.parent_wire,
        )?;
    }
    let mut sections = Vec::new();
    for member in &p.sequence_rows {
        let row = by_id
            .get(member.id.as_str())
            .copied()
            .ok_or(Error::Unconfirmed)?;
        if member.reused {
            let source = p.intent.reused().ok_or(Error::Unconfirmed)?;
            if !member.updates.is_empty() && !row.raw().has(12) {
                return Err(Error::Unconfirmed);
            }
            validation::existing(p, row, source, &member.draft, &member.wire)?;
            sections.push(source.merge_partial(row)?);
        } else {
            validation::new_child(p, row, &member.position, &member.draft, &member.wire)?;
            sections.push(row.clone());
        }
    }
    Ok(AcceptedEnd {
        intent: Arc::clone(&p.intent),
        parent: p.intent.original.merge_partial(parent)?,
        child: sections[0].clone(),
        parent_wire: p.parent_wire.clone(),
        parent_draft: p.parent_draft().clone(),
        child_wire: p.child_wire.clone(),
        child_draft: p.child_draft().clone(),
        second: None,
        second_wire: None,
        second_draft: None,
        repeat_typing: false,
        sequence_sections: sections,
        sequence_rows: p.sequence_rows.clone(),
    })
}
pub(in crate::canvas::edit::structure::end) fn confirms_readback(
    a: &AcceptedEnd,
    snapshot: &CanvasCollabSnapshot,
) -> Result<bool, Error> {
    if inventory::Owner::capture(snapshot)? != a.intent.inventory.owner
        || super::super::super::super::eligible_document(snapshot).is_err()
        || a.intent.title.revalidate(snapshot).is_err()
        || snapshot.sections.len()
            != a.intent.inventory.sections.len()
                + a.sequence_rows.iter().filter(|m| !m.reused).count()
    {
        return Ok(false);
    }
    let mut seen = std::collections::BTreeSet::new();
    let by_id = a
        .sequence_rows
        .iter()
        .zip(&a.sequence_sections)
        .map(|(m, s)| (m.id.as_str(), (m, s)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut positions = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    for section in &snapshot.sections {
        bytes = bytes
            .checked_add(section.raw().raw().len())
            .ok_or(Error::Unsupported)?;
        if !seen.insert(section.id())
            || bytes > crate::canvas::wire::MAX_BYTES
            || section.document_id()? != Some(a.intent.inventory.owner.document.as_str())
            || section.thread_id()? != Some(a.intent.inventory.owner.thread.as_str())
        {
            return Ok(false);
        }
        let pos = section.raw().string(8)?.ok_or(Error::Unconfirmed)?;
        position::canonical(pos)?;
        if !positions.insert(pos) {
            return Ok(false);
        }
        let expected = if section.id() == a.parent.id() {
            Some((&a.parent, &a.parent_draft, &a.parent_wire))
        } else {
            by_id
                .get(section.id())
                .map(|(m, s)| (*s, &m.draft, &m.wire))
        };
        if let Some((acknowledged, draft, wire)) = expected {
            if split::validation::readback_stable(section)?
                != split::validation::readback_stable(acknowledged)?
                || !split::validation::content_equal(
                    section,
                    &acknowledged.raw().message(12)?.ok_or(Error::Unconfirmed)?,
                )?
                || validation::history(section, draft, wire).is_err()
            {
                return Ok(false);
            }
        } else if a.intent.inventory.sections.get(section.id()) != Some(section) {
            return Ok(false);
        }
    }
    Ok(seen.contains(a.parent.id())
        && a.sequence_rows.iter().all(|m| seen.contains(m.id.as_str()))
        && a.intent
            .inventory
            .sections
            .keys()
            .all(|id| seen.contains(id.as_str())))
}
