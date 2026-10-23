//! Validate renderer event evidence separately from core CONTENT report history.
use super::{Placeholder, TextDraft, Value, projection};
use serde::Deserialize;

#[cfg(test)]
#[path = "split_typing_tests.rs"]
mod tests;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Typing {
    revision: u64,
    pub(super) after: Vec<Value>,
    raw_after: Vec<Value>,
    html: String,
    placeholder: Option<Placeholder>,
    events: Vec<Event>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    input_type: String,
    data: Option<String>,
    selection_start: usize,
    selection_end: usize,
    start: usize,
    end: usize,
    before_text: String,
    after_text: String,
    before_revision: u64,
    revision: u64,
    caret: usize,
    before_input_trusted: bool,
    input_trusted: bool,
    after: Vec<Value>,
    raw_after: Vec<Value>,
    html: String,
    placeholder: Option<Placeholder>,
}

fn byte_offset(text: &str, utf16: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units == utf16 {
            return Some(byte);
        }
        units += ch.len_utf16();
    }
    (units == utf16).then_some(text.len())
}

fn plain_child(blocks: &[Value], parent: &TextDraft) -> Option<TextDraft> {
    if blocks.len() != 1 || blocks[0].get("type")?.as_str()? != "rich_text" {
        return None;
    }
    let parts = blocks[0].get("elements")?.as_array()?;
    if parts.len() != 2 {
        return None;
    }
    let draft = |part: &Value| {
        TextDraft::from_rich_blocks(&[serde_json::json!({"type":"rich_text","elements":[part]})])
            .ok()
    };
    if draft(&parts[0])?.rtml().ok()? != parent.rtml().ok()? {
        return None;
    }
    let child = draft(&parts[1])?;
    if child.runs.iter().any(|run| run.style != Default::default()) {
        return None;
    }
    Some(child)
}

pub(super) fn validate(value: &Typing, parent: &TextDraft) -> Option<Vec<TextDraft>> {
    validate_from(value, parent, &TextDraft::from_rtml("").ok()?)
}

pub(super) fn validate_from(
    value: &Typing,
    parent: &TextDraft,
    initial_child: &TextDraft,
) -> Option<Vec<TextDraft>> {
    validate_inner(value, parent, initial_child, None, None)
}
pub(super) fn validate_start(
    value: &Typing,
    original: &TextDraft,
    leading: &super::start::LeadingPlaceholder,
) -> Option<Vec<TextDraft>> {
    validate_inner(value, &TextDraft::default(), original, Some(leading), None)
}
pub(super) fn validate_empty(
    value: &Typing,
    proof: &super::empty::Proof,
) -> Option<Vec<TextDraft>> {
    validate_inner(
        value,
        &TextDraft::default(),
        &TextDraft::default(),
        None,
        Some(proof),
    )
}
pub(super) fn validate_repeat(
    value: &Typing,
    parent: &TextDraft,
    repeat: &super::repeat::Repeat,
) -> Option<Vec<TextDraft>> {
    let mut normalized = value.clone();
    let reduce = |raw: &mut Vec<Value>,
                  after: &mut Vec<Value>,
                  html: &str,
                  placeholder: &mut Option<Placeholder>|
     -> Option<()> {
        let pair = super::repeat::typing_pair(raw, after, html, repeat)?;
        *raw = pair.0;
        *after = pair.1;
        if let Some(p) = placeholder {
            if p.section_count != 3 {
                return None;
            }
            let suffix = format!(
                "<div class=\"rich-editor-section\">{}</div></div>",
                p.child_html
            );
            if !html.ends_with(&suffix) {
                return None;
            }
            p.section_count = 2;
        }
        Some(())
    };
    normalized.revision = normalized.revision.checked_sub(1)?;
    reduce(
        &mut normalized.raw_after,
        &mut normalized.after,
        &normalized.html,
        &mut normalized.placeholder,
    )?;
    for event in &mut normalized.events {
        event.before_revision = event.before_revision.checked_sub(1)?;
        event.revision = event.revision.checked_sub(1)?;
        reduce(
            &mut event.raw_after,
            &mut event.after,
            &event.html,
            &mut event.placeholder,
        )?;
    }
    validate(&normalized, parent)
}
fn validate_inner(
    value: &Typing,
    parent: &TextDraft,
    initial_child: &TextDraft,
    leading: Option<&super::start::LeadingPlaceholder>,
    empty: Option<&super::empty::Proof>,
) -> Option<Vec<TextDraft>> {
    // Native no-op events remain attested but do not inflate logical history.
    if value.events.is_empty() || value.events.len() > 4096 {
        return None;
    }
    let mut previous = initial_child
        .runs
        .iter()
        .map(|run| run.text.as_str())
        .collect::<String>();
    let mut reports = Vec::new();
    let mut bytes = 0usize;
    for (index, event) in value.events.iter().enumerate() {
        if !event.before_input_trusted
            || !event.input_trusted
            || event.before_revision != index as u64 + 1
            || event.revision != index as u64 + 2
            || event.before_text != previous
            || event.start > event.end
            || event.selection_start > event.selection_end
        {
            return None;
        }
        let start = byte_offset(&previous, event.start)?;
        let end = byte_offset(&previous, event.end)?;
        byte_offset(&previous, event.selection_start)?;
        byte_offset(&previous, event.selection_end)?;
        let selected = event.selection_start != event.selection_end;
        if selected && (event.start != event.selection_start || event.end != event.selection_end) {
            return None;
        }
        let inserted = match event.input_type.as_str() {
            "insertText" => {
                let data = event.data.as_deref()?;
                if data.is_empty()
                    || event.start != event.selection_start
                    || event.end != event.selection_end
                {
                    return None;
                }
                data
            }
            "deleteContentBackward" | "deleteContentForward" => {
                if event.data.is_some()
                    || start == end
                    || (!selected
                        && if event.input_type == "deleteContentBackward" {
                            event.end != event.selection_start
                        } else {
                            event.start != event.selection_start
                        })
                {
                    return None;
                }
                ""
            }
            _ => return None,
        };
        if event.after_text != format!("{}{inserted}{}", &previous[..start], &previous[end..])
            || event.caret != event.start + inserted.encode_utf16().count()
            || !if let Some(proof) = empty {
                super::empty::typing_projection(&event.raw_after, proof, &event.html)
                    .is_some_and(|raw| projection(&raw, event.placeholder.as_ref(), &event.after))
            } else if let Some(proof) = leading {
                super::start::leading_projection(&event.raw_after, proof)
                    .is_some_and(|raw| projection(&raw, event.placeholder.as_ref(), &event.after))
            } else {
                projection(&event.raw_after, event.placeholder.as_ref(), &event.after)
            }
            || event.after_text.is_empty() != event.placeholder.is_some()
            || event.html.is_empty()
        {
            return None;
        }
        let child = plain_child(&event.after, parent)?;
        if child
            .runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<String>()
            != event.after_text
        {
            return None;
        }
        let rtml = child.rtml().ok()?;
        if rtml.len() > 65536 {
            return None;
        }
        if event.after_text != previous {
            bytes = bytes.checked_add(rtml.len())?;
            if bytes > 8 * 1024 * 1024 {
                return None;
            }
            reports.push(child);
        }
        previous.clone_from(&event.after_text);
    }
    let last = value.events.last()?;
    if value.revision != last.revision
        || value.after != last.after
        || value.raw_after != last.raw_after
        || value.html != last.html
        || value.placeholder != last.placeholder
    {
        return None;
    }
    Some(reports)
}
