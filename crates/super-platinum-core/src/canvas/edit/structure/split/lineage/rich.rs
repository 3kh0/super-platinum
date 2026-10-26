//! Exact-origin child CONTENT proof. Callers independently establish either a
//! proved retained owner tombstone or an isolated, completely saved live pair.
use super::*;
use automerge::{
    ObjId, ScalarValue,
    legacy::{Key, ObjectId, OpType},
};
use std::collections::{BTreeMap, HashSet};

const MAX_CHANGES: usize = 4096;
const MAX_OPERATIONS: usize = 262_144;

fn mark_value(name: &str, value: &ScalarValue) -> Result<(), Error> {
    match (name, value) {
        ("b" | "i" | "u" | "del" | "code", ScalarValue::Boolean(_))
        | ("a", ScalarValue::Boolean(false)) => Ok(()),
        ("a", ScalarValue::Str(encoded)) => {
            // The supported link mark is exactly an href object, not an
            // arbitrary attributes bag. Reuse TextDraft's value constraints.
            let value: serde_json::Value =
                serde_json::from_str(encoded).map_err(|_| Error::Unsupported)?;
            let object = value.as_object().ok_or(Error::Unsupported)?;
            let href = object
                .get("href")
                .and_then(|v| v.as_str())
                .ok_or(Error::Unsupported)?;
            if object.len() != 1 {
                return Err(Error::Unsupported);
            }
            TextDraft {
                runs: vec![text::TextRun {
                    text: "x".into(),
                    style: text::TextStyle {
                        link: Some(href.into()),
                        ..Default::default()
                    },
                }],
            }
            .validate()
        }
        _ => Err(Error::Unsupported),
    }
}

pub(super) fn descendants(
    doc: &Automerge,
    tail: automerge::ChangeHash,
    draft: &TextDraft,
    prefix_len: usize,
) -> Result<(), Error> {
    descendants_policy(doc, tail, draft, prefix_len, false)
}

pub(super) fn independent_descendants(
    doc: &Automerge,
    tail: automerge::ChangeHash,
    draft: &TextDraft,
    prefix_len: usize,
) -> Result<(), Error> {
    descendants_policy(doc, tail, draft, prefix_len, true)
}

fn descendants_policy(
    doc: &Automerge,
    tail: automerge::ChangeHash,
    draft: &TextDraft,
    prefix_len: usize,
    independent: bool,
) -> Result<(), Error> {
    let prefix = doc.fork_at(&[tail]).map_err(|_| Error::Unsupported)?;
    let prefix_changes = prefix.get_changes(&[]);
    let changes = doc.get_changes(&[tail]);
    if prefix_changes.len() != prefix_len
        || changes.len() > MAX_CHANGES
        || changes.len() + prefix_len != doc.get_changes(&[]).len()
        || changes
            .iter()
            .try_fold(0usize, |n, c| n.checked_add(c.len()))
            .is_none_or(|n| n > MAX_OPERATIONS)
    {
        return Err(Error::Unsupported);
    }
    let (_, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let ObjId::Id(object_counter, object_actor, _) = &object else {
        return Err(Error::Unsupported);
    };
    let mut seen = HashSet::new();
    let mut sequences = BTreeMap::new();
    for change in prefix_changes {
        seen.insert(change.hash());
        sequences.insert(change.actor_id().to_hex_string(), change.seq());
    }
    let mut previous = tail;
    for change in changes {
        let actor = change.actor_id().to_hex_string();
        let next = sequences.get(&actor).copied().unwrap_or(0) + 1;
        // Extra dependencies may only repeat already-proved ancestors. Every
        // next change must depend on the immediately preceding one, excluding
        // concurrent branches even when their text projection happens to agree.
        if actor.len() != 20
            || (!independent && !actor.ends_with("00000000"))
            || change.seq() != next
            || change.deps().is_empty()
            || change.deps().len() > 1024
            || !change.deps().contains(&previous)
            || change.deps().iter().any(|h| !seen.contains(h))
            || change.is_empty()
        {
            return Err(Error::Unsupported);
        }
        for op in change.decode().operations {
            if !matches!(&op.obj, ObjectId::Id(id) if id.counter() == *object_counter && id.actor() == object_actor)
                || !matches!(op.key, Key::Seq(_))
            {
                return Err(Error::Unsupported);
            }
            match &op.action {
                OpType::Put(ScalarValue::Str(_)) | OpType::Delete => {}
                OpType::MarkBegin(mark) if op.insert => mark_value(&mark.name, &mark.value)?,
                OpType::MarkEnd(_) if op.insert => {}
                // In particular, reject even transient ROOT writes, unknown
                // attributes, nested objects and counters hidden by later ops.
                _ => return Err(Error::Unsupported),
            }
        }
        let text = doc
            .text_at(&object, &[change.hash()])
            .map_err(|_| Error::Unsupported)?;
        let rtml = text.strip_suffix('\u{feff}').ok_or(Error::Unsupported)?;
        let plain = TextDraft::from_rtml(rtml)?;
        if plain.model()?.text != rtml {
            return Err(Error::Unsupported);
        }
        let units = rtml.encode_utf16().count();
        let marks = doc
            .marks_at(&object, &[change.hash()])
            .map_err(|_| Error::Unsupported)?;
        if marks.len() > 4096 {
            return Err(Error::Unsupported);
        }
        for mark in marks {
            mark_value(mark.name(), mark.value())?;
            if mark.start >= mark.end || mark.end > units {
                return Err(Error::Unsupported);
            }
        }
        previous = change.hash();
        seen.insert(previous);
        sequences.insert(actor, next);
    }
    if doc.get_heads() != [previous] {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(doc, draft, false)
}
