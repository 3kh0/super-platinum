//! One fresh annotation actor updates annotation marks; original text and
//! formatting are unchanged. No actor is reused after an uncertain operation.
use super::{Error, Existing, compact, range::RangeModel};
use crate::canvas::{
    edit::{F, RawMessage},
    wire::Section,
};
use automerge::{
    ActorId, Automerge, ObjType, ROOT, ReadDoc, ScalarValue, Value,
    marks::{ExpandMark, Mark},
    transaction::{CommitOptions, Transactable},
};

pub(in crate::canvas::edit::annotation) struct PreparedHistory {
    pub wire: RawMessage,
    pub rtml: String,
    pub expires: Option<u64>,
}

pub(super) fn prepare(
    section: &Section,
    original: &Existing,
    desired: &RangeModel,
    session: &str,
    now: u64,
) -> Result<PreparedHistory, Error> {
    let annotation = desired
        .marks
        .last()
        .filter(|mark| mark.name() == "annotation")
        .ok_or(Error::Unsupported)?;
    prepare_change(section, original, desired, Some(annotation), session, now)
}

/// Whole-section creation removes hidden wrappers without adding an inline
/// range. This private path cannot substitute a fabricated all-text selection.
pub(super) fn prepare_removal(
    section: &Section,
    original: &Existing,
    desired: &RangeModel,
    session: &str,
    now: u64,
) -> Result<PreparedHistory, Error> {
    if original.draft.model()?.text != desired.text
        || desired.marks.iter().any(|m| !original.marks.contains(m))
        || original
            .marks
            .iter()
            .any(|m| m.name() != "annotation" && !desired.marks.contains(m))
        || !original
            .marks
            .iter()
            .any(|m| m.name() == "annotation" && !desired.marks.contains(m))
    {
        return Err(Error::Unsupported);
    }
    prepare_change(section, original, desired, None, session, now)
}

fn prepare_change(
    section: &Section,
    original: &Existing,
    desired: &RangeModel,
    annotation: Option<&Mark>,
    session: &str,
    now: u64,
) -> Result<PreparedHistory, Error> {
    if session.len() != 12 || !session.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Unsupported);
    }
    let actor: ActorId = format!("{session}00000000")
        .parse()
        .map_err(|_| Error::Unsupported)?;
    let (seed, expires) = match section.crdt()? {
        Some(seed) => {
            validate_existing(&seed, section, original, Some(now))?;
            let expires = seed.uint(4)?;
            (seed, expires)
        }
        None if original.spans.is_empty() => (
            super::super::super::text::crdt::bootstrap(section, &original.draft, session, now)?,
            None,
        ),
        None => return Err(Error::Unsupported),
    };
    let mut doc = validate_existing(&seed, section, original, None)?.with_actor(actor);
    if doc.get_heads().len() != 1
        || !doc.get_missing_deps(&[]).is_empty()
        || doc
            .get_all(ROOT, "text")
            .map_err(|_| Error::Unsupported)?
            .len()
            != 1
        || doc
            .get_all(ROOT, "deleted")
            .map_err(|_| Error::Unsupported)?
            .len()
            != 1
    {
        return Err(Error::Unsupported);
    }
    let (_, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let mut tx = doc.transaction();
    // Retained Quip updateMarks writes false for removed marks; it does not
    // delete archived controls or their thread metadata. One fresh transaction.
    for prior in &original.marks {
        if prior.name() == "annotation" && !desired.marks.contains(prior) {
            tx.mark(
                &object,
                Mark::new("annotation".into(), false, prior.start, prior.end),
                ExpandMark::After,
            )
            .map_err(|_| Error::Unsupported)?;
        }
    }
    if let Some(annotation) = annotation {
        tx.mark(&object, annotation.clone(), ExpandMark::After)
            .map_err(|_| Error::Unsupported)?;
    }
    tx.commit_with(
        CommitOptions::default()
            .with_time(i64::try_from(now / 1_000_000).map_err(|_| Error::Unsupported)?),
    );
    check_document(&doc, &desired.text, &desired.marks)?;
    let heads = doc.get_heads();
    let [head] = heads.as_slice() else {
        return Err(Error::Unsupported);
    };
    let head = head.to_string();
    let mut processed = seed
        .fields()
        .iter()
        .filter(|field| field.tag() == 6)
        .map(|field| {
            let one = RawMessage::decode(seed.encoded_field(field)?)?;
            F::bytes(6, one.string(6)?.ok_or(Error::Unsupported)?.as_bytes()).map_err(Error::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    processed.push(F::bytes(6, head.as_bytes())?);
    let marks = doc.marks(&object).map_err(|_| Error::Unsupported)?;
    let rtml = compact::render(&desired.text, &marks)?;
    let wire = seed
        .remove(4)?
        .remove(7)?
        .replace_fields(5, &[F::bytes(5, &doc.save())?])?
        .replace_fields(6, &processed)?
        .replace_fields(12, &[F::bytes(12, head.as_bytes())?])?;
    Ok(PreparedHistory {
        wire,
        rtml,
        expires,
    })
}

fn signature(
    marks: &[Mark],
) -> Result<std::collections::BTreeMap<(usize, String), ScalarValue>, Error> {
    let mut signature = std::collections::BTreeMap::new();
    for mark in marks {
        if mark.value() == &ScalarValue::Boolean(false) {
            continue;
        }
        for offset in mark.start..mark.end {
            let key = (offset, mark.name().to_owned());
            if signature.insert(key, mark.value().clone()).is_some() {
                return Err(Error::Unsupported);
            }
        }
    }
    Ok(signature)
}

fn check_document(doc: &Automerge, text: &str, expected_marks: &[Mark]) -> Result<(), Error> {
    if doc.keys(ROOT).any(|key| key != "text" && key != "deleted")
        || doc.get_heads().len() != 1
        || !doc.get_missing_deps(&[]).is_empty()
        || doc
            .get_all(ROOT, "text")
            .map_err(|_| Error::Unsupported)?
            .len()
            != 1
        || doc
            .get_all(ROOT, "deleted")
            .map_err(|_| Error::Unsupported)?
            .len()
            != 1
    {
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
            != Some(Value::from(false))
        || doc.text(&object).map_err(|_| Error::Unsupported)? != format!("{text}\u{feff}")
    {
        return Err(Error::Changed);
    }
    let marks = doc.marks(&object).map_err(|_| Error::Unsupported)?;
    // The ordinary validator stays closed to annotation marks. This separate
    // model permits exactly the sealed range/control, verified below per unit.
    compact::render(text, &marks)?;
    if signature(&marks)? != signature(expected_marks)? {
        return Err(Error::Changed);
    }
    Ok(())
}

pub(super) fn validate(
    wire: &RawMessage,
    section: &Section,
    expected: &RangeModel,
    now: Option<u64>,
) -> Result<Automerge, Error> {
    validate_model(wire, section, &expected.text, &expected.marks, now)
}

pub(super) fn validate_existing(
    wire: &RawMessage,
    section: &Section,
    original: &Existing,
    now: Option<u64>,
) -> Result<Automerge, Error> {
    if original.spans.is_empty() {
        return super::super::super::text::crdt::validate(wire, section, &original.draft, now);
    }
    let text = original.draft.model()?.text;
    validate_model(wire, section, &text, &original.marks, now)
}
fn validate_model(
    wire: &RawMessage,
    section: &Section,
    text: &str,
    marks: &[Mark],
    now: Option<u64>,
) -> Result<Automerge, Error> {
    let sequence = section
        .sequence()?
        .filter(|s| *s > 0 && *s <= i64::MAX as u64)
        .ok_or(Error::Unsupported)?;
    let zero = wire.uint(2)? == Some(0);
    if wire
        .fields()
        .iter()
        .any(|f| !matches!(f.tag(), 1..=8 | 11 | 12))
        || wire.string(1)? != Some(section.id())
        || wire.boolean(7)? == Some(true)
        || wire.uint(2)?.is_none_or(|base| base > sequence)
        || wire.uint(3)?.is_none_or(|last| last > sequence)
        || zero && (!super::modern_id(section.id()) || wire.uint(3)? != Some(sequence))
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
    let bytes = wire
        .bytes(5)?
        .filter(|bytes| bytes.len() <= 1024 * 1024)
        .ok_or(Error::Unsupported)?;
    let doc = Automerge::load(bytes).map_err(|_| Error::Unsupported)?;
    check_document(&doc, text, marks)?;
    let known = |head: &str| -> Result<(), Error> {
        let hash = head.parse().map_err(|_| Error::Unsupported)?;
        if doc.get_change_by_hash(&hash).is_none() {
            return Err(Error::Unsupported);
        }
        Ok(())
    };
    let processed = wire
        .fields()
        .iter()
        .filter(|field| field.tag() == 6)
        .map(|field| {
            let one = RawMessage::decode(wire.encoded_field(field)?)?;
            Ok(one.string(6)?.ok_or(Error::Unsupported)?.to_owned())
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if processed.is_empty() || processed.len() > 1024 {
        return Err(Error::Unsupported);
    }
    for head in &processed {
        known(head)?;
    }
    known(wire.string(8)?.ok_or(Error::Unsupported)?)?;
    if let Some(head) = wire.string(12)? {
        known(head)?;
    }
    for item in wire.messages(11)? {
        if item.fields().iter().any(|f| !matches!(f.tag(), 1 | 2))
            || item.uint(2)?.is_none_or(|n| n > sequence)
        {
            return Err(Error::Unsupported);
        }
        known(item.string(1)?.ok_or(Error::Unsupported)?)?;
    }
    if zero && !processed.contains(&doc.get_heads()[0].to_string()) {
        return Err(Error::Unsupported);
    }
    Ok(doc)
}
