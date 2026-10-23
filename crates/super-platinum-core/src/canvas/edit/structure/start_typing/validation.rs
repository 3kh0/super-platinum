use super::*;
use automerge::Automerge;

/// Pure v9 proof: source prefix, every ordered report patch, and empty creation.
#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure) fn validate_request(
    original: &Section,
    child: &str,
    position: &str,
    session: &str,
    witness: &StartTypingWitness,
    data: &RawMessage,
) -> Result<(), Error> {
    StartTypingWitness::new(&witness.before, &witness.updates)?;
    admit(original, &witness.before)?;
    if text::canonical_draft(original)? != witness.before
        || child == original.id()
        || !position::modern_id(child)
        || session.len() != 12
        || !session.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::Unsupported);
    }
    let rows = data.messages(1)?;
    let [parent, new] = rows.as_slice() else {
        return Err(Error::Unsupported);
    };
    let wire = parent.message(38)?.ok_or(Error::Unsupported)?;
    let blank = new.message(38)?.ok_or(Error::Unsupported)?;
    let row = Section::from_raw(original.raw().merge_partial(parent)?)?;
    let doc = text::crdt::validate(&wire, &row, witness.parent_draft(), None)?;
    if wire.has(10) || wire.has(9) {
        return Err(Error::Unsupported);
    }
    let seed = if let Some(seed) = original.crdt()? {
        seed
    } else {
        let base = wire
            .string(8)?
            .ok_or(Error::Unsupported)?
            .parse()
            .map_err(|_| Error::Unsupported)?;
        let baseline = doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
        text::crdt::check_document_state(&baseline, &witness.before, false)?;
        single_bootstrap(&baseline, session)?;
        let revision = original.sequence()?.ok_or(Error::Unsupported)?;
        RawMessage::from_fields(&[
            F::bytes(1, original.id().as_bytes())?,
            F::uint(2, revision)?,
            F::uint(3, revision - revision % 1000)?,
            F::bytes(5, &baseline.save())?,
            F::bytes(6, base.to_string().as_bytes())?,
            F::bytes(8, base.to_string().as_bytes())?,
        ])?
    };
    end::typing::validate_reports(&seed, &wire, true, &witness.updates, session)?;
    let new_section = Section::from_raw(new.clone())?;
    let blank_doc = crdt::validate(
        &blank,
        &new_section,
        &TextDraft::default(),
        false,
        None,
        true,
    )?;
    single_bootstrap(&blank_doc, session)?;
    let head = blank_doc.get_heads()[0].to_string();
    let expected = RawMessage::from_fields(&[
        F::bytes(1, child.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(5, blank.bytes(5)?.ok_or(Error::Unsupported)?)?,
        F::bytes(6, head.as_bytes())?,
        F::bytes(8, head.as_bytes())?,
        F::bytes(12, head.as_bytes())?,
    ])?;
    if blank != expected
        || *data
            != prepared::project(
                original,
                child,
                position,
                session,
                witness.parent_draft(),
                &wire,
                &blank,
                &data.message(14)?.ok_or(Error::Unsupported)?,
            )?
    {
        return Err(Error::Changed);
    }
    Ok(())
}
fn single_bootstrap(doc: &Automerge, session: &str) -> Result<(), Error> {
    let changes = doc.get_changes(&[]);
    if changes.len() != 1
        || doc.get_heads().len() != 1
        || changes[0].actor_id().to_hex_string() != format!("{session}00000000")
        || changes[0].seq() != 1
        || !changes[0].deps().is_empty()
    {
        return Err(Error::Unsupported);
    }
    Ok(())
}
fn common(p: &PreparedStartTypingEdit, s: &Section, base: u64) -> Result<(), Error> {
    if s.document_id()? != Some(p.document_id())
        || s.thread_id()? != Some(p.thread_id())
        || s.sequence()?
            .is_none_or(|n| n <= base || n > i64::MAX as u64)
        || s.raw().uint(19)? != Some(1)
        || s.raw().uint(20)? != Some(1)
        || s.raw().string(33)? != Some(format!("{}-0", p.session_id()).as_str())
        || s.deleted()? == Some(true)
        || s.read_only()? == Some(true)
        || s.section_type()?.unwrap_or(0) != 0
        || s.style()?.unwrap_or(0) != 0
        || s.raw().uint(11)?.unwrap_or(0) != 0
        || s.raw().boolean(29)? == Some(true)
        || s.parents()?.is_some_and(|v| !v.fields().is_empty())
        || s.raw().has(17)
    {
        return Err(Error::Unconfirmed);
    }
    Ok(())
}
pub(super) fn parent(p: &PreparedStartTypingEdit, s: &Section) -> Result<(), Error> {
    let original = &p.intent.original;
    common(p, s, original.sequence()?.ok_or(Error::Unconfirmed)?)?;
    if !split::validation::content_equal(s, &text::content(original, p.parent_draft())?)?
        || split::validation::stable_parent(&original.merge_partial(s)?)?
            != split::validation::stable_parent(original)?
    {
        return Err(Error::Unconfirmed);
    }
    end::validation::history(s, p.parent_draft(), &p.parent_wire)
}
pub(super) fn child(p: &PreparedStartTypingEdit, s: &Section) -> Result<(), Error> {
    common(p, s, 0)?;
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
    }) || s.raw().string(8)? != Some(position::base(&p.intent.position)?)
        || s.raw()
            .string(21)?
            .is_some_and(|v| Some(v) != s.raw().string(8).ok().flatten())
        || s.attributes()? != Some(RawMessage::from_fields(&[F::uint(4, 0)?])?)
        || !split::validation::content_equal(
            s,
            &split::validation::child_content(&TextDraft::default())?,
        )?
    {
        return Err(Error::Unconfirmed);
    }
    end::validation::history(s, &TextDraft::default(), &p.child_wire)
}
