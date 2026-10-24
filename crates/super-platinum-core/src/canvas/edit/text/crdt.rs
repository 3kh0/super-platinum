//! Captured Quip text/mark history: escaped RTML text plus a terminal FEFF.
use super::{Error, F, RawMessage, Section, TextDraft};
use automerge::{
    ActorId, Automerge, ObjType, ROOT, ReadDoc, Value,
    marks::ExpandMark,
    transaction::{CommitOptions, Transactable},
};

fn actor(session: &str) -> Result<ActorId, Error> {
    if session.len() != 12 || !session.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Unsupported);
    }
    format!("{session}00000000")
        .parse()
        .map_err(|_| Error::Unsupported)
}
fn seconds(now: u64) -> Result<i64, Error> {
    i64::try_from(now / 1_000_000).map_err(|_| Error::Unsupported)
}

pub(in crate::canvas::edit) fn bootstrap(
    section: &Section,
    draft: &TextDraft,
    session: &str,
    now: u64,
) -> Result<RawMessage, Error> {
    let model = draft.model()?;
    let mut doc = Automerge::new().with_actor(actor(session)?);
    let mut tx = doc.transaction();
    let text = tx
        .put_object(ROOT, "text", ObjType::Text)
        .map_err(|_| Error::Unsupported)?;
    tx.splice_text(&text, 0, 0, &format!("{}\u{feff}", model.text))
        .map_err(|_| Error::Unsupported)?;
    tx.put(ROOT, "deleted", false)
        .map_err(|_| Error::Unsupported)?;
    for mark in model.marks {
        tx.mark(&text, mark, ExpandMark::After)
            .map_err(|_| Error::Unsupported)?;
    }
    tx.commit_with(CommitOptions::default().with_time(seconds(now)?));
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    let seq = section.sequence()?.ok_or(Error::Unsupported)?;
    Ok(RawMessage::from_fields(&[
        F::bytes(1, section.id().as_bytes())?,
        F::uint(2, seq)?,
        F::uint(3, seq - seq % 1000)?,
        F::bytes(5, &doc.save())?,
        F::bytes(6, head.to_string().as_bytes())?,
        F::bytes(8, head.to_string().as_bytes())?,
    ])?)
}

pub(in crate::canvas::edit) fn validate(
    wire: &RawMessage,
    section: &Section,
    draft: &TextDraft,
    now: Option<u64>,
) -> Result<Automerge, Error> {
    if wire.has(10) {
        super::super::structure::split_parent_history(wire, section, draft)?;
        return validate(&wire.remove(10)?, section, draft, now);
    }
    if wire.uint(2)? == Some(0) {
        return super::super::structure::canonical_zero_base_history(wire, section, draft, now);
    }
    if wire
        .fields()
        .iter()
        .any(|f| !matches!(f.tag(), 1..=8 | 11 | 12))
        || wire.string(1)? != Some(section.id())
        || wire.boolean(7)? == Some(true)
        || wire
            .uint(2)?
            .is_none_or(|n| n == 0 || section.sequence().ok().flatten().is_none_or(|seq| n > seq))
        || wire
            .uint(3)?
            .is_none_or(|n| section.sequence().ok().flatten().is_none_or(|seq| n > seq))
    {
        return Err(Error::Unsupported);
    }
    if let Some(now) = now {
        if wire
            .uint(4)?
            .is_none_or(|expires| expires <= now || expires > i64::MAX as u64)
        {
            return Err(Error::Expired);
        }
    }
    let bytes = wire.bytes(5)?.ok_or(Error::Unsupported)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Unsupported);
    }
    let doc = Automerge::load(bytes).map_err(|_| Error::Unsupported)?;
    check_document(&doc, draft)?;
    let mut count = 0;
    for field in wire.fields().iter().filter(|f| f.tag() == 6) {
        let raw = RawMessage::decode(wire.encoded_field(field)?)?;
        known_head(&doc, raw.string(6)?.ok_or(Error::Unsupported)?)?;
        count += 1;
    }
    if count == 0 || count > 1024 {
        return Err(Error::Unsupported);
    }
    known_head(&doc, wire.string(8)?.ok_or(Error::Unsupported)?)?;
    if let Some(head) = wire.string(12)? {
        known_head(&doc, head)?;
    }
    for item in wire.messages(11)? {
        if item.fields().iter().any(|f| !matches!(f.tag(), 1 | 2)) {
            return Err(Error::Unsupported);
        }
        known_head(&doc, item.string(1)?.ok_or(Error::Unsupported)?)?;
        if item
            .uint(2)?
            .is_none_or(|n| n > section.sequence().ok().flatten().unwrap_or(0))
        {
            return Err(Error::Unsupported);
        }
    }
    Ok(doc)
}

fn known_head(doc: &Automerge, value: &str) -> Result<(), Error> {
    let hash = value.parse().map_err(|_| Error::Unsupported)?;
    if doc.get_change_by_hash(&hash).is_none() {
        return Err(Error::Unsupported);
    }
    Ok(())
}

fn check_document(doc: &Automerge, draft: &TextDraft) -> Result<(), Error> {
    check_document_state(doc, draft, false)
}

pub(in crate::canvas::edit) fn check_document_state(
    doc: &Automerge,
    draft: &TextDraft,
    deleted: bool,
) -> Result<(), Error> {
    if doc.keys(ROOT).any(|key| key != "text" && key != "deleted") {
        return Err(Error::Unsupported);
    }
    let (kind, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    if kind != Value::Object(ObjType::Text)
        || doc
            .get(ROOT, "deleted")
            .map_err(|_| Error::Unsupported)?
            .map(|v| v.0)
            != Some(Value::from(deleted))
    {
        return Err(Error::Unsupported);
    }
    let model = draft.model()?;
    if doc.text(&object).map_err(|_| Error::Unsupported)? != format!("{}\u{feff}", model.text) {
        return Err(Error::Changed);
    }
    let raw_marks = doc.marks(&object).map_err(|_| Error::Unsupported)?;
    if raw_marks.len() > 4096
        || raw_marks.iter().any(|mark| {
            !matches!(mark.name(), "b" | "i" | "u" | "del" | "code" | "a")
                || !matches!(
                    mark.value(),
                    automerge::ScalarValue::Boolean(_) | automerge::ScalarValue::Str(_)
                )
        })
    {
        return Err(Error::Unsupported);
    }
    let actual = merged_active_marks(&raw_marks);
    let units = model.text.encode_utf16().count();
    if actual.len() > 4096
        || actual.iter().any(|mark| {
            mark.start >= mark.end
                || mark.end > units
                || !matches!(mark.name(), "b" | "i" | "u" | "del" | "code" | "a")
        })
    {
        return Err(Error::Unsupported);
    }
    // Adjacent equivalent rich runs can split a mark in the draft; compare at
    // each boundary instead of treating serialization boundaries as semantics.
    let signature = |marks: &[automerge::marks::Mark]| {
        let mut result = std::collections::BTreeMap::new();
        for mark in marks {
            for i in mark.start..mark.end {
                result.insert((i, mark.name.to_string()), mark.value.clone());
            }
        }
        result
    };
    if signature(&actual) != signature(&model.marks) {
        return Err(Error::Changed);
    }
    Ok(())
}

pub(in crate::canvas::edit) fn update(
    seed: &RawMessage,
    section: &Section,
    original: &TextDraft,
    desired: &TextDraft,
    session: &str,
    now: u64,
) -> Result<RawMessage, Error> {
    let mut doc = validate(seed, section, original, None)?.with_actor(actor(session)?);
    let (_, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let model = desired.model()?;
    let before_text = doc.get_heads();
    let mut tx = doc.transaction();
    tx.update_text(&object, format!("{}\u{feff}", model.text))
        .map_err(|_| Error::Unsupported)?;
    // The original completes its text phase before computing marks from the
    // resulting document. Keep the intermediate text head in history, but only
    // publish the final report head below. Empty transactions add no change.
    tx.commit_with(CommitOptions::default().with_time(seconds(now)?));
    replay_append_marks(
        &mut doc,
        &object,
        &before_text,
        &original.model()?.text,
        &model.text,
        now,
    )?;
    let mut tx = doc.transaction();
    // E268501 updateMarks: merge equivalent ranges; write false only for
    // removed/changed old ranges, then apply newly added ranges. Existing
    // unchanged marks are not rewritten during an unrelated text insertion.
    let old = merged_active_marks(&tx.marks(&object).map_err(|_| Error::Unsupported)?);
    let desired_marks = merged_active_marks(&model.marks);
    for mark in old.iter().filter(|mark| !desired_marks.contains(mark)) {
        tx.mark(
            &object,
            automerge::marks::Mark::new(mark.name().into(), false, mark.start, mark.end),
            ExpandMark::After,
        )
        .map_err(|_| Error::Unsupported)?;
    }
    for mark in desired_marks.iter().filter(|mark| !old.contains(mark)) {
        tx.mark(&object, mark.clone(), ExpandMark::After)
            .map_err(|_| Error::Unsupported)?;
    }
    tx.commit_with(CommitOptions::default().with_time(seconds(now)?));
    check_document(&doc, desired)?;
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    let mut processed = seed
        .fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            let field = RawMessage::decode(seed.encoded_field(f)?)?;
            Ok(F::bytes(
                6,
                field.string(6)?.ok_or(Error::Unsupported)?.as_bytes(),
            )?)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let head = head.to_string();
    processed.push(F::bytes(6, head.as_bytes())?);
    Ok(seed
        .remove(4)?
        .remove(7)?
        .replace_fields(5, &[F::bytes(5, &doc.save())?])?
        .replace_fields(6, &processed)?
        .replace_fields(12, &[F::bytes(12, head.as_bytes())?])?)
}

// Original applyPatches explicitly reapplies marks carried by a splice before
// updateMarks reconciles the desired report. Automerge inheritance alone has
// the same projection but omits that historical change. This bounded adapter
// covers one appended splice before FEFF, not arbitrary multi-patch edits.
fn replay_append_marks(
    doc: &mut Automerge,
    object: &automerge::ObjId,
    before: &[automerge::ChangeHash],
    original: &str,
    desired: &str,
    now: u64,
) -> Result<(), Error> {
    let Some(suffix) = desired.strip_prefix(original).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let patches = doc.diff(before, &doc.get_heads());
    let [patch] = patches.as_slice() else {
        return Ok(());
    };
    let automerge::PatchAction::SpliceText {
        index,
        value,
        marks: Some(marks),
    } = &patch.action
    else {
        return Ok(());
    };
    if &patch.obj != object
        || *index != original.encode_utf16().count()
        || value.make_string() != suffix
    {
        return Ok(());
    }
    let end = index
        .checked_add(suffix.encode_utf16().count())
        .ok_or(Error::Unsupported)?;
    // E267533 getMarkOrNull excludes false/null; E267657 uses UTF-16
    // splice offset/length. Each active mark is its own original change.
    for (name, value) in marks.iter().filter(|(_, value)| {
        matches!(
            value,
            automerge::ScalarValue::Boolean(true) | automerge::ScalarValue::Str(_)
        )
    }) {
        let mut tx = doc.transaction();
        tx.mark(
            object,
            automerge::marks::Mark::new(name.into(), value.clone(), *index, end),
            ExpandMark::After,
        )
        .map_err(|_| Error::Unsupported)?;
        tx.commit_with(CommitOptions::default().with_time(seconds(now)?));
    }
    Ok(())
}

// E267858 getMarks ignores boolean-false tombstones; E268200 groups by
// (name,value), merges touching ranges, then compares exact resulting marks.
fn merged_active_marks(marks: &[automerge::marks::Mark]) -> Vec<automerge::marks::Mark> {
    let mut groups: Vec<Vec<automerge::marks::Mark>> = Vec::new();
    for mark in marks.iter().filter(|m| {
        matches!(
            m.value(),
            automerge::ScalarValue::Boolean(true) | automerge::ScalarValue::Str(_)
        )
    }) {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group[0].name == mark.name && group[0].value == mark.value)
        {
            group.push(mark.clone());
        } else {
            groups.push(vec![mark.clone()]);
        }
    }
    let mut result = Vec::new();
    for mut group in groups {
        group.sort_by_key(|m| m.start);
        let mut merged: Vec<automerge::marks::Mark> = Vec::new();
        for mark in group {
            if let Some(previous) = merged
                .last_mut()
                .filter(|previous| previous.end >= mark.start)
            {
                previous.end = previous.end.max(mark.end);
            } else {
                merged.push(mark);
            }
        }
        result.extend(merged);
    }
    result
}
