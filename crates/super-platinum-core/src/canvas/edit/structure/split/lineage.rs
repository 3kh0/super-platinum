//! Narrow admission for a saved ordinary split parent and its canonical child.
//! Historical lineage is proved, retained verbatim, and never treated as an ACK.
use super::*;
use automerge::{Automerge, ROOT, ReadDoc};

struct Evidence {
    child: String,
    actor: String,
    tail: TextDraft,
}
fn evidence(wire: &RawMessage, section: &Section, current: &TextDraft) -> Result<Evidence, Error> {
    let infos = wire.messages(10)?;
    let [info] = infos.as_slice() else {
        return Err(Error::Unsupported);
    };
    if info.fields().len() != 7
        || info.fields().iter().any(|f| !(1..=7).contains(&f.tag()))
        || info.uint(1)? != Some(0)
    {
        return Err(Error::Unsupported);
    }
    let actor = info.string(2)?.ok_or(Error::Unsupported)?;
    let child = info.string(3)?.ok_or(Error::Unsupported)?;
    if actor.len() != 20
        || !actor.bytes().all(|b| b.is_ascii_hexdigit())
        || !actor.ends_with("00000000")
        || !position::modern_id(child)
        || child == section.id()
        || info.string(6)?
            != Some(
                format!(
                    "{}-{}",
                    wire.uint(2)?.ok_or(Error::Unsupported)?,
                    wire.string(8)?.ok_or(Error::Unsupported)?
                )
                .as_str(),
            )
    {
        return Err(Error::Unsupported);
    }
    let bytes = info.bytes(7)?.ok_or(Error::Unsupported)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Unsupported);
    }
    let before = Automerge::load(bytes).map_err(|_| Error::Unsupported)?;
    let heads = before.get_heads();
    if heads.len() != 1
        || info.string(5)?
            != Some(
                serde_json::to_string(&heads.iter().map(ToString::to_string).collect::<Vec<_>>())
                    .map_err(|_| Error::Unsupported)?
                    .as_str(),
            )
    {
        return Err(Error::Unsupported);
    }
    let (_, object) = before
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let encoded = before.text(&object).map_err(|_| Error::Unsupported)?;
    let rtml = encoded.strip_suffix('\u{feff}').ok_or(Error::Unsupported)?;
    let source = TextDraft::from_rtml(rtml)?;
    plain(&source)?;
    text::crdt::check_document_state(&before, &source, false)?;
    if source.rtml()? != rtml {
        return Err(Error::Unsupported);
    }
    let offset = usize::try_from(info.uint(4)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    let boundary = utf16_boundary(rtml, offset).ok_or(Error::Unsupported)?;
    let (left_rtml, tail_rtml) = rtml.split_at(boundary);
    let left = TextDraft::from_rtml(left_rtml)?;
    let tail = TextDraft::from_rtml(tail_rtml)?;
    // A scalar boundary can still bisect an escaped entity. Refuse HTML repair
    // rather than treating its decoded text as the historical split boundary.
    if left.rtml()? != left_rtml || tail.rtml()? != tail_rtml {
        return Err(Error::Unsupported);
    }
    SplitWitness::new(&source, plain(&left)?.encode_utf16().count(), &left, &tail)?;
    let document = Automerge::load(wire.bytes(5)?.ok_or(Error::Unsupported)?)
        .map_err(|_| Error::Unsupported)?;
    text::crdt::check_document_state(&document, current, false)?;
    if before
        .get_changes(&[])
        .iter()
        .any(|c| document.get_change_by_hash(&c.hash()).is_none())
    {
        return Err(Error::Unsupported);
    }
    let candidates = document
        .get_changes(&heads)
        .into_iter()
        .filter(|c| c.deps() == heads.as_slice() && c.actor_id().to_hex_string() == actor)
        .collect::<Vec<_>>();
    let [split] = candidates.as_slice() else {
        return Err(Error::Unsupported);
    };
    let shortened = document
        .fork_at(&[split.hash()])
        .map_err(|_| Error::Unsupported)?;
    text::crdt::check_document_state(&shortened, &left, false)?;
    if !validation::processed(wire, &split.hash().to_string())? {
        return Err(Error::Unsupported);
    }
    Ok(Evidence {
        child: child.into(),
        actor: actor.into(),
        tail,
    })
}

pub(in crate::canvas::edit) fn parent_history(
    wire: &RawMessage,
    section: &Section,
    draft: &TextDraft,
) -> Result<(), Error> {
    evidence(wire, section, draft).map(|_| ())
}
pub(in crate::canvas::edit) fn same_lineage(
    actual: &RawMessage,
    expected: &RawMessage,
) -> Result<bool, Error> {
    let a = actual.messages(10)?;
    let e = expected.messages(10)?;
    if a.is_empty() && e.is_empty() {
        return Ok(true);
    }
    if a.len() != 1 || e.len() != 1 {
        return Ok(false);
    }
    if a[0].fields().len() != 7 || a[0].fields().iter().any(|f| !(1..=7).contains(&f.tag())) {
        return Ok(false);
    }
    for tag in [1, 4] {
        if a[0].uint(tag)? != e[0].uint(tag)? {
            return Ok(false);
        }
    }
    for tag in [2, 3, 5, 6] {
        if a[0].string(tag)? != e[0].string(tag)? {
            return Ok(false);
        }
    }
    Ok(a[0].bytes(7)? == e[0].bytes(7)?)
}
pub(in crate::canvas::edit) fn associated_child<'a>(
    sections: impl IntoIterator<Item = &'a Section>,
    parent: &Section,
    now: u64,
) -> Result<Option<Section>, Error> {
    let Some(wire) = parent.crdt()?.filter(|w| w.has(10)) else {
        return Ok(None);
    };
    let ev = evidence(&wire, parent, &text::canonical_draft(parent)?)?;
    let mut found = None;
    let parent_position = parent.raw().string(8)?.ok_or(Error::Unsupported)?;
    position::canonical(parent_position)?;
    let mut next: Option<(&str, &str)> = None;
    for s in sections {
        if s.id() == ev.child {
            if found.replace(s).is_some() {
                return Err(Error::Unsupported);
            }
        }
        let pos = s.raw().string(8)?.ok_or(Error::Unsupported)?;
        position::canonical(pos)?;
        if pos > parent_position && next.is_none_or(|(p, _)| pos < p) {
            next = Some((pos, s.id()));
        }
    }
    let child = found.ok_or(Error::Changed)?;
    if next.map(|(_, id)| id) != Some(ev.child.as_str())
        || child.thread_id()? != parent.thread_id()?
        || child.document_id()? != parent.document_id()?
        || child.section_type()?.unwrap_or(0) != 0
        || child.style()?.unwrap_or(0) != 0
        || child.deleted()? == Some(true)
        || child.read_only()? == Some(true)
        || child.parents()?.is_some_and(|p| !p.fields().is_empty())
    {
        return Err(Error::Changed);
    }
    let child_wire = child.crdt()?.ok_or(Error::Unsupported)?;
    if child_wire.uint(2)? != Some(0) || child_wire.has(10) {
        return Err(Error::Unsupported);
    }
    let child_draft = text::canonical_draft(child)?;
    let doc = text::crdt::validate(&child_wire, child, &child_draft, Some(now))?;
    let base: automerge::ChangeHash = child_wire
        .string(8)?
        .ok_or(Error::Unsupported)?
        .parse()
        .map_err(|_| Error::Unsupported)?;
    let initial = doc.fork_at(&[base]).map_err(|_| Error::Unsupported)?;
    let initial_changes = initial.get_changes(&[]);
    if initial_changes.len() != 1
        || initial_changes[0].actor_id().to_hex_string() != ev.actor
        || initial_changes[0].seq() != 1
        || !initial_changes[0].deps().is_empty()
    {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(&initial, &TextDraft::default(), false)?;
    let candidates = doc
        .get_changes(&[base])
        .into_iter()
        .filter(|c| c.deps() == [base] && c.actor_id().to_hex_string() == ev.actor)
        .collect::<Vec<_>>();
    let [tail] = candidates.as_slice() else {
        return Err(Error::Unsupported);
    };
    if tail.seq() != 2 {
        return Err(Error::Unsupported);
    }
    text::crdt::check_document_state(
        &doc.fork_at(&[tail.hash()])
            .map_err(|_| Error::Unsupported)?,
        &ev.tail,
        false,
    )?;
    if !validation::processed(&child_wire, &tail.hash().to_string())? {
        // A first save after child typing coalesces the queued snapshots: only
        // the final head is processed. The extracted tail is still the exact
        // causal prefix, not an independently acknowledged head. Later ordinary
        // saves may use another actor; prove their linear text-only ancestry.
        plain_descendants(&doc, tail.hash(), &child_draft)?;
    }
    Ok(Some(child.clone()))
}

fn plain_descendants(
    doc: &Automerge,
    tail: automerge::ChangeHash,
    draft: &TextDraft,
) -> Result<(), Error> {
    if !draft.runs.is_empty() {
        plain(draft)?;
    }
    let prefix = doc.fork_at(&[tail]).map_err(|_| Error::Unsupported)?;
    if prefix.get_changes(&[]).len() != 2 {
        return Err(Error::Unsupported);
    }
    let (_, object) = doc
        .get(ROOT, "text")
        .map_err(|_| Error::Unsupported)?
        .ok_or(Error::Unsupported)?;
    let mut sequence = std::collections::BTreeMap::new();
    for change in prefix.get_changes(&[]) {
        sequence.insert(change.actor_id().to_hex_string(), change.seq());
    }
    let mut previous = tail;
    let changes = doc.get_changes(&[tail]);
    if changes.is_empty() || changes.len() + 2 != doc.get_changes(&[]).len() {
        return Err(Error::Unsupported);
    }
    for change in changes {
        let actor = change.actor_id().to_hex_string();
        let expected = sequence.get(&actor).copied().unwrap_or(0) + 1;
        if actor.len() != 20
            || !actor.ends_with("00000000")
            || change.seq() != expected
            || change.deps() != [previous]
        {
            return Err(Error::Unsupported);
        }
        let patches = doc.diff(&[previous], &[change.hash()]);
        if patches.is_empty()
            || patches.iter().any(|p| {
                p.obj != object
                    || !matches!(
                        &p.action,
                        automerge::PatchAction::SpliceText { marks: None, .. }
                            | automerge::PatchAction::DeleteSeq { .. }
                    )
            })
        {
            return Err(Error::Unsupported);
        }
        sequence.insert(actor, expected);
        previous = change.hash();
    }
    if doc.get_heads() != [previous] {
        return Err(Error::Unsupported);
    }
    // canonical_zero_base_history already required this exact final head in
    // processed6, a positive canonical revision and a current lease.
    Ok(())
}
