//! Exact visible native UTF-16 boundaries mapped into Quip's escaped text.
//! No offset is inferred from Markdown, exported HTML or displayed labels.
use super::{Error, TextDraft, TextRun};
use automerge::marks::Mark;

pub(super) struct RangeModel {
    pub text: String,
    pub marks: Vec<Mark>,
    pub rtml: String,
    pub selected: TextDraft,
    pub escaped_start: usize,
    pub escaped_end: usize,
}

pub(super) fn capture(
    draft: &TextDraft,
    start: usize,
    end: usize,
    control: &str,
) -> Result<RangeModel, Error> {
    if start >= end || !super::modern_id(control) {
        return Err(Error::Unsupported);
    }
    let mut model = draft.model()?;
    let mut visible = 0usize;
    let mut escaped = 0usize;
    let mut from = (start == 0).then_some(0);
    let mut to = None;
    let mut selected = TextDraft::default();
    for run in &draft.runs {
        let mut text = String::new();
        for ch in run.text.chars() {
            let next = visible + ch.len_utf16();
            // A native range is UTF-16, but cannot split a scalar in the typed
            // paragraph. In particular no half-surrogate is silently rounded.
            if (visible < start && start < next) || (visible < end && end < next) {
                return Err(Error::Unsupported);
            }
            if start <= visible && next <= end {
                text.push(ch);
            }
            escaped += match ch {
                '&' => 5,
                '<' | '>' => 4,
                '\n' => 4,
                _ => ch.len_utf16(),
            };
            visible = next;
            if visible == start {
                from = Some(escaped);
            }
            if visible == end {
                to = Some(escaped);
            }
        }
        if !text.is_empty() {
            selected.runs.push(TextRun {
                text,
                style: run.style.clone(),
            });
        }
    }
    let from = from.ok_or(Error::Unsupported)?;
    let to = to.ok_or(Error::Unsupported)?;
    if end > visible || from >= to || escaped != model.text.encode_utf16().count() {
        return Err(Error::Unsupported);
    }
    selected.validate()?;
    model
        .marks
        .push(Mark::new("annotation".into(), control.to_owned(), from, to));
    let rtml = super::compact::render(&model.text, &model.marks)?;
    Ok(RangeModel {
        text: model.text,
        marks: model.marks,
        rtml,
        selected,
        escaped_start: from,
        escaped_end: to,
    })
}

/// Adding a disjoint mark retains every original mark; no annotation unwrap or
/// hidden-control removal is inferred from the visible text projection.
pub(super) fn capture_existing(
    original: &super::Existing,
    start: usize,
    end: usize,
    control: &str,
) -> Result<RangeModel, Error> {
    original.disjoint(start, end)?;
    let mut desired = capture(&original.draft, start, end, control)?;
    let added = desired.marks.pop().ok_or(Error::Unsupported)?;
    desired.marks = original.marks.clone();
    desired.marks.push(added);
    desired.rtml = super::compact::render(&desired.text, &desired.marks)?;
    Ok(desired)
}

/// Unwrap only the exact archived identities captured by the native branch.
/// Text and styling remain unchanged; all surviving annotations stay disjoint.
pub(super) fn capture_removing(
    original: &super::Existing,
    start: usize,
    end: usize,
    control: &str,
    removal: Option<&super::Removal>,
) -> Result<RangeModel, Error> {
    let Some(removal) = removal else {
        return capture_existing(original, start, end, control);
    };
    if original
        .spans
        .iter()
        .any(|s| !removal.contains(&s.id) && start < s.end && s.start < end)
    {
        return Err(Error::Unsupported);
    }
    let mut desired = capture(&original.draft, start, end, control)?;
    let added = desired.marks.pop().ok_or(Error::Unsupported)?;
    desired.marks = original
        .marks
        .iter()
        .filter(|mark| {
            !(mark.name() == "annotation"
                && mark.value().to_str().is_some_and(|id| removal.contains(id)))
        })
        .cloned()
        .collect();
    desired.marks.push(added);
    desired.rtml = super::compact::render(&desired.text, &desired.marks)?;
    Ok(desired)
}
