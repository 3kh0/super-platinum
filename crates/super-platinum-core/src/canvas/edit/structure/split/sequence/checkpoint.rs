//! Inert proof of all three reports and both exact historical boundaries.
use super::*;
use automerge::{Automerge, ReadDoc};
#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure) fn validate(
    original: &Section,
    child: &str,
    position: &str,
    second_child: &str,
    second_position: &str,
    session: &str,
    witness: &SplitWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    let witness = witness.checked()?;
    let second = witness.second.as_ref().ok_or(Error::Unsupported)?;
    if second_child == child || second_child == original.id() || !position::modern_id(second_child)
    {
        return Err(Error::Changed);
    }
    let rows = data.messages(1)?;
    let [parent, first_child, last] = rows.as_slice() else {
        return Err(Error::Changed);
    };
    let pw = parent.message(38)?.ok_or(Error::Unsupported)?;
    let cw = first_child.message(38)?.ok_or(Error::Unsupported)?;
    let tw = last.message(38)?.ok_or(Error::Unsupported)?;
    let pi = pw.messages(10)?;
    let ci = cw.messages(10)?;
    let (first_info, second_info, target_wire) = match second.target {
        SplitSequenceTarget::Original if pi.len() == 2 && ci.is_empty() => (&pi[0], &pi[1], &pw),
        SplitSequenceTarget::FirstChild if pi.len() == 1 && ci.len() == 1 => (&pi[0], &ci[0], &cw),
        _ => return Err(Error::Changed),
    };
    if tw.has(10) {
        return Err(Error::Changed);
    }
    let title = data.message(14)?.ok_or(Error::Unsupported)?;
    let flat = witness.flattened()?;
    let single_pw = if pi.len() == 1 {
        pw.clone()
    } else {
        pw.replace_fields(10, &[F::message(10, first_info)?])?
    };
    let ordinary_cw = cw.remove(10)?;
    let pair = prepared::project(
        original,
        child,
        position,
        session,
        &flat,
        &single_pw,
        &ordinary_cw,
        &title,
    )?;
    super::super::checkpoint::validate_checkpoint_split(
        original, child, position, session, &flat, &pair,
    )?;
    let bytes = second_info.bytes(7)?.ok_or(Error::Unsupported)?;
    let prefix = Automerge::load(bytes).map_err(|_| Error::Unsupported)?;
    text::crdt::check_document_state(&prefix, &second.enter, false)?;
    let heads = prefix.get_heads();
    if heads.len() != 1 {
        return Err(Error::Changed);
    }
    let final_doc = Automerge::load(target_wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    if final_doc
        .fork_at(&heads)
        .map_err(|_| Error::Changed)?
        .save()
        != bytes
    {
        return Err(Error::Changed);
    }
    let (seed, reports, reused, index) = match second.target {
        SplitSequenceTarget::Original => {
            let source = Automerge::load(first_info.bytes(7)?.ok_or(Error::Unsupported)?)
                .map_err(|_| Error::Unsupported)?;
            let seed = if let Some(seed) = original.crdt()? {
                seed
            } else {
                let revision = original.sequence()?.ok_or(Error::Unsupported)?;
                let source_heads = source.get_heads();
                if source_heads.len() != 1 {
                    return Err(Error::Changed);
                }
                RawMessage::from_fields(&[
                    F::bytes(1, original.id().as_bytes())?,
                    F::uint(2, revision)?,
                    F::uint(3, revision - revision % 1000)?,
                    F::bytes(5, &source.save())?,
                    F::bytes(6, source_heads[0].to_string().as_bytes())?,
                    F::bytes(8, source_heads[0].to_string().as_bytes())?,
                ])?
            };
            let mut reports = vec![witness.parent.clone()];
            reports.extend(witness.parent_updates.iter().cloned());
            (seed, reports, true, 1)
        }
        SplitSequenceTarget::FirstChild => {
            let base = target_wire
                .string(8)?
                .ok_or(Error::Unsupported)?
                .parse()
                .map_err(|_| Error::Unsupported)?;
            let initial = final_doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
            let seed = typing::child_session(child, &initial, &base.to_string(), &initial.save())?;
            let mut reports = vec![witness.child.clone()];
            reports.extend(witness.updates.iter().cloned());
            (seed, reports, false, 0)
        }
    };
    let live = super::super::super::end::typing::project_session(
        &seed,
        reused,
        bytes,
        &heads[0].to_string(),
    )?;
    super::super::super::end::typing::validate_reports(&seed, &live, reused, &reports, session)?;
    if *second_info != build::lineage(&live, second_child, &second.parent, session, index)? {
        return Err(Error::Changed);
    }
    // Exact live prefix and next truncation, not a same-text descendant.
    let next = final_doc.get_changes(&heads);
    let truncate = next.first().ok_or(Error::Changed)?;
    if truncate.deps() != heads
        || truncate.actor_id().to_hex_string() != format!("{session}00000000")
    {
        return Err(Error::Changed);
    }
    text::crdt::check_document_state(
        &final_doc
            .fork_at(&[truncate.hash()])
            .map_err(|_| Error::Changed)?,
        &second.parent,
        false,
    )?;
    let mut tail = flat.clone();
    tail.child = second.child.clone();
    tail.updates = second.updates[2].clone();
    typing::validate_child(last, &tw, second_child, session, &tail)?;
    let state = State {
        child: second_child.into(),
        position: second_position.into(),
        wire: tw,
        lineage: second_info.clone(),
    };
    if *data
        != project(
            original, child, position, session, &witness, &pw, &cw, &state, &title,
        )?
    {
        return Err(Error::Changed);
    }
    Ok(())
}
