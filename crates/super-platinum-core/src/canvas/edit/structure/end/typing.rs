//! Ordered plain child CONTENT reports, separate from native event attestation.
use super::*;
use automerge::transaction::{CommitOptions, Transactable};
use automerge::{ActorId, Automerge, Patch, PatchAction, PatchLog, ROOT, ReadDoc};

mod proof;
pub(in crate::canvas::edit::structure) use proof::validate_reports;
pub(in crate::canvas::edit::structure) use proof::{validate, validate_clearing, validate_empty};

// Local resource bounds, not Slack limits. Per-draft and final-history limits
// are enforced independently by the existing draft/CRDT validators.
pub(super) const MAX_REPORTS: usize = 4096;
pub(super) const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn validate_updates(updates: &[TextDraft]) -> Result<(), Error> {
    validate_updates_from(&TextDraft::default(), updates)
}

pub(in crate::canvas::edit::structure) fn validate_updates_from(
    initial: &TextDraft,
    updates: &[TextDraft],
) -> Result<(), Error> {
    if updates.len() > MAX_REPORTS {
        return Err(Error::Unsupported);
    }
    let empty = TextDraft::default();
    let mut previous = initial;
    let mut bytes = 0usize;
    for draft in updates {
        if draft == previous {
            return Err(Error::Changed);
        }
        if draft != &empty {
            split::plain(draft)?;
        }
        bytes = bytes
            .checked_add(draft.rtml()?.len())
            .ok_or(Error::Unsupported)?;
        if bytes > MAX_REPORT_BYTES {
            return Err(Error::Unsupported);
        }
        previous = draft;
    }
    Ok(())
}

fn actor(session: &str) -> Result<ActorId, Error> {
    if session.len() != 12 || !session.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Unsupported);
    }
    format!("{session}00000000")
        .parse()
        .map_err(|_| Error::Unsupported)
}

fn plain_document(doc: &Automerge) -> Result<(), Error> {
    let (_, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    if !doc
        .marks(&object)
        .map_err(|_| Error::Unsupported)?
        .is_empty()
    {
        return Err(Error::Unsupported);
    }
    Ok(())
}

fn patch(patch: &Patch, object: &automerge::ObjId) -> Result<(usize, isize, String), Error> {
    if &patch.obj != object {
        return Err(Error::Unsupported);
    }
    match &patch.action {
        PatchAction::SpliceText {
            index,
            value,
            marks,
        } if marks.is_none() => {
            let value = value.make_string();
            if value.is_empty() {
                return Err(Error::Unsupported);
            }
            Ok((*index, 0, value))
        }
        PatchAction::DeleteSeq { index, length } if *length > 0 => Ok((
            *index,
            isize::try_from(*length).map_err(|_| Error::Unsupported)?,
            String::new(),
        )),
        _ => Err(Error::Unsupported),
    }
}

/// Original A.diff uses mostRecentPatch from the live provisional change.
/// A reconstructed history diff can reverse adjacent deletion/insertion order,
/// even in the original WASM after save/load. Preserve transaction patch order.
/// The disposable clone grants no executable intent or transport authority.
fn report_patches(doc: &Automerge, draft: &TextDraft, time: i64) -> Result<Vec<Patch>, Error> {
    let mut provisional = doc.clone();
    let (_, object) = provisional
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let mut tx = provisional.transaction_log_patches(PatchLog::active());
    tx.update_text(&object, format!("{}\u{feff}", draft.model()?.text))
        .map_err(|_| Error::Unsupported)?;
    let (_, mut log) = tx.commit_with(CommitOptions::default().with_time(time));
    let patches = provisional.make_patches(&mut log);
    if patches.is_empty() {
        return Err(Error::Changed);
    }
    Ok(patches)
}

/// Source applies every live provisional text patch as a separate real change.
/// A replacement can produce several changes within one CONTENT report.
pub(super) fn build(
    seed: &RawMessage,
    reused: bool,
    updates: &[TextDraft],
    session: &str,
    now: u64,
) -> Result<RawMessage, Error> {
    validate_updates(updates)?;
    if updates.is_empty() {
        return Err(Error::Unsupported);
    }
    build_reports(seed, reused, updates, session, now)
}

// Callers supply a validated empty-start sequence or source-bound clearing plan.
pub(in crate::canvas::edit::structure) fn build_reports(
    seed: &RawMessage,
    reused: bool,
    updates: &[TextDraft],
    session: &str,
    now: u64,
) -> Result<RawMessage, Error> {
    let mut doc = Automerge::load(seed.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?
        .with_actor(actor(session)?);
    plain_document(&doc)?;
    let (_, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let time = i64::try_from(now / 1_000_000).map_err(|_| Error::Unsupported)?;
    for draft in updates {
        let patches = report_patches(&doc, draft, time)?;
        for p in &patches {
            let (index, delete, insert) = patch(p, &object)?;
            let mut tx = doc.transaction();
            tx.splice_text(&object, index, delete, &insert)
                .map_err(|_| Error::Unsupported)?;
            tx.commit_with(CommitOptions::default().with_time(time));
        }
        text::crdt::check_document_state(&doc, draft, false)?;
    }
    let bytes = doc.save();
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Unsupported);
    }
    project_session(seed, reused, &bytes, &doc.get_heads()[0].to_string())
}

pub(in crate::canvas::edit::structure) fn project_session(
    seed: &RawMessage,
    reused: bool,
    bytes: &[u8],
    head: &str,
) -> Result<RawMessage, Error> {
    let old = if reused {
        seed.fields()
            .iter()
            .filter(|f| f.tag() == 6)
            .map(|f| {
                Ok(F::bytes(
                    6,
                    RawMessage::decode(seed.encoded_field(f)?)?
                        .bytes(6)?
                        .ok_or(Error::Unsupported)?,
                )?)
            })
            .collect::<Result<Vec<_>, Error>>()?
    } else {
        Vec::new()
    };
    let mut processed = old.clone();
    processed.push(F::bytes(6, head.as_bytes())?);
    processed.extend(old);
    Ok(seed
        .remove(4)?
        .remove(7)?
        .replace_fields(5, &[F::bytes(5, bytes)?])?
        .replace_fields(6, &processed)?
        .replace_fields(12, &[F::bytes(12, head.as_bytes())?])?)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project(
    parent: &Section,
    reused: Option<&Section>,
    child: &str,
    position: &str,
    session: &str,
    parent_wire: &RawMessage,
    child_wire: &RawMessage,
    title: &RawMessage,
    draft: &TextDraft,
) -> Result<RawMessage, Error> {
    let base = if let Some(existing) = reused {
        prepared::project_reuse(parent, existing, session, parent_wire, child_wire, title)?
    } else {
        prepared::project(
            parent,
            child,
            position,
            session,
            parent_wire,
            child_wire,
            title,
        )?
    };
    let mut rows = base.messages(1)?;
    let content = if let Some(existing) = reused {
        text::content(existing, draft)?
    } else {
        split::validation::child_content(draft)?
    };
    rows[1] = rows[1]
        .replace_fields(9, &[F::uint(9, 0)?])?
        .replace_fields(12, &[F::message(12, &content)?])?;
    Ok(base.replace_fields(
        1,
        &rows
            .iter()
            .map(|r| F::message(1, r))
            .collect::<Result<Vec<_>, _>>()?,
    )?)
}
