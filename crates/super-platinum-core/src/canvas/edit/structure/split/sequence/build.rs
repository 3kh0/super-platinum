use super::*;
use automerge::{Automerge, ReadDoc};
#[derive(Clone)]
pub(in crate::canvas::edit::structure::split) struct State {
    pub child: String,
    pub position: String,
    pub wire: RawMessage,
    pub lineage: RawMessage,
}
pub(in crate::canvas::edit::structure::split) fn second_position(
    original: &str,
    first: &str,
    next: Option<&str>,
    target: SplitSequenceTarget,
) -> Result<String, Error> {
    match target {
        SplitSequenceTarget::Original => {
            position::between(Some(original), Some(position::base(first)?))
        }
        SplitSequenceTarget::FirstChild => position::between(Some(position::base(first)?), next),
    }
}
pub(in crate::canvas::edit::structure::split) fn lineage(
    wire: &RawMessage,
    child: &str,
    left: &TextDraft,
    session: &str,
    index: u64,
) -> Result<RawMessage, Error> {
    let bytes = wire.bytes(5)?.ok_or(Error::Unsupported)?;
    let doc = Automerge::load(bytes).map_err(|_| Error::Unsupported)?;
    let heads = doc.get_heads();
    if heads.len() != 1 {
        return Err(Error::Unsupported);
    }
    Ok(RawMessage::from_fields(&[
        F::uint(1, index)?,
        F::bytes(2, format!("{session}00000000").as_bytes())?,
        F::bytes(3, child.as_bytes())?,
        F::uint(4, left.rtml()?.encode_utf16().count() as u64)?,
        F::bytes(
            5,
            serde_json::to_string(&heads.iter().map(ToString::to_string).collect::<Vec<_>>())
                .map_err(|_| Error::Unsupported)?
                .as_bytes(),
        )?,
        F::bytes(
            6,
            format!(
                "{}-{}",
                wire.uint(2)?.ok_or(Error::Unsupported)?,
                wire.string(8)?.ok_or(Error::Unsupported)?
            )
            .as_bytes(),
        )?,
        F::bytes(7, bytes)?,
    ])?)
}
pub(in crate::canvas::edit::structure::split) fn build(
    intent: Arc<SplitIntent>,
    witness: SplitWitness,
    now: u64,
) -> Result<PreparedSplitEdit, Error> {
    let second = witness.second.as_ref().ok_or(Error::Unsupported)?;
    let mut before = witness.clone();
    before.second = None;
    let prefix = PreparedSplitEdit::build(Arc::clone(&intent), before, now)?;
    let mut result = PreparedSplitEdit::build(Arc::clone(&intent), witness.flattened()?, now)?;
    let source_position = intent.original.raw().string(8)?.ok_or(Error::Unsupported)?;
    let next = intent
        .inventory
        .ordered
        .iter()
        .filter_map(|id| intent.inventory.sections.get(id))
        .filter_map(|s| s.raw().string(8).ok().flatten())
        .filter(|p| *p > source_position)
        .min();
    let position = second_position(source_position, &intent.position, next, second.target)?;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let child = format!("{}{}", &intent.child[..10], &suffix[..25]);
    if child == intent.child || intent.inventory.sections.contains_key(&child) {
        return Err(Error::Changed);
    }
    let (target_wire, index) = match second.target {
        SplitSequenceTarget::Original => (&prefix.parent_wire, 1),
        SplitSequenceTarget::FirstChild => (&prefix.child_wire, 0),
    };
    let lineage = lineage(target_wire, &child, &second.parent, &intent.session, index)?;
    let child_seed = crdt::insert(&child, &second.child, &intent.session, now)?;
    let wire = if second.updates[2].is_empty() {
        child_seed
    } else {
        super::super::super::end::typing::build_reports(
            &child_seed,
            false,
            &second.updates[2],
            &intent.session,
            now,
        )?
    };
    match second.target {
        SplitSequenceTarget::Original => {
            result.parent_wire = result.parent_wire.replace_fields(
                10,
                &[F::message(10, &result.lineage)?, F::message(10, &lineage)?],
            )?
        }
        SplitSequenceTarget::FirstChild => {
            result.child_wire = result
                .child_wire
                .replace_fields(10, &[F::message(10, &lineage)?])?
        }
    }
    let state = State {
        child,
        position,
        wire,
        lineage,
    };
    result.data = project(
        &intent.original,
        &intent.child,
        &intent.position,
        &intent.session,
        &witness,
        &result.parent_wire,
        &result.child_wire,
        &state,
        intent.title.rich_text(),
    )?;
    result.witness = witness;
    result.second = Some(state);
    Ok(result)
}
#[allow(clippy::too_many_arguments)]
pub(in crate::canvas::edit::structure::split) fn project(
    original: &Section,
    child: &str,
    position: &str,
    session: &str,
    witness: &SplitWitness,
    parent_wire: &RawMessage,
    child_wire: &RawMessage,
    second: &State,
    title: &RawMessage,
) -> Result<RawMessage, Error> {
    let first = prepared::project(
        original,
        child,
        position,
        session,
        witness,
        parent_wire,
        child_wire,
        title,
    )?;
    let mut tail = witness.clone();
    tail.second = None;
    tail.child = witness.member_drafts()[2].clone();
    tail.updates.clear();
    let projection = prepared::project(
        original,
        &second.child,
        &second.position,
        session,
        &tail,
        parent_wire,
        &second.wire,
        title,
    )?;
    let rows = first.messages(1)?;
    let last = projection.messages(1)?;
    Ok(RawMessage::from_fields(&[
        F::message(1, &rows[0])?,
        F::message(1, &rows[1])?,
        F::message(1, &last[1])?,
        F::message(14, title)?,
    ])?)
}
