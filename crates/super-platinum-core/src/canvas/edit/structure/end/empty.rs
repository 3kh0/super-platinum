//! One Enter in a clean canonical empty source. Existing sessions are projected
//! without a content mutation; absence alone permits one initial bootstrap.
use super::*;

pub(super) fn witness(w: &EndWitness) -> Result<(), Error> {
    if !w.empty_enter || w.before != TextDraft::default() || w.offset != 0 || !w.updates.is_empty()
    {
        return Err(Error::Unsupported);
    }
    Ok(())
}

pub(super) fn typing_witness(w: &EndWitness) -> Result<(), Error> {
    if !w.empty_enter || w.before != TextDraft::default() || w.offset != 0 || w.updates.is_empty() {
        return Err(Error::Unsupported);
    }
    typing::validate_updates(&w.updates)
}

/// Recover only the initial clean session of a source whose canonical record
/// omitted CRDT. This is history evidence, never a reconstructed dispatch.
pub(super) fn typed_seed(
    source: &Section,
    wire: &RawMessage,
    session: &str,
) -> Result<RawMessage, Error> {
    admit(source)?;
    if let Some(seed) = source.crdt()? {
        return Ok(seed);
    }
    let doc = automerge::Automerge::load(wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let base = wire
        .string(8)?
        .ok_or(Error::Unsupported)?
        .parse()
        .map_err(|_| Error::Unsupported)?;
    let baseline = doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
    let revision = source.sequence()?.ok_or(Error::Unsupported)?;
    let seed = RawMessage::from_fields(&[
        F::bytes(1, source.id().as_bytes())?,
        F::uint(2, revision)?,
        F::uint(3, revision - revision % 1000)?,
        F::bytes(5, &baseline.save())?,
        F::bytes(6, base.to_string().as_bytes())?,
        F::bytes(8, base.to_string().as_bytes())?,
    ])?;
    validation::validate_parent_session(source, &TextDraft::default(), session, &seed)?;
    Ok(seed)
}

pub(in crate::canvas::edit::structure) fn admit(s: &Section) -> Result<(), Error> {
    if s.section_type()?.unwrap_or(0) != 0
        || s.style()?.unwrap_or(0) != 0
        || s.sequence()?.is_none_or(|n| n == 0 || n > i64::MAX as u64)
        || s.deleted()? == Some(true)
        || s.read_only()? == Some(true)
        || s.parents()?.is_some_and(|p| !p.fields().is_empty())
        || s.raw().uint(11)?.unwrap_or(0) != 0
        || s.raw().boolean(29)? == Some(true)
        || s.raw().has(15)
        || s.attributes()?.is_some_and(|a| a.has(13) || a.has(29))
        || text::canonical_draft(s)? != TextDraft::default()
        || s.raw()
            .message(12)?
            .ok_or(Error::Unsupported)?
            .message(1)?
            .ok_or(Error::Unsupported)?
            .string(1)?
            != Some("")
    {
        return Err(Error::Unsupported);
    }
    if let Some(wire) = s.crdt()? {
        if wire.has(9) || wire.has(10) || wire.uint(2)?.is_none() {
            return Err(Error::Unsupported);
        }
        let doc = text::crdt::validate(&wire, s, &TextDraft::default(), None)?;
        let heads = doc.get_heads();
        if heads.len() != 1 || !split::validation::processed(&wire, &heads[0].to_string())? {
            return Err(Error::Unsupported);
        }
    }
    Ok(())
}

/// Only the immediate final sibling can be reused. A nonfinal literal empty
/// remains untouched and the new blank is positioned before it.
pub(in crate::canvas::edit::structure) fn candidate<'a>(
    ordered: &[&'a Section],
    target: &str,
) -> Result<Option<&'a Section>, Error> {
    let index = ordered
        .iter()
        .position(|s| s.id() == target)
        .ok_or(Error::Unsupported)?;
    let Some(next) = ordered.get(index + 1).copied() else {
        return Ok(None);
    };
    // Reachability/layout/default-content behavior outside the ordinary sibling
    // cohort is not inferred from a global final section or filtered inventory.
    if next.section_type()?.unwrap_or(0) != 0
        || next.style()?.unwrap_or(0) != 0
        || next.parents()?.is_some_and(|p| !p.fields().is_empty())
        || next.read_only()? == Some(true)
        || next.attributes()?.is_some_and(|a| a.has(13) || a.has(29))
    {
        return Err(Error::Unsupported);
    }
    let draft = text::canonical_draft(next)?;
    if index + 2 != ordered.len() {
        return Ok(None);
    }
    if draft
        .runs
        .iter()
        .all(|r| r.text.chars().all(reuse::source_space))
    {
        admit(next)?; // Refuse nonliteral source-empty final content, never duplicate it.
        return Ok(Some(next));
    }
    Ok(None)
}

pub(super) fn select(
    inventory: &Inventory,
    target: &str,
    now: u64,
) -> Result<Option<Section>, Error> {
    let ordered = inventory
        .ordered
        .iter()
        .map(|id| &inventory.sections[id])
        .collect::<Vec<_>>();
    let Some(child) = candidate(&ordered, target)? else {
        return Ok(None);
    };
    inventory.paragraph(child, now)?;
    Ok(Some(child.clone()))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure) fn validate(
    original: &Section,
    reused: Option<&Section>,
    child: &str,
    position: &str,
    session: &str,
    w: &EndWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    witness(w)?;
    admit(original)?;
    let Some(reused) = reused else {
        return validation::validate_checkpoint_end(original, child, position, session, w, data);
    };
    admit(reused)?;
    if original.id() == child
        || reused.id() != child
        || reused.raw().string(8)? != Some(position)
        || session.len() != 12
        || !session.bytes().all(|b| b.is_ascii_hexdigit())
        || data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14))
    {
        return Err(Error::Unsupported);
    }
    let rows = data.messages(1)?;
    let [parent, selected] = rows.as_slice() else {
        return Err(Error::Unsupported);
    };
    let parent_wire = parent.message(38)?.ok_or(Error::Unsupported)?;
    let child_wire = selected.message(38)?.ok_or(Error::Unsupported)?;
    validation::validate_parent_session(original, &TextDraft::default(), session, &parent_wire)?;
    validation::validate_parent_session(reused, &TextDraft::default(), session, &child_wire)?;
    let title = data.message(14)?.ok_or(Error::Unsupported)?;
    if *data
        != prepared::project_reuse(original, reused, session, &parent_wire, &child_wire, &title)?
    {
        return Err(Error::Changed);
    }
    Ok(())
}
