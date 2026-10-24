//! Exactly two interior Enters. DOM order and creation identity are distinct.
use super::*;
use super_platinum_core::canvas::edit::structure::SplitSequenceTarget;

pub(super) fn parse(w: &EventWitness, blocks: &[Value]) -> Option<Captured> {
    let value = w.sequence.as_ref()?;
    if value.events.len() > 4099
        || serde_json::to_vec(value).ok()?.len() > 64 * 1024 * 1024
        || w.repeat.is_some()
        || w.typing.is_some()
        || w.empty_proof.is_some()
        || w.placeholder.is_some()
        || w.leading_placeholder.is_some()
        || w.caret_child_zero.is_some()
        || w.caret_original_zero.is_some()
    {
        return None;
    }
    let boundary = value
        .events
        .iter()
        .position(|e| e.kind == "second-middle")?;
    let prior = value.events.get(boundary.checked_sub(1)?)?;
    // Reuse the complete accepted first-split proof, including between-boundary
    // edits, the original source, and the first Enter's immutable header.
    let prefix = EventWitness {
        sequence: Some(Sequence {
            revision: prior.revision,
            after: prior.after.clone(),
            raw_after: prior.raw_after.clone(),
            html: prior.html.clone(),
            events: value.events[..boundary].to_vec(),
        }),
        repeat: None,
        empty_proof: None,
        typing: None,
        caret_original_zero: None,
        leading_placeholder: None,
        kind: Some("middle-sequence".into()),
        caret_child_zero: None,
        raw_after: w.raw_after.clone(),
        placeholder: None,
        generation: w.generation.clone(),
        visit: w.visit.clone(),
        offset: w.offset,
        before: w.before.clone(),
        after: w.after.clone(),
        revision: w.revision,
        before_input_trusted: w.before_input_trusted,
        input_trusted: w.input_trusted,
        input_type: w.input_type.clone(),
    };
    let Captured::Split(first) = super::parse(&prefix, &prior.after)? else {
        return None;
    };
    if !first.pre_enter_updates().is_empty() || prior.sections.len() != 2 {
        return None;
    }
    let mut reports = first.parent_updates().len() + first.child_updates().len();
    let mut bytes = 0usize;
    for draft in first.parent_updates().iter().chain(first.child_updates()) {
        bytes = bytes.checked_add(draft.rtml().ok()?.len())?;
    }
    let event = &value.events[boundary];
    let target = event.active_index.checked_sub(1)?;
    if target > 1
        || !trusted(event, boundary)
        || event.input_type != "insertParagraph"
        || event.data.is_some()
        || event.start != event.end
        || event.selection_start != event.start
        || event.selection_end != event.end
        || event.caret != 0
        || event.sections.len() != 3
        || event.before_text != prior.sections[target].text
    {
        return None;
    }
    let point = offset(&event.before_text, event.start)?;
    if point == 0
        || point == event.before_text.len()
        || event.sections[target].text != event.before_text[..point]
        || event.sections[target + 1].text != event.before_text[point..]
        || event.after_text != event.sections[target + 1].text
        || event.sections[target].placeholder.is_some()
        || event.sections[target + 1].placeholder.is_some()
    {
        return None;
    }
    let previous = snapshot(prior)?;
    let initial = snapshot(event)?;
    for old in 0..2 {
        let next = if old > target { old + 1 } else { old };
        if old != target && !same_row(prior, old, event, next) {
            return None;
        }
    }
    let target_id = if target == 0 {
        SplitSequenceTarget::Original
    } else {
        SplitSequenceTarget::FirstChild
    };
    let witness = first
        .with_second_split(
            target_id,
            &previous[target],
            event.start,
            &initial[target],
            &initial[target + 1],
        )
        .ok()?;
    let order = if target == 0 { [0, 2, 1] } else { [0, 1, 2] };
    let mut updates: [Vec<TextDraft>; 3] = Default::default();
    let mut prior = event;
    for (index, event) in value.events.iter().enumerate().skip(boundary + 1) {
        let active = event.active_index;
        if event.kind != "edit"
            || !trusted(event, index)
            || active >= 3
            || event.sections.len() != 3
            || event.before_text != prior.sections[active].text
            || event.after_text != event.sections[active].text
        {
            return None;
        }
        let drafts = snapshot(event)?;
        edit_proof(event, &prior.sections[active].text)?;
        for other in 0..3 {
            if other != active && !same_row(prior, other, event, other) {
                return None;
            }
        }
        if event.after_text != event.before_text {
            reports += 1;
            bytes = bytes.checked_add(drafts[active].rtml().ok()?.len())?;
            if reports > 4096 || bytes > 8 * 1024 * 1024 {
                return None;
            }
            updates[order[active]].push(drafts[active].clone());
        }
        prior = event;
    }
    if value.after != blocks
        || value.after != prior.after
        || value.raw_after != prior.raw_after
        || value.html != prior.html
        || value.revision != prior.revision
    {
        return None;
    }
    witness
        .with_final_updates(&updates[0], &updates[1], &updates[2])
        .ok()
        .map(Captured::Split)
}

fn trusted(event: &Event, index: usize) -> bool {
    event.before_input_trusted
        && event.input_trusted
        && event.before_revision == index as u64
        && event.revision == index as u64 + 1
}

fn same_row(before: &Event, old: usize, after: &Event, next: usize) -> bool {
    before.sections[old] == after.sections[next]
        && before.after[0]["elements"][old] == after.after[0]["elements"][next]
        && before.raw_after[0]["elements"][old] == after.raw_after[0]["elements"][next]
}

#[cfg(test)]
#[path = "split_sequence_second_tests.rs"]
mod tests;
