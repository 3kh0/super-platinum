//! Exact end-Enter projection and receipt proof; no mutation is reconstructed.
use super::*;
use automerge::{Automerge, ReadDoc};

pub(super) fn project_parent_session(seed: &RawMessage) -> Result<RawMessage, Error> {
    // Original Session.toPb omits expiry/deletion bookkeeping/current snapshot.
    Ok(seed.remove(4)?.remove(7)?.remove(12)?)
}

pub(super) fn validate_parent_session(
    original: &Section,
    draft: &TextDraft,
    session: &str,
    parent_wire: &RawMessage,
) -> Result<(), Error> {
    let actor = format!("{session}00000000");
    if parent_wire.has(10) || parent_wire.has(12) {
        return Err(Error::Unsupported);
    }
    let parent_doc = text::crdt::validate(parent_wire, original, draft, None)?;
    let heads = parent_doc.get_heads();
    if heads.len() != 1 {
        return Err(Error::Unsupported);
    }
    let expected_parent = if let Some(seed) = original.crdt()? {
        project_parent_session(&seed)?
    } else {
        let changes = parent_doc.get_changes(&[]);
        if changes.len() != 1
            || changes[0].actor_id().to_hex_string() != actor
            || changes[0].seq() != 1
            || !changes[0].deps().is_empty()
        {
            return Err(Error::Unsupported);
        }
        let revision = original.sequence()?.ok_or(Error::Unsupported)?;
        RawMessage::from_fields(&[
            F::bytes(1, original.id().as_bytes())?,
            F::uint(2, revision)?,
            F::uint(3, revision - revision % 1000)?,
            F::bytes(5, parent_wire.bytes(5)?.ok_or(Error::Unsupported)?)?,
            F::bytes(6, heads[0].to_string().as_bytes())?,
            F::bytes(8, heads[0].to_string().as_bytes())?,
        ])?
    };
    if *parent_wire != expected_parent {
        return Err(Error::Changed);
    }
    Ok(())
}

pub(in crate::canvas::edit::structure) fn validate_checkpoint_end(
    original: &Section,
    child: &str,
    position: &str,
    session: &str,
    witness: &EndWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    if witness.empty_enter {
        empty::witness(witness)?;
        empty::admit(original)?;
    } else {
        EndWitness::new(
            &witness.before,
            witness.offset,
            &witness.before,
            &TextDraft::default(),
        )?;
        admit_source(original, &witness.before)?;
    }
    if text::canonical_draft(original)? != witness.before
        || child == original.id()
        || !position::modern_id(child)
        || session.len() != 12
        || !session.bytes().all(|b| b.is_ascii_hexdigit())
        || data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14))
    {
        return Err(Error::Unsupported);
    }
    let updates = data.messages(1)?;
    let [parent, tail] = updates.as_slice() else {
        return Err(Error::Unsupported);
    };
    let parent_wire = parent.message(38)?.ok_or(Error::Unsupported)?;
    let child_wire = tail.message(38)?.ok_or(Error::Unsupported)?;
    let actor = format!("{session}00000000");
    validate_parent_session(original, &witness.before, session, &parent_wire)?;
    let tail_section = Section::from_raw(tail.clone())?;
    let child_doc = crdt::validate(
        &child_wire,
        &tail_section,
        &TextDraft::default(),
        false,
        None,
        true,
    )?;
    let changes = child_doc.get_changes(&[]);
    let heads = child_doc.get_heads();
    if changes.len() != 1
        || heads.len() != 1
        || changes[0].actor_id().to_hex_string() != actor
        || changes[0].seq() != 1
        || !changes[0].deps().is_empty()
    {
        return Err(Error::Unsupported);
    }
    let current = heads[0].to_string();
    let expected_child = RawMessage::from_fields(&[
        F::bytes(1, child.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(5, child_wire.bytes(5)?.ok_or(Error::Unsupported)?)?,
        F::bytes(6, current.as_bytes())?,
        F::bytes(8, current.as_bytes())?,
        F::bytes(12, current.as_bytes())?,
    ])?;
    if child_wire != expected_child {
        return Err(Error::Changed);
    }
    let title = data.message(14)?.ok_or(Error::Unsupported)?;
    if *data
        != prepared::project(
            original,
            child,
            position,
            session,
            &parent_wire,
            &child_wire,
            &title,
        )?
    {
        return Err(Error::Changed);
    }
    Ok(())
}

pub(in crate::canvas::edit::structure) fn history(
    section: &Section,
    draft: &TextDraft,
    sent: &RawMessage,
) -> Result<(), Error> {
    let wire = section.crdt()?.ok_or(Error::Unconfirmed)?;
    if wire.has(9) || wire.has(10) {
        return Err(Error::Unconfirmed);
    }
    let actual = crdt::validate(
        &wire,
        section,
        draft,
        false,
        Some(0),
        sent.uint(2)? == Some(0),
    )?;
    let expected = Automerge::load(sent.bytes(5)?.ok_or(Error::Unconfirmed)?)
        .map_err(|_| Error::Unconfirmed)?;
    // Parent content remains unchanged; child content may include ordered reports.
    // Exact change sets/heads reject fabricated no-ops and unrelated histories.
    if actual.get_heads() != expected.get_heads()
        || actual.get_changes(&[]).len() != expected.get_changes(&[]).len()
        || expected
            .get_changes(&[])
            .iter()
            .any(|c| actual.get_change_by_hash(&c.hash()).is_none())
        || wire.uint(2)? != sent.uint(2)?
        || wire.uint(3)? != section.sequence()?
        || wire.string(8)? != sent.string(8)?
    {
        return Err(Error::Unconfirmed);
    }
    for head in expected.get_heads() {
        if !split::validation::processed(&wire, &head.to_string())? {
            return Err(Error::Unconfirmed);
        }
    }
    Ok(())
}

fn common(p: &PreparedEndEdit, section: &Section, base: u64) -> Result<(), Error> {
    if section.document_id()? != Some(p.document_id())
        || section.thread_id()? != Some(p.thread_id())
        || section
            .sequence()?
            .is_none_or(|n| n <= base || n > i64::MAX as u64)
        || section.raw().uint(19)? != Some(1)
        || section.raw().uint(20)? != Some(1)
        || section.raw().string(33)? != Some(format!("{}-0", p.session_id()).as_str())
        || section.deleted()? == Some(true)
        || section.read_only()? == Some(true)
        || section.section_type()?.unwrap_or(0) != 0
        || section.style()?.unwrap_or(0) != 0
        || section.raw().uint(11)?.unwrap_or(0) != 0
        || section.raw().boolean(29)? == Some(true)
        || section.parents()?.is_some_and(|v| !v.fields().is_empty())
        || section.raw().has(17)
    {
        return Err(Error::Unconfirmed);
    }
    Ok(())
}
pub(super) fn parent(p: &PreparedEndEdit, section: &Section) -> Result<(), Error> {
    let original = &p.intent.original;
    existing(p, section, original, &p.intent.seed, &p.parent_wire)
}
pub(super) fn existing(
    p: &PreparedEndEdit,
    section: &Section,
    original: &Section,
    draft: &TextDraft,
    wire: &RawMessage,
) -> Result<(), Error> {
    common(p, section, original.sequence()?.ok_or(Error::Unconfirmed)?)?;
    // Metadata ACK may omit content. Any supplied content must be identical;
    // omission is never independent readback evidence for the unchanged body.
    if section.raw().has(12)
        && !split::validation::content_equal(section, &text::content(original, draft)?)?
    {
        return Err(Error::Unconfirmed);
    }
    if split::validation::stable_parent(&original.merge_partial(section)?)?
        != split::validation::stable_parent(original)?
    {
        return Err(Error::Unconfirmed);
    }
    history(section, draft, wire)
}
pub(super) fn child(p: &PreparedEndEdit, section: &Section) -> Result<(), Error> {
    if let Some(original) = p.intent.reused() {
        if (p.clears_child() || !p.child_updates().is_empty()) && !section.raw().has(12) {
            return Err(Error::Unconfirmed);
        }
        return existing(p, section, original, p.child_draft(), &p.child_wire);
    }
    new_child(
        p,
        section,
        &p.intent.position,
        p.child_draft(),
        &p.child_wire,
    )
}
pub(super) fn new_child(
    p: &PreparedEndEdit,
    section: &Section,
    inserted_position: &str,
    draft: &TextDraft,
    wire: &RawMessage,
) -> Result<(), Error> {
    common(p, section, 0)?;
    if section.raw().fields().iter().any(|f| {
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
    }) || section.raw().string(8)? != Some(position::base(inserted_position)?)
        || section
            .raw()
            .string(21)?
            .is_some_and(|pos| Some(pos) != section.raw().string(8).ok().flatten())
        || section.attributes()? != Some(RawMessage::from_fields(&[F::uint(4, 0)?])?)
        || !split::validation::content_equal(section, &split::validation::child_content(draft)?)?
    {
        return Err(Error::Unconfirmed);
    }
    history(section, draft, wire)
}
