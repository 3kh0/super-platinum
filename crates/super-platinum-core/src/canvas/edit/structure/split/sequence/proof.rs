use super::*;
use crate::canvas::wire::EditAck;
fn lineages<'a>(
    w: &SplitWitness,
    first: &'a RawMessage,
    second: &'a State,
    index: usize,
) -> Vec<&'a RawMessage> {
    match (w.second_split_target(), index) {
        (Some(SplitSequenceTarget::Original), 0) => vec![first, &second.lineage],
        (Some(SplitSequenceTarget::FirstChild), 0) => vec![first],
        (Some(SplitSequenceTarget::FirstChild), 1) => vec![&second.lineage],
        _ => vec![],
    }
}
fn child(
    p: &PreparedSplitEdit,
    s: &Section,
    position: &str,
    draft: &TextDraft,
    wire: &RawMessage,
    infos: &[&RawMessage],
) -> Result<(), Error> {
    validation::common(p, s, 0)?;
    if s.raw().fields().iter().any(|f| {
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
    }) || s.raw().string(8)? != Some(position::base(position)?)
        || s.raw()
            .string(21)?
            .is_some_and(|v| Some(v) != s.raw().string(8).ok().flatten())
        || s.attributes()? != Some(RawMessage::from_fields(&[F::uint(4, 0)?])?)
        || !validation::content_equal(s, &validation::child_content(draft)?)?
    {
        return Err(Error::Unconfirmed);
    }
    validation::history_many(s, draft, wire, infos)
}
pub(in crate::canvas::edit::structure::split) fn ack(
    p: &PreparedSplitEdit,
    received: &EditAck,
) -> Result<AcceptedSplit, Error> {
    let second = p.second.as_ref().ok_or(Error::Unsupported)?;
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
        || data.document_id()?.is_some_and(|v| v != p.document_id())
        || data.sections().len() != 3
    {
        return Err(Error::Unconfirmed);
    }
    let ids = p.member_ids();
    let rows = ids
        .iter()
        .map(|id| {
            data.sections()
                .iter()
                .find(|s| s.id() == *id)
                .ok_or(Error::Unconfirmed)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let drafts = p.member_drafts();
    validation::common(
        p,
        rows[0],
        p.intent.original.sequence()?.ok_or(Error::Unconfirmed)?,
    )?;
    if !validation::content_equal(rows[0], &text::content(&p.intent.original, drafts[0])?)?
        || validation::stable_parent(&p.intent.original.merge_partial(rows[0])?)?
            != validation::stable_parent(&p.intent.original)?
    {
        return Err(Error::Unconfirmed);
    }
    validation::history_many(
        rows[0],
        drafts[0],
        &p.parent_wire,
        &lineages(&p.witness, &p.lineage, second, 0),
    )?;
    child(
        p,
        rows[1],
        &p.intent.position,
        drafts[1],
        &p.child_wire,
        &lineages(&p.witness, &p.lineage, second, 1),
    )?;
    child(p, rows[2], &second.position, drafts[2], &second.wire, &[])?;
    Ok(AcceptedSplit {
        intent: Arc::clone(&p.intent),
        witness: p.witness.clone(),
        parent: p.intent.original.merge_partial(rows[0])?,
        child: rows[1].clone(),
        parent_wire: p.parent_wire.clone(),
        child_wire: p.child_wire.clone(),
        lineage: p.lineage.clone(),
        second: Some(second.clone()),
        second_section: Some(rows[2].clone()),
    })
}
pub(in crate::canvas::edit::structure::split) fn readback(
    p: &AcceptedSplit,
    snapshot: &CanvasCollabSnapshot,
) -> Result<bool, Error> {
    let second = p.second.as_ref().ok_or(Error::Unsupported)?;
    let third = p.second_section.as_ref().ok_or(Error::Unsupported)?;
    if inventory::Owner::capture(snapshot)? != p.intent.inventory.owner
        || super::super::super::super::eligible_document(snapshot).is_err()
        || p.intent.title.revalidate(snapshot).is_err()
        || snapshot.sections.len() != p.intent.inventory.sections.len() + 2
    {
        return Ok(false);
    }
    let expected = [&p.parent, &p.child, third];
    let wires = [&p.parent_wire, &p.child_wire, &second.wire];
    let drafts = p.witness.member_drafts();
    let mut seen = std::collections::BTreeSet::new();
    let mut positions = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    for section in &snapshot.sections {
        bytes = bytes
            .checked_add(section.raw().raw().len())
            .ok_or(Error::Unsupported)?;
        if !seen.insert(section.id())
            || bytes > crate::canvas::wire::MAX_BYTES
            || section.document_id()? != Some(p.intent.inventory.owner.document.as_str())
            || section.thread_id()? != Some(p.intent.inventory.owner.thread.as_str())
        {
            return Ok(false);
        }
        let position = section.raw().string(8)?.ok_or(Error::Unconfirmed)?;
        position::canonical(position)?;
        if !positions.insert(position) {
            return Ok(false);
        }
        if let Some(index) = expected.iter().position(|s| s.id() == section.id()) {
            let ack = expected[index];
            if validation::readback_stable(section)? != validation::readback_stable(ack)?
                || !validation::content_equal(
                    section,
                    &ack.raw().message(12)?.ok_or(Error::Unconfirmed)?,
                )?
                || validation::history_many(
                    section,
                    drafts[index],
                    wires[index],
                    &lineages(&p.witness, &p.lineage, second, index),
                )
                .is_err()
                || !validation::contains_history(section, &ack.crdt()?.ok_or(Error::Unconfirmed)?)?
            {
                return Ok(false);
            }
        } else if p.intent.inventory.sections.get(section.id()) != Some(section) {
            return Ok(false);
        }
    }
    Ok(expected.iter().all(|s| seen.contains(s.id()))
        && p.intent
            .inventory
            .sections
            .keys()
            .all(|id| seen.contains(id.as_str())))
}
