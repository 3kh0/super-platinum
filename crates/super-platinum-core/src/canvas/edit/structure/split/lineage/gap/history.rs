use super::*;
use automerge::{
    ObjId, ObjType, PatchAction, ScalarValue, Value,
    legacy::{ElementId, Key, ObjectId, OpType},
};

pub(super) fn ordinary(row: &Section, now: u64) -> Result<(), Error> {
    ordinary_policy(row, now, false, false)
}

/// Separate source-proven ordinary replacement history; gap admission is unchanged.
pub(in crate::canvas::edit::structure) fn independent(
    row: &Section,
    now: u64,
) -> Result<(), Error> {
    ordinary_policy(row, now, true, false)
}

/// Only exact-byte certificate evidence may use this path. Opaque Section fields
/// were already preserved by the sealed pair; they confer no history authority.
/// Generic independent/gap admission keeps its original closed field grammar.
pub(in crate::canvas::edit::structure) fn witnessed(row: &Section, now: u64) -> Result<(), Error> {
    ordinary_policy(row, now, true, true)
}

fn ordinary_policy(
    row: &Section,
    now: u64,
    independent: bool,
    witnessed: bool,
) -> Result<(), Error> {
    if row.section_type()?.unwrap_or(0) != 0
        || row
            .sequence()?
            .is_none_or(|n| n == 0 || n > i64::MAX as u64)
        || row.style()?.unwrap_or(0) != 0
        || row.read_only()? == Some(true)
        || row.parents()?.is_some_and(|p| !p.fields().is_empty())
        // Known unsupported Section fields remain refused even at the witnessed
        // seam; only uninterpreted extensions bypass the closed field list.
        || row.raw().fields().iter().any(|f| matches!(f.tag(), 15 | 36 | 39))
        || (!witnessed
            && row.raw().fields().iter().any(|f| {
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
                        | 25
                        | 29
                        | 33
                        | 35
                        | 38
                        | 103
                        | 104
                )
            }))
        || row.raw().uint(11)?.unwrap_or(0) != 0
        || row.raw().boolean(29)? == Some(true)
        || row
            .raw()
            .string(21)?
            .is_some_and(|p| Some(p) != row.raw().string(8).ok().flatten())
    {
        return Err(Error::Unsupported);
    }
    // Original Base.toPb emits content_origin=USER_EDIT (enum0) even for
    // ordinary untouched rows. Preserve that neutral metadata verbatim; other
    // origins and ambiguous encodings do not establish independent gap proof.
    if row.raw().fields().iter().filter(|f| f.tag() == 25).count() > 1
        || row.raw().uint(25)?.is_some_and(|origin| origin != 0)
    {
        return Err(Error::Unsupported);
    }
    if let Some(attributes) = row.attributes()? {
        if attributes.fields().len() > 1
            || attributes.fields().iter().any(|f| f.tag() != 4)
            || attributes.uint(4)?.unwrap_or(0) != 0
        {
            return Err(Error::Unsupported);
        }
    }
    let wire = row.crdt()?.ok_or(Error::Unsupported)?;
    if wire.has(10) || wire.has(12) || wire.uint(3)? != row.sequence()? {
        return Err(Error::Unsupported);
    }
    let draft = text::canonical_draft(row)?;
    let deleted = row.deleted()? == Some(true);
    let doc = super::super::super::super::crdt::validate(
        &wire,
        row,
        &draft,
        deleted,
        Some(now),
        position::modern_id(row.id()),
    )?;
    let heads = doc.get_heads();
    if heads.len() != 1 || !validation::processed(&wire, &heads[0].to_string())? {
        return Err(Error::Unsupported);
    }
    let live = if deleted {
        deletion_prefix(&doc, &wire, &draft, independent)?
    } else {
        doc
    };
    let base: automerge::ChangeHash = wire
        .string(8)?
        .ok_or(Error::Unsupported)?
        .parse()
        .map_err(|_| Error::Unsupported)?;
    let initial = live.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
    bootstrap(&initial, wire.uint(2)? == Some(0), independent)?;
    if independent {
        super::super::rich::independent_descendants(&live, base, &draft, 1)
    } else {
        super::super::rich::descendants(&live, base, &draft, 1)
    }
}

fn bootstrap(doc: &Automerge, zero: bool, independent: bool) -> Result<(), Error> {
    let changes = doc.get_changes(&[]);
    let [change] = changes.as_slice() else {
        return Err(Error::Unsupported);
    };
    let actor = change.actor_id().to_hex_string();
    // The captured replacement-created empty row has a full ten-byte actor,
    // unlike the legacy bootstrap's session-plus-zero-suffix actor. Only the
    // independent zero-base branch admits it; its exact empty bootstrap ops
    // and the following deletion-only history remain mandatory.
    if change.seq() != 1
        || !change.deps().is_empty()
        || actor.len() != 20
        || (!(independent && zero) && !actor.ends_with("00000000"))
    {
        return Err(Error::Unsupported);
    }
    let (_, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let ObjId::Id(counter, object_actor, _) = &object else {
        return Err(Error::Unsupported);
    };
    let expanded = change.decode();
    if expanded.start_op.get() != 1 || expanded.operations.len() > 262_144 {
        return Err(Error::Unsupported);
    }
    let mut made = 0;
    let mut deleted = 0;
    for (i, op) in expanded.operations.iter().enumerate() {
        if !op.pred.is_empty() {
            return Err(Error::Unsupported);
        }
        match (&op.obj, &op.key, &op.action, op.insert) {
            (ObjectId::Root, Key::Map(k), OpType::Make(ObjType::Text), false)
                if k == "text" && *counter == i as u64 + 1 && object_actor == change.actor_id() =>
            {
                made += 1
            }
            (ObjectId::Root, Key::Map(k), OpType::Put(ScalarValue::Boolean(false)), false)
                if k == "deleted" =>
            {
                deleted += 1
            }
            (
                ObjectId::Id(id),
                Key::Seq(ElementId::Head | ElementId::Id(_)),
                OpType::Put(ScalarValue::Str(_)),
                true,
            ) if id.counter() == *counter && id.actor() == object_actor => {}
            _ => return Err(Error::Unsupported),
        }
    }
    if made != 1 || deleted != 1 {
        return Err(Error::Unsupported);
    }
    let text = doc.text(&object).map_err(|_| Error::Unsupported)?;
    let source = text.strip_suffix('\u{feff}').ok_or(Error::Unsupported)?;
    let initial = TextDraft::from_rtml(source)?;
    if initial.model()?.text != source || (zero && !source.is_empty()) {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(doc, &initial, false)
}

fn deletion_prefix(
    doc: &Automerge,
    wire: &RawMessage,
    draft: &TextDraft,
    independent: bool,
) -> Result<Automerge, Error> {
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    let change = doc.get_change_by_hash(head).ok_or(Error::Unsupported)?;
    if change.deps().is_empty() || change.deps().len() > 1024 || change.len() != 1 {
        return Err(Error::Unsupported);
    }
    let prefix = doc.fork_at(change.deps()).map_err(|_| Error::Unsupported)?;
    let prior = prefix.get_heads();
    let [previous] = prior.as_slice() else {
        return Err(Error::Unsupported);
    };
    let actor = change.actor_id().to_hex_string();
    // Original replacement deletion uses a full ten-byte actor too. The old
    // gap proof keeps its suffix policy; both branches prove the exact delta.
    if actor.len() != 20
        || (!independent && !actor.ends_with("00000000"))
        || !validation::processed(wire, &previous.to_string())?
        || prefix.get_changes(&[]).len() + 1 != doc.get_changes(&[]).len()
    {
        return Err(Error::Unsupported);
    }
    let sequence = prefix
        .get_changes(&[])
        .iter()
        .filter(|c| c.actor_id() == change.actor_id())
        .map(|c| c.seq())
        .max()
        .unwrap_or(0);
    let operations = change.decode().operations;
    let [op] = operations.as_slice() else {
        return Err(Error::Unsupported);
    };
    if change.seq() != sequence + 1
        || !matches!((&op.obj,&op.key,&op.action,op.insert),(ObjectId::Root,Key::Map(k),OpType::Put(ScalarValue::Boolean(true)),false) if k=="deleted")
    {
        return Err(Error::Unsupported);
    }
    let patches = doc.diff(&[*previous], &[*head]);
    if patches.len() != 1
        || patches[0].obj != ROOT
        || !matches!(&patches[0].action,PatchAction::PutMap{key,value,conflict:false} if key=="deleted" && value.0==Value::from(true))
    {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(&prefix, draft, false)?;
    Ok(prefix)
}
