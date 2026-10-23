//! Third-paragraph reports after two end Enters. The first two sessions stay unchanged.
use super::*;

pub(super) fn witness(w: &EndWitness) -> Result<(), Error> {
    if !w.repeat || w.empty_enter || !w.updates.is_empty() || w.second_updates.is_empty() {
        return Err(Error::Unsupported);
    }
    EndWitness::new(&w.before, w.offset, &w.before, &TextDraft::default())?;
    typing::validate_updates(&w.second_updates)
}

pub(super) fn prepared_data(
    intent: &EndIntent,
    w: &EndWitness,
    parent: &RawMessage,
    first: &RawMessage,
    second: &repeat::Second,
    third: &RawMessage,
) -> Result<RawMessage, Error> {
    let mut data = repeat::project(
        &intent.original,
        intent.reused(),
        &intent.child,
        &intent.position,
        second,
        &intent.session,
        parent,
        first,
        third,
        intent.title.rich_text(),
    )?;
    if w.is_repeat_typing() {
        witness(w)?;
        data = with_content(&data, w.second_updates.last().ok_or(Error::Unsupported)?)?;
        validate(
            &intent.original,
            intent.reused(),
            &intent.child,
            &intent.position,
            &second.id,
            &second.position,
            &intent.session,
            &w.before,
            &w.second_updates,
            &data,
        )?;
    } else {
        repeat::witness(w)?;
        repeat::validate(
            &intent.original,
            intent.reused(),
            &intent.child,
            &intent.position,
            &second.id,
            &second.position,
            &intent.session,
            &w.before,
            &data,
        )?;
    }
    Ok(data)
}
fn with_content(data: &RawMessage, draft: &TextDraft) -> Result<RawMessage, Error> {
    let mut rows = data.messages(1)?;
    if rows.len() != 3 {
        return Err(Error::Unsupported);
    }
    // Keep singular-field positions as emitted by the existing creation factory.
    // Repeated row order and all untouched bytes remain exact.
    let replacement = F::message(12, &split::validation::child_content(draft)?)?;
    if rows[2].fields().iter().filter(|f| f.tag() == 12).count() != 1 {
        return Err(Error::Unsupported);
    }
    let mut bytes = Vec::new();
    for field in rows[2].fields() {
        bytes.extend_from_slice(if field.tag() == 12 {
            replacement.raw()
        } else {
            rows[2].encoded_field(field)?
        });
    }
    rows[2] = RawMessage::decode(&bytes)?;
    document(&rows, &data.message(14)?.ok_or(Error::Unsupported)?)
}
fn document(rows: &[RawMessage], title: &RawMessage) -> Result<RawMessage, Error> {
    let mut fields = rows
        .iter()
        .map(|r| F::message(1, r))
        .collect::<Result<Vec<_>, _>>()?;
    fields.push(F::message(14, title)?);
    Ok(RawMessage::from_fields(&fields)?)
}

/// Restore only the independently proven one-change empty bootstrap for history
/// comparison. This creates no intent, dispatch authority or replay operation.
pub(super) fn seed(
    row: &RawMessage,
    session: &str,
    draft: &TextDraft,
) -> Result<RawMessage, Error> {
    let wire = row.message(38)?.ok_or(Error::Unsupported)?;
    let section = Section::from_raw(row.clone())?;
    let doc = crdt::validate(&wire, &section, draft, false, None, true)?;
    let base = wire
        .string(8)?
        .ok_or(Error::Unsupported)?
        .parse()
        .map_err(|_| Error::Unsupported)?;
    let baseline = doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
    text::crdt::check_document_state(&baseline, &TextDraft::default(), false)?;
    let changes = baseline.get_changes(&[]);
    if changes.len() != 1
        || baseline.get_heads() != vec![base]
        || changes[0].actor_id().to_hex_string() != format!("{session}00000000")
        || changes[0].seq() != 1
        || !changes[0].deps().is_empty()
    {
        return Err(Error::Unsupported);
    }
    let id = section.id();
    Ok(RawMessage::from_fields(&[
        F::bytes(1, id.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(5, &baseline.save())?,
        F::bytes(6, base.to_string().as_bytes())?,
        F::bytes(8, base.to_string().as_bytes())?,
        F::bytes(12, base.to_string().as_bytes())?,
    ])?)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure) fn validate(
    original: &Section,
    reused: Option<&Section>,
    child: &str,
    first_position: &str,
    second_id: &str,
    second_position: &str,
    session: &str,
    before: &TextDraft,
    reports: &[TextDraft],
    data: &RawMessage,
) -> Result<(), Error> {
    if reports.is_empty() {
        return Err(Error::Unsupported);
    }
    typing::validate_updates(reports)?;
    let draft = reports.last().ok_or(Error::Unsupported)?;
    let rows = data.messages(1)?;
    let [_, _, third] = rows.as_slice() else {
        return Err(Error::Unsupported);
    };
    let wire = third.message(38)?.ok_or(Error::Unsupported)?;
    let seed = seed(third, session, draft)?;
    typing::validate_reports(&seed, &wire, false, reports, session)?;
    // v12 proves the unchanged original/first and exact new-third creation shape.
    // Substitute only the proven original bootstrap and its empty body for that
    // existing validator, then compare the real typed projection independently.
    let mut baseline = with_content(data, &TextDraft::default())?;
    let mut base_rows = baseline.messages(1)?;
    base_rows[2] = base_rows[2].replace_fields(38, &[F::message(38, &seed)?])?;
    baseline = document(&base_rows, &data.message(14)?.ok_or(Error::Unsupported)?)?;
    repeat::validate(
        original,
        reused,
        child,
        first_position,
        second_id,
        second_position,
        session,
        before,
        &baseline,
    )?;
    let expected = with_content(
        &repeat::project(
            original,
            reused,
            child,
            first_position,
            &repeat::Second {
                id: second_id.into(),
                position: second_position.into(),
            },
            session,
            &rows[0].message(38)?.ok_or(Error::Unsupported)?,
            &rows[1].message(38)?.ok_or(Error::Unsupported)?,
            &wire,
            &data.message(14)?.ok_or(Error::Unsupported)?,
        )?,
        draft,
    )?;
    if *data != expected {
        return Err(Error::Changed);
    }
    Ok(())
}
