//! Saved coalesced owner: plain truncation/report prefix, then rich CONTENT.
//! A historical split head is never fabricated into processed authority.
use super::*;
use automerge::{
    ObjId, ScalarValue,
    legacy::{Key, ObjectId, OpType},
};
use std::collections::HashSet;

pub(super) fn descendants(
    wire: &RawMessage,
    doc: &Automerge,
    split: automerge::ChangeHash,
    draft: &TextDraft,
    split_prefix_len: usize,
) -> Result<(), Error> {
    if wire.bytes(5)?.is_none_or(|b| b.len() > 1024 * 1024) {
        return Err(Error::Unsupported);
    }
    let mut processed = HashSet::new();
    let mut count = 0usize;
    for field in wire.fields().iter().filter(|f| f.tag() == 6) {
        count += 1;
        if count > 1024 {
            return Err(Error::Unsupported);
        }
        let raw = RawMessage::decode(wire.encoded_field(field)?)?;
        let head = raw
            .string(6)?
            .ok_or(Error::Unsupported)?
            .parse::<automerge::ChangeHash>()
            .map_err(|_| Error::Unsupported)?;
        if doc.get_change_by_hash(&head).is_none() {
            return Err(Error::Unsupported);
        }
        processed.insert(head);
    }
    if processed.contains(&split) {
        return Err(Error::Unsupported);
    }
    let changes = doc.get_changes(&[split]);
    if changes.len() > 4096
        || changes.len().checked_add(split_prefix_len) != Some(doc.get_changes(&[]).len())
        || changes
            .iter()
            .try_fold(0usize, |n, c| n.checked_add(c.len()))
            .is_none_or(|n| n > 262_144)
    {
        return Err(Error::Unsupported);
    }
    // Select the FIRST processed descendant. Skipping an earlier rich or
    // malformed boundary in favour of a convenient later plain value is unsafe.
    let boundary = changes
        .iter()
        .position(|c| processed.contains(&c.hash()))
        .ok_or(Error::Unsupported)?;
    if boundary + 1 >= changes.len() {
        return Err(Error::Unsupported);
    }
    let saved = changes[boundary].hash();
    let prefix = doc.fork_at(&[saved]).map_err(|_| Error::Unsupported)?;
    let (_, object) = prefix
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let encoded = prefix.text(&object).map_err(|_| Error::Unsupported)?;
    let value = encoded.strip_suffix('\u{feff}').ok_or(Error::Unsupported)?;
    let plain = TextDraft::from_rtml(value)?;
    text::crdt::check_document_state(&prefix, &plain, false)?;
    plain_descendants(&prefix, split, &plain, split_prefix_len)?;
    let split_change = doc.get_change_by_hash(&split).ok_or(Error::Unsupported)?;
    let split_actor = split_change.actor_id();
    let ObjId::Id(counter, actor, _) = &object else {
        return Err(Error::Unsupported);
    };
    // Coalesced reports belong to the original split actor. Inspect operations,
    // not only net patches: transient hidden ROOT writes must stay forbidden.
    for change in &changes[..=boundary] {
        if change.actor_id() != split_actor {
            return Err(Error::Unsupported);
        }
        for op in change.decode().operations {
            if !matches!(&op.obj, ObjectId::Id(id) if id.counter() == *counter && id.actor() == actor)
                || !matches!(op.key, Key::Seq(_))
                || !matches!(op.action, OpType::Put(ScalarValue::Str(_)) | OpType::Delete)
            {
                return Err(Error::Unsupported);
            }
        }
    }
    let prefix_len = split_prefix_len
        .checked_add(boundary + 1)
        .ok_or(Error::Unsupported)?;
    // Shared grammar proves exact prefix inclusion, linear actor advances,
    // known ancestor dependencies, supported marks/removals and final state.
    rich::descendants(doc, saved, draft, prefix_len)
}
