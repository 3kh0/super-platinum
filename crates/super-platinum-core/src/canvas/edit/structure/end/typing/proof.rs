//! Read-only validation of the original pair and every retained child change.
//! Ephemeral report planning does not reconstruct an executable intent or
//! transport authority; the persisted request is never replayed.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure) fn validate(
    original: &Section,
    reused: Option<&Section>,
    child: &str,
    position: &str,
    session: &str,
    witness: &EndWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    validate_inner(
        original, reused, child, position, session, witness, data, false, false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure) fn validate_clearing(
    original: &Section,
    reused: &Section,
    child: &str,
    position: &str,
    session: &str,
    witness: &EndWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    validate_inner(
        original,
        Some(reused),
        child,
        position,
        session,
        witness,
        data,
        true,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure) fn validate_empty(
    original: &Section,
    reused: Option<&Section>,
    child: &str,
    position: &str,
    session: &str,
    witness: &EndWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    validate_inner(
        original, reused, child, position, session, witness, data, false, true,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_inner(
    original: &Section,
    reused: Option<&Section>,
    child: &str,
    position: &str,
    session: &str,
    witness: &EndWitness,
    data: &RawMessage,
    clears: bool,
    empty_enter: bool,
) -> Result<(), Error> {
    if empty_enter {
        empty::typing_witness(witness)?;
        empty::admit(original)?;
    } else {
        if witness.empty_enter {
            return Err(Error::Unsupported);
        }
        EndWitness::new(
            &witness.before,
            witness.offset,
            &witness.before,
            &TextDraft::default(),
        )?;
        admit_source(original, &witness.before)?;
    }
    validate_updates(&witness.updates)?;
    let reports = if clears {
        clearing::reports(reused.ok_or(Error::Unsupported)?, &witness.updates)?
    } else {
        witness.updates.clone()
    };
    if reports.is_empty()
        || original.id() == child
        || text::canonical_draft(original)? != witness.before
    {
        return Err(Error::Unsupported);
    }
    // Validate the retained dispatch actor through immutable change metadata.
    if session.len() != 12 || !session.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Unsupported);
    }
    let rows = data.messages(1)?;
    let [parent, tail] = rows.as_slice() else {
        return Err(Error::Unsupported);
    };
    let parent_wire = parent.message(38)?.ok_or(Error::Unsupported)?;
    let wire = tail.message(38)?.ok_or(Error::Unsupported)?;
    validation::validate_parent_session(original, &witness.before, session, &parent_wire)?;
    let section = Section::from_raw(tail.clone())?;
    let final_draft = witness.child_draft();
    // Zero-base permission comes from an independently admitted canonical
    // source, never from the outgoing wire or the fact that it is reused.
    let reused_seed = reused
        .map(|source| {
            if source.id() != child {
                return Err(Error::Changed);
            }
            if empty_enter {
                return empty::typed_seed(source, &wire, session);
            }
            if clears {
                reuse::admit_clearing(source)?;
            } else {
                reuse::admit_reused(source)?;
            }
            source.crdt()?.ok_or(Error::Unsupported)
        })
        .transpose()?;
    let allow_zero = reused_seed
        .as_ref()
        .map_or(Ok(true), |seed| Ok::<_, Error>(seed.uint(2)? == Some(0)))?;
    let doc = crdt::validate(&wire, &section, &final_draft, false, None, allow_zero)?;
    plain_document(&doc)?;
    let actor = format!("{session}00000000");
    let seed = if let Some(seed) = reused_seed {
        seed
    } else {
        if !position::modern_id(child) {
            return Err(Error::Unsupported);
        }
        let base = wire
            .string(8)?
            .ok_or(Error::Unsupported)?
            .parse()
            .map_err(|_| Error::Unsupported)?;
        let baseline = doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
        text::crdt::check_document_state(&baseline, &TextDraft::default(), false)?;
        plain_document(&baseline)?;
        let changes = baseline.get_changes(&[]);
        if changes.len() != 1
            || changes[0].actor_id().to_hex_string() != actor
            || changes[0].seq() != 1
            || !changes[0].deps().is_empty()
        {
            return Err(Error::Unsupported);
        }
        RawMessage::from_fields(&[
            F::bytes(1, child.as_bytes())?,
            F::uint(2, 0)?,
            F::uint(3, 0)?,
            F::bytes(5, &baseline.save())?,
            F::bytes(6, base.to_string().as_bytes())?,
            F::bytes(8, base.to_string().as_bytes())?,
            F::bytes(12, base.to_string().as_bytes())?,
        ])?
    };
    validate_reports(&seed, &wire, reused.is_some(), &reports, session)?;
    let title = data.message(14)?.ok_or(Error::Unsupported)?;
    if *data
        != project(
            original,
            reused,
            child,
            position,
            session,
            &parent_wire,
            &wire,
            &title,
            &final_draft,
        )?
    {
        return Err(Error::Changed);
    }
    Ok(())
}

/// Prove every real history patch against the source-equivalent ordered report
/// plan. The seed is independently admitted by the caller; no replay is exposed.
pub(in crate::canvas::edit::structure) fn validate_reports(
    seed: &RawMessage,
    wire: &RawMessage,
    reused: bool,
    reports: &[TextDraft],
    session: &str,
) -> Result<(), Error> {
    let doc = Automerge::load(wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    plain_document(&doc)?;
    let actor = format!("{session}00000000");
    let baseline = Automerge::load(seed.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    plain_document(&baseline)?;
    let start = baseline.get_heads();
    if start.len() != 1
        || baseline
            .get_changes(&[])
            .iter()
            .any(|c| doc.get_change_by_hash(&c.hash()).is_none())
    {
        return Err(Error::Changed);
    }
    let changes = doc.get_changes(&start);
    if changes.is_empty()
        || baseline.get_changes(&[]).len() + changes.len() != doc.get_changes(&[]).len()
    {
        return Err(Error::Changed);
    }
    let mut sequence = baseline
        .get_changes(&[])
        .iter()
        .filter(|c| c.actor_id().to_hex_string() == actor)
        .map(|c| c.seq())
        .max()
        .unwrap_or(0);
    let (_, object) = baseline
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let mut previous = start;
    let mut group_start = previous.clone();
    let mut group = Vec::new();
    let mut report = 0;
    for change in changes {
        sequence += 1;
        if change.actor_id().to_hex_string() != actor
            || change.seq() != sequence
            || change.deps() != previous
        {
            return Err(Error::Changed);
        }
        let next = vec![change.hash()];
        let patches = doc.diff(&previous, &next);
        let [single] = patches.as_slice() else {
            return Err(Error::Changed);
        };
        group.push(patch(single, &object)?);
        let at = doc.fork_at(&next).map_err(|_| Error::Unsupported)?;
        plain_document(&at)?;
        if let Some(expected) = reports.get(report) {
            if text::crdt::check_document_state(&at, expected, false).is_ok() {
                // Recomputed history diffs lose the original live patch order.
                // A disposable planner reproduces the source CONTENT report;
                // actual history still must match every actor/dependency/patch.
                let start_doc = doc.fork_at(&group_start).map_err(|_| Error::Unsupported)?;
                let planned = report_patches(&start_doc, expected, 0)?
                    .iter()
                    .map(|p| patch(p, &object))
                    .collect::<Result<Vec<_>, _>>()?;
                if planned != group {
                    return Err(Error::Changed);
                }
                report += 1;
                group.clear();
                group_start = next.clone();
            }
        }
        previous = next;
    }
    if report != reports.len() || !group.is_empty() || previous != doc.get_heads() {
        return Err(Error::Changed);
    }
    let expected_wire = project_session(
        &seed,
        reused,
        wire.bytes(5)?.ok_or(Error::Unsupported)?,
        &previous[0].to_string(),
    )?;
    if *wire != expected_wire {
        return Err(Error::Changed);
    }
    Ok(())
}
