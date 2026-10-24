//! Exact new-child prefix and optional post-split CONTENT report proof.
use super::*;
use automerge::Automerge;

pub(super) fn validate_child(
    update: &RawMessage,
    wire: &RawMessage,
    child: &str,
    session: &str,
    witness: &SplitWitness,
) -> Result<(), Error> {
    let section = Section::from_raw(update.clone())?;
    let doc = crdt::validate(wire, &section, witness.child_draft(), false, None, true)?;
    let actor = format!("{session}00000000");
    let base = wire
        .string(8)?
        .ok_or(Error::Unsupported)?
        .parse()
        .map_err(|_| Error::Unsupported)?;
    let initial = doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
    let initial_changes = initial.get_changes(&[]);
    if initial_changes.len() != 1
        || initial_changes[0].actor_id().to_hex_string() != actor
        || initial_changes[0].seq() != 1
        || !initial_changes[0].deps().is_empty()
    {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(&initial, &TextDraft::default(), false)?;
    let changes = doc.get_changes(&[base]);
    let tail = changes.first().ok_or(Error::Unsupported)?;
    if tail.actor_id().to_hex_string() != actor || tail.seq() != 2 || tail.deps() != [base] {
        return Err(Error::Unsupported);
    }
    let tail_doc = doc
        .fork_at(&[tail.hash()])
        .map_err(|_| Error::Unsupported)?;
    if tail_doc.get_changes(&[]).len() != 2 {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(&tail_doc, &witness.child, false)?;
    let tail_wire = child_session(child, &tail_doc, &base.to_string(), &tail_doc.save())?;
    if witness.updates.is_empty() {
        // V3 accepted valid encoded documents without requiring a particular
        // compression representation. Retain those exact durable bytes.
        let expected = child_session(
            child,
            &tail_doc,
            &base.to_string(),
            wire.bytes(5)?.ok_or(Error::Unsupported)?,
        )?;
        if doc.get_changes(&[]).len() != 2 || *wire != expected {
            return Err(Error::Changed);
        }
    } else {
        super::super::end::typing::validate_reports(
            &tail_wire,
            wire,
            false,
            &witness.updates,
            session,
        )?;
    }
    Ok(())
}

pub(super) fn child_session(
    child: &str,
    doc: &Automerge,
    base: &str,
    bytes: &[u8],
) -> Result<RawMessage, Error> {
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    RawMessage::from_fields(&[
        F::bytes(1, child.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(5, bytes)?,
        F::bytes(6, head.to_string().as_bytes())?,
        F::bytes(8, base.as_bytes())?,
        F::bytes(12, head.to_string().as_bytes())?,
    ])
    .map_err(Into::into)
}
