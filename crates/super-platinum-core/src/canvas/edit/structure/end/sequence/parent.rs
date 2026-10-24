//! Source-bound original-row reports. Untouched v14 parents keep metadata-only proof.
use super::*;

pub(super) fn project(
    original: &Section,
    session: &str,
    wire: &RawMessage,
    draft: Option<&TextDraft>,
) -> Result<RawMessage, Error> {
    let row = prepared::metadata(original, session, wire)?;
    match draft {
        None => Ok(row),
        Some(draft) => Ok(row
            .replace_fields(9, &[F::uint(9, 0)?])?
            .replace_fields(12, &[F::message(12, &text::content(original, draft)?)?])?),
    }
}

pub(super) fn validate(
    original: &Section,
    before: &TextDraft,
    session: &str,
    reports: &[TextDraft],
    row: &RawMessage,
) -> Result<(), Error> {
    let wire = row.message(38)?.ok_or(Error::Unsupported)?;
    let Some(final_draft) = reports.last() else {
        return validation::validate_parent_session(original, before, session, &wire);
    };
    typing::validate_updates_from(before, reports)?;
    let current = Section::from_raw(original.raw().merge_partial(row)?)?;
    let doc = text::crdt::validate(&wire, &current, final_draft, None)?;
    if wire.has(9) || wire.has(10) {
        return Err(Error::Unsupported);
    }
    let seed = match original.crdt()? {
        Some(seed) => seed,
        None => {
            // Recover only an independently proven clean source bootstrap. This
            // disposable history view supplies evidence, never a replay handle.
            let base = wire
                .string(8)?
                .ok_or(Error::Unsupported)?
                .parse()
                .map_err(|_| Error::Unsupported)?;
            let baseline = doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
            let revision = original.sequence()?.ok_or(Error::Unsupported)?;
            let seed = RawMessage::from_fields(&[
                F::bytes(1, original.id().as_bytes())?,
                F::uint(2, revision)?,
                F::uint(3, revision - revision % 1000)?,
                F::bytes(5, &baseline.save())?,
                F::bytes(6, base.to_string().as_bytes())?,
                F::bytes(8, base.to_string().as_bytes())?,
            ])?;
            validation::validate_parent_session(original, before, session, &seed)?;
            seed
        }
    };
    typing::validate_reports(&seed, &wire, true, reports, session)
}
