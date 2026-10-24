//! Source-bound verification of every row and every ordered report. No replay.
use super::*;

pub(in crate::canvas::edit::structure) fn validate(
    original: &Section,
    reused: Option<&Section>,
    session: &str,
    before: &TextDraft,
    parent_updates: &[TextDraft],
    members: &[Member],
    data: &RawMessage,
) -> Result<(), Error> {
    repeat::admit(original, before)?;
    validate_borrowed_stages(
        before,
        parent_updates,
        members.iter().map(|m| m.updates.as_slice()),
    )?;
    if session.len() != 12
        || !session.bytes().all(|b| b.is_ascii_hexdigit())
        || data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14))
    {
        return Err(Error::Unsupported);
    }
    let rows = data.messages(1)?;
    if rows.len() != members.len() + 1 {
        return Err(Error::Changed);
    }
    let parent_wire = rows[0].message(38)?.ok_or(Error::Unsupported)?;
    parent::validate(original, before, session, parent_updates, &rows[0])?;
    let mut seen = std::collections::BTreeSet::from([original.id()]);
    for (i, (member, row)) in members.iter().zip(&rows[1..]).enumerate() {
        if !seen.insert(member.id.as_str())
            || row.string(1)? != Some(member.id.as_str())
            || member.draft != member.updates.last().cloned().unwrap_or_default()
            || row.message(38)?.as_ref() != Some(&member.wire)
        {
            return Err(Error::Changed);
        }
        let source = if i == 0 { reused } else { None };
        if member.reused != source.is_some() {
            return Err(Error::Changed);
        }
        if let Some(source) = source {
            empty::admit(source)?;
            if source.id() != member.id || source.raw().string(8)? != Some(member.position.as_str())
            {
                return Err(Error::Changed);
            }
            if member.updates.is_empty() {
                validation::validate_parent_session(
                    source,
                    &TextDraft::default(),
                    session,
                    &member.wire,
                )?;
            } else {
                let seed = empty::typed_seed(source, &member.wire, session)?;
                let section = Section::from_raw(row.clone())?;
                crdt::validate(
                    &member.wire,
                    &section,
                    &member.draft,
                    false,
                    None,
                    seed.uint(2)? == Some(0),
                )?;
                typing::validate_reports(&seed, &member.wire, true, &member.updates, session)?;
            }
        } else {
            if !position::modern_id(&member.id) || !member.position.ends_with(":temp") {
                return Err(Error::Unsupported);
            }
            let seed = repeat_typing::seed(row, session, &member.draft)?;
            if member.updates.is_empty() {
                // Saved-document byte encoding is not canonical; compare exact history
                // bytes from the outgoing one-change document, plus all session fields.
                let doc =
                    automerge::Automerge::load(member.wire.bytes(5)?.ok_or(Error::Unsupported)?)
                        .map_err(|_| Error::Unsupported)?;
                if doc.get_changes(&[]).len() != 1 || seed.remove(5)? != member.wire.remove(5)? {
                    return Err(Error::Changed);
                }
            } else {
                typing::validate_reports(&seed, &member.wire, false, &member.updates, session)?;
            }
        }
    }
    if *data
        != project(
            original,
            reused,
            session,
            &parent_wire,
            parent_updates.last(),
            members,
            &data.message(14)?.ok_or(Error::Unsupported)?,
        )?
    {
        return Err(Error::Changed);
    }
    Ok(())
}
