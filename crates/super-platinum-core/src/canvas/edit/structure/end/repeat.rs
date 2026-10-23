//! Exactly two end Enters. No content report, tail lineage or third action.
use super::*;

pub(super) struct Second {
    pub id: String,
    pub position: String,
}

pub(super) fn witness(w: &EndWitness) -> Result<(), Error> {
    if !w.repeat || w.empty_enter || !w.updates.is_empty() || !w.second_updates.is_empty() {
        return Err(Error::Unsupported);
    }
    EndWitness::new(&w.before, w.offset, &w.before, &TextDraft::default())?;
    Ok(())
}

/// Separate cohort: genuine acknowledged-zero sources and missing reused CRDT
/// are allowed here without changing the older single-end admission contract.
pub(in crate::canvas::edit::structure) fn admit(
    s: &Section,
    draft: &TextDraft,
) -> Result<(), Error> {
    split::plain(draft)?;
    if text::canonical_draft(s)? != *draft
        || s.section_type()?.unwrap_or(0) != 0
        || s.style()?.unwrap_or(0) != 0
        || s.sequence()?.is_none_or(|n| n == 0 || n > i64::MAX as u64)
        || s.deleted()? == Some(true)
        || s.read_only()? == Some(true)
        || s.parents()?.is_some_and(|p| !p.fields().is_empty())
        || s.raw().uint(11)?.unwrap_or(0) != 0
        || s.raw().boolean(29)? == Some(true)
        || s.raw().has(15)
        || s.attributes()?.is_some_and(|a| a.has(13) || a.has(29))
    {
        return Err(Error::Unsupported);
    }
    if let Some(wire) = s.crdt()? {
        if wire.has(9) || wire.has(10) || wire.uint(2)?.is_none() {
            return Err(Error::Unsupported);
        }
        let doc = text::crdt::validate(&wire, s, draft, None)?;
        let heads = doc.get_heads();
        if heads.len() != 1 || !split::validation::processed(&wire, &heads[0].to_string())? {
            return Err(Error::Unsupported);
        }
    }
    Ok(())
}

pub(super) fn second_position(
    inventory: &Inventory,
    child: &str,
    first: &str,
) -> Result<String, Error> {
    let base = position::base(first)?;
    let next = inventory
        .ordered
        .iter()
        .filter(|id| id.as_str() != child)
        .map(|id| {
            inventory.sections[id]
                .raw()
                .string(8)?
                .ok_or(Error::Unsupported)
        })
        .collect::<Result<Vec<_>, Error>>()?
        .into_iter()
        .find(|p| *p > base);
    position::between(Some(base), next)
}
pub(super) fn second(
    snapshot: &CanvasCollabSnapshot,
    inventory: &Inventory,
    child: &str,
    first: &str,
) -> Result<Second, Error> {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let id = position::section_id(snapshot, &suffix[..25])?;
    if id == child || inventory.sections.contains_key(&id) {
        return Err(Error::Changed);
    }
    Ok(Second {
        id,
        position: second_position(inventory, child, first)?,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project(
    original: &Section,
    reused: Option<&Section>,
    child: &str,
    first_position: &str,
    second: &Second,
    session: &str,
    parent_wire: &RawMessage,
    child_wire: &RawMessage,
    second_wire: &RawMessage,
    title: &RawMessage,
) -> Result<RawMessage, Error> {
    let pair = if let Some(reused) = reused {
        prepared::project_reuse(original, reused, session, parent_wire, child_wire, title)?
    } else {
        prepared::project(
            original,
            child,
            first_position,
            session,
            parent_wire,
            child_wire,
            title,
        )?
    };
    let second_pair = prepared::project(
        original,
        &second.id,
        &second.position,
        session,
        parent_wire,
        second_wire,
        title,
    )?;
    let rows = pair.messages(1)?;
    let new = second_pair.messages(1)?;
    Ok(RawMessage::from_fields(&[
        F::message(1, &rows[0])?,
        F::message(1, &rows[1])?,
        F::message(1, &new[1])?,
        F::message(14, title)?,
    ])?)
}

fn new_session(row: &RawMessage, id: &str, session: &str) -> Result<RawMessage, Error> {
    let wire = row.message(38)?.ok_or(Error::Unsupported)?;
    let section = Section::from_raw(row.clone())?;
    let doc = crdt::validate(&wire, &section, &TextDraft::default(), false, None, true)?;
    let changes = doc.get_changes(&[]);
    let heads = doc.get_heads();
    if changes.len() != 1
        || heads.len() != 1
        || changes[0].actor_id().to_hex_string() != format!("{session}00000000")
        || changes[0].seq() != 1
        || !changes[0].deps().is_empty()
    {
        return Err(Error::Unsupported);
    }
    let head = heads[0].to_string();
    let expected = RawMessage::from_fields(&[
        F::bytes(1, id.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(5, wire.bytes(5)?.ok_or(Error::Unsupported)?)?,
        F::bytes(6, head.as_bytes())?,
        F::bytes(8, head.as_bytes())?,
        F::bytes(12, head.as_bytes())?,
    ])?;
    if wire != expected {
        return Err(Error::Changed);
    }
    Ok(wire)
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
    data: &RawMessage,
) -> Result<(), Error> {
    admit(original, before)?;
    if original.id() == child
        || original.id() == second_id
        || child == second_id
        || !position::modern_id(second_id)
        || !second_position.ends_with(":temp")
        || session.len() != 12
        || !session.bytes().all(|b| b.is_ascii_hexdigit())
        || data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14))
    {
        return Err(Error::Unsupported);
    }
    position::base(second_position)?;
    let rows = data.messages(1)?;
    let [parent, first, second] = rows.as_slice() else {
        return Err(Error::Unsupported);
    };
    let parent_wire = parent.message(38)?.ok_or(Error::Unsupported)?;
    validation::validate_parent_session(original, before, session, &parent_wire)?;
    let child_wire = if let Some(source) = reused {
        empty::admit(source)?;
        if source.id() != child || source.raw().string(8)? != Some(first_position) {
            return Err(Error::Unsupported);
        }
        let wire = first.message(38)?.ok_or(Error::Unsupported)?;
        validation::validate_parent_session(source, &TextDraft::default(), session, &wire)?;
        wire
    } else {
        if !position::modern_id(child) || !first_position.ends_with(":temp") {
            return Err(Error::Unsupported);
        }
        position::base(first_position)?;
        new_session(first, child, session)?
    };
    let second_wire = new_session(second, second_id, session)?;
    let title = data.message(14)?.ok_or(Error::Unsupported)?;
    let second = Second {
        id: second_id.into(),
        position: second_position.into(),
    };
    if *data
        != project(
            original,
            reused,
            child,
            first_position,
            &second,
            session,
            &parent_wire,
            &child_wire,
            &second_wire,
            &title,
        )?
    {
        return Err(Error::Changed);
    }
    Ok(())
}
