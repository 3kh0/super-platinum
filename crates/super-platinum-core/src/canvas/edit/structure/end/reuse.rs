//! Bounded existing-target reuse. Source whitespace/BR/default emptiness is
//! broader than literal empty content; unsupported candidates must not duplicate.
use super::*;

pub(super) fn source_space(c: char) -> bool {
    matches!(c,'\u{9}'..='\u{d}'|'\u{20}'|'\u{a0}'|'\u{1680}'|'\u{2000}'..='\u{200b}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}

pub(in crate::canvas::edit::structure) fn candidate<'a>(
    ordered: &[&'a Section],
    target: &str,
) -> Result<Option<&'a Section>, Error> {
    candidate_mode(ordered, target, false)
}

pub(in crate::canvas::edit::structure) fn candidate_current<'a>(
    ordered: &[&'a Section],
    target: &str,
) -> Result<Option<&'a Section>, Error> {
    candidate_mode(ordered, target, true)
}

fn candidate_mode<'a>(
    ordered: &[&'a Section],
    target: &str,
    allow_clearing: bool,
) -> Result<Option<&'a Section>, Error> {
    let Some(last) = ordered.last().copied().filter(|s| s.id() != target) else {
        return Ok(None);
    };
    if last.section_type()?.unwrap_or(0) != 0 {
        return Ok(None);
    }
    // HIDE_ON_FOCUS default content can be source-empty despite visible text.
    // All default-content policies remain outside this literal-empty cohort.
    if last.attributes()?.is_some_and(|a| a.has(13)) {
        return Err(Error::Unsupported);
    }
    let draft = text::canonical_draft(last)?;
    if !draft.runs.iter().all(|r| r.text.chars().all(source_space)) {
        return Ok(None);
    }
    let text = last
        .raw()
        .message(12)?
        .ok_or(Error::Unsupported)?
        .message(1)?
        .ok_or(Error::Unsupported)?;
    if allow_clearing && clearing::source_draft(last).is_ok() {
        if ordered.len() < 2 || ordered[ordered.len() - 2].id() != target {
            return Err(Error::Unsupported);
        }
        admit_clearing(last)?;
        return Ok(Some(last));
    }
    if text.string(1)? != Some("")
        || draft != TextDraft::default()
        || ordered.len() < 2
        || ordered[ordered.len() - 2].id() != target
    {
        return Err(Error::Unsupported);
    }
    admit_reused(last)?;
    Ok(Some(last))
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
    let Some(child) = candidate_current(&ordered, target)? else {
        return Ok(None);
    };
    inventory.paragraph(child, now)?; // Including expiry, not only raw freshness.
    Ok(Some(child.clone()))
}

pub(in crate::canvas::edit::structure) fn admit_reused(section: &Section) -> Result<(), Error> {
    admit_draft(section, &TextDraft::default())
}

pub(super) fn admit_clearing(section: &Section) -> Result<(), Error> {
    admit_draft(section, &clearing::source_draft(section)?)
}

fn admit_draft(section: &Section, draft: &TextDraft) -> Result<(), Error> {
    if section.section_type()?.unwrap_or(0) != 0
        || section.style()?.unwrap_or(0) != 0
        || section
            .sequence()?
            .is_none_or(|s| s == 0 || s > i64::MAX as u64)
        || section.deleted()? == Some(true)
        || section.read_only()? == Some(true)
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        || section.raw().boolean(29)? == Some(true)
        || section.raw().has(15)
        || section
            .attributes()?
            .is_some_and(|a| a.has(13) || a.has(29))
        || text::canonical_draft(section)? != *draft
    {
        return Err(Error::Unsupported);
    }
    let wire = section.crdt()?.ok_or(Error::Unsupported)?;
    // A canonical modern child may retain base0 after acknowledgement. The
    // shared text validator enforces modern ID, positive canonical revision,
    // last_saved == revision and an acknowledged current history head.
    if wire.has(9) || wire.has(10) || wire.uint(2)?.is_none() {
        return Err(Error::Unsupported);
    }
    let doc = text::crdt::validate(&wire, section, draft, None)?;
    let heads = doc.get_heads();
    if heads.len() != 1 || !split::validation::processed(&wire, &heads[0].to_string())? {
        return Err(Error::Unsupported);
    }
    Ok(())
}

pub(in crate::canvas::edit::structure) fn validate_checkpoint_reuse(
    original: &Section,
    child: &Section,
    session: &str,
    witness: &EndWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    EndWitness::new(
        &witness.before,
        witness.offset,
        &witness.before,
        &TextDraft::default(),
    )?;
    admit_source(original, &witness.before)?;
    admit_reused(child)?;
    if original.id() == child.id()
        || text::canonical_draft(original)? != witness.before
        || session.len() != 12
        || !session.bytes().all(|c| c.is_ascii_hexdigit())
        || data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14))
    {
        return Err(Error::Unsupported);
    }
    let rows = data.messages(1)?;
    let [parent, tail] = rows.as_slice() else {
        return Err(Error::Unsupported);
    };
    let parent_wire = parent.message(38)?.ok_or(Error::Unsupported)?;
    let child_wire = tail.message(38)?.ok_or(Error::Unsupported)?;
    validation::validate_parent_session(original, &witness.before, session, &parent_wire)?;
    if child_wire != validation::project_parent_session(&child.crdt()?.ok_or(Error::Unsupported)?)?
    {
        return Err(Error::Changed);
    }
    let title = data.message(14)?.ok_or(Error::Unsupported)?;
    if *data
        != prepared::project_reuse(original, child, session, &parent_wire, &child_wire, &title)?
    {
        return Err(Error::Changed);
    }
    Ok(())
}
