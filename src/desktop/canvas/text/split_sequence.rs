//! Ordered, bounded end-Enter and active-last plain input evidence.
use super::{Captured, EndWitness, EventWitness, TextDraft, Value};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Sequence {
    pub(super) revision: u64,
    pub(super) after: Vec<Value>,
    raw_after: Vec<Value>,
    html: String,
    events: Vec<Event>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    kind: String,
    input_type: String,
    data: Option<String>,
    active_index: usize,
    before_revision: u64,
    revision: u64,
    before_input_trusted: bool,
    input_trusted: bool,
    selection_start: usize,
    selection_end: usize,
    start: usize,
    end: usize,
    caret: usize,
    before_text: String,
    after_text: String,
    after: Vec<Value>,
    raw_after: Vec<Value>,
    html: String,
    sections: Vec<Section>,
}
#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Section {
    html: String,
    text: String,
    placeholder: Option<Empty>,
}
#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Empty {
    html: String,
    span_depth: usize,
}
fn offset(text: &str, wanted: usize) -> Option<usize> {
    let mut units = 0;
    for (index, c) in text.char_indices() {
        if units == wanted {
            return Some(index);
        }
        units += c.len_utf16();
    }
    (units == wanted).then_some(text.len())
}
fn scalar(text: &str) -> bool {
    text.chars()
        .all(|c| !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}'))
}
fn text(draft: &TextDraft) -> String {
    draft.runs.iter().map(|run| run.text.as_str()).collect()
}
fn plain_html(html: &str, expected: &str) -> bool {
    let mut rest = html;
    let mut depth = 0usize;
    let mut content = String::new();
    while !rest.is_empty() {
        if let Some(next) = rest.strip_prefix("<span>") {
            depth += 1;
            if depth > 16 {
                return false;
            }
            rest = next;
        } else if let Some(next) = rest.strip_prefix("</span>") {
            if depth == 0 {
                return false;
            }
            depth -= 1;
            rest = next;
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            if end == 0 {
                return false;
            }
            content.push_str(&rest[..end]);
            rest = &rest[end..];
        }
    }
    depth == 0
        && TextDraft::from_rtml(&content).ok().is_some_and(|d| {
            text(&d) == expected && d.runs.iter().all(|r| r.style == Default::default())
        })
}
fn snapshot(event: &Event) -> Option<Vec<TextDraft>> {
    if event.sections.len() < 2
        || event.sections.len() > 4100
        || event.raw_after.len() != 1
        || event.after.len() != 1
    {
        return None;
    }
    let raw = event.raw_after[0].get("elements")?.as_array()?;
    if raw.len() != event.sections.len() || event.raw_after[0]["type"] != "rich_text" {
        return None;
    }
    let mut projected = event.raw_after.clone();
    let mut html = String::from("<div class=\"rich-editor-block\">");
    let mut drafts = Vec::new();
    for (index, section) in event.sections.iter().enumerate() {
        if !scalar(&section.text) || raw[index]["type"] != "rich_text_section" {
            return None;
        }
        if let Some(empty) = &section.placeholder {
            let expected = format!(
                "{}<br>{}",
                "<span>".repeat(empty.span_depth.min(17)),
                "</span>".repeat(empty.span_depth.min(17))
            );
            if !section.text.is_empty()
                || empty.span_depth > 16
                || empty.html != expected
                || section.html != expected
                || raw[index]["elements"]
                    != if empty.span_depth == 0 {
                        serde_json::json!([])
                    } else {
                        serde_json::json!([{"type":"text","text":"\n"}])
                    }
            {
                return None;
            }
            projected[0]["elements"][index]["elements"] = serde_json::json!([]);
        } else if section.text.is_empty() || !plain_html(&section.html, &section.text) {
            return None;
        }
        html.push_str("<div class=\"rich-editor-section\">");
        html.push_str(&section.html);
        html.push_str("</div>");
        let draft=TextDraft::from_rich_blocks(&[serde_json::json!({"type":"rich_text","elements":[projected[0]["elements"][index].clone()]})]).ok()?;
        if text(&draft) != section.text
            || draft.runs.iter().any(|r| r.style != Default::default())
            || draft.rtml().ok()?.len() > 65536
        {
            return None;
        }
        drafts.push(draft);
    }
    html.push_str("</div>");
    (html == event.html && projected == event.after).then_some(drafts)
}
pub(super) fn parse(w: &EventWitness, blocks: &[Value]) -> Option<Captured> {
    let value = w.sequence.as_ref()?;
    if w.kind.as_deref() != Some("sequence")
        || w.repeat.is_some()
        || w.typing.is_some()
        || w.empty_proof.is_some()
        || w.placeholder.is_some()
        || w.leading_placeholder.is_some()
        || w.caret_child_zero.is_some()
        || w.caret_original_zero.is_some()
        || value.events.is_empty()
        || value.events.len() > 4099
        || serde_json::to_vec(value).ok()?.len() > 64 * 1024 * 1024
    {
        return None;
    }
    let before = TextDraft::from_rich_blocks(&w.before).ok()?;
    let original = text(&before);
    if original.is_empty() || !scalar(&original) || w.offset != original.encode_utf16().count() {
        return None;
    }
    let mut updates: Vec<Vec<TextDraft>> = Vec::new();
    let mut previous: Option<&Event> = None;
    let mut bytes = 0usize;
    let mut reports = 0usize;
    for (index, event) in value.events.iter().enumerate() {
        if !event.before_input_trusted
            || !event.input_trusted
            || event.before_revision != index as u64
            || event.revision != index as u64 + 1
        {
            return None;
        }
        let drafts = snapshot(event)?;
        if drafts.first() != Some(&before) {
            return None;
        }
        let active = updates.len();
        let prior_text = previous.map_or(original.as_str(), |e| {
            e.sections.last().unwrap().text.as_str()
        });
        if event.before_text != prior_text
            || event.selection_start > event.selection_end
            || event.start > event.end
        {
            return None;
        }
        offset(prior_text, event.selection_start)?;
        offset(prior_text, event.selection_end)?;
        let start = offset(prior_text, event.start)?;
        let end = offset(prior_text, event.end)?;
        if event.kind == "enter" {
            let len = prior_text.encode_utf16().count();
            if event.input_type != "insertParagraph"
                || event.data.is_some()
                || event.active_index != active + 1
                || event.start != len
                || event.end != len
                || event.selection_start != len
                || event.selection_end != len
                || event.caret != 0
                || !event.after_text.is_empty()
                || event.sections.len() != active + 2
                || event.sections.last()?.placeholder.is_none()
            {
                return None;
            }
            if let Some(prior) = previous {
                if event.sections[..prior.sections.len()] != prior.sections[..]
                    || event.after[0]["elements"].as_array()?[..prior.sections.len()]
                        != prior.after[0]["elements"].as_array()?[..]
                    || event.raw_after[0]["elements"].as_array()?[..prior.sections.len()]
                        != prior.raw_after[0]["elements"].as_array()?[..]
                {
                    return None;
                }
            }
            updates.push(Vec::new());
        } else if event.kind == "edit" {
            let prior = previous?;
            if active == 0
                || event.active_index != active
                || event.sections.len() != active + 1
                || event.sections[..active] != prior.sections[..active]
                || event.after[0]["elements"].as_array()?[..active]
                    != prior.after[0]["elements"].as_array()?[..active]
                || event.raw_after[0]["elements"].as_array()?[..active]
                    != prior.raw_after[0]["elements"].as_array()?[..active]
            {
                return None;
            }
            let selected = event.selection_start != event.selection_end;
            if selected
                && (event.start != event.selection_start || event.end != event.selection_end)
            {
                return None;
            }
            let inserted = match event.input_type.as_str() {
                "insertText" => {
                    let s = event.data.as_deref()?;
                    if s.is_empty()
                        || !scalar(s)
                        || event.start != event.selection_start
                        || event.end != event.selection_end
                    {
                        return None;
                    }
                    s
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
            if event.after_text
                != format!("{}{inserted}{}", &prior_text[..start], &prior_text[end..])
                || event.caret != event.start + inserted.encode_utf16().count()
            {
                return None;
            }
            if event.after_text != prior_text {
                let draft = drafts.last()?.clone();
                bytes = bytes.checked_add(draft.rtml().ok()?.len())?;
                reports += 1;
                if bytes > 8 * 1024 * 1024 || reports > 4096 {
                    return None;
                }
                updates.last_mut()?.push(draft);
            }
        } else {
            return None;
        }
        if event.sections.last()?.text != event.after_text {
            return None;
        }
        previous = Some(event);
    }
    let first = value.events.first()?;
    let last = previous?;
    if w.after != first.after
        || w.raw_after.as_ref() != Some(&first.raw_after)
        || value.after != blocks
        || value.after != last.after
        || value.raw_after != last.raw_after
        || value.html != last.html
        || value.revision != last.revision
    {
        return None;
    }
    EndWitness::sequence(&before, w.offset, &updates)
        .ok()
        .map(Captured::End)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn actual_native_sequence_captures_bind_reports_and_refuse_drift() {
        let mut cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-native.json")).unwrap();
        assert_eq!(cases.len(), 80);
        let long: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-long-native.json")).unwrap();
        assert_eq!(long.len(), 18);
        cases.extend(long);
        for case in cases {
            let capture = case["capture"].clone();
            if let Some(bytes) = case["sequenceBytes"].as_u64() {
                let value: Sequence =
                    serde_json::from_value(capture["split"]["sequence"].clone()).unwrap();
                assert_eq!(serde_json::to_vec(&value).unwrap().len() as u64, bytes);
            }
            let Some(Captured::End(witness)) =
                super::super::parse(serde_json::from_value(capture.clone()).unwrap(), 1, 0)
            else {
                panic!("{}", case["name"])
            };
            assert!(witness.is_sequence());
            assert!((2..=16).contains(&witness.append_count()));
            for defect in 0..9 {
                let mut bad = capture.clone();
                let event = &mut bad["split"]["sequence"]["events"][0];
                match defect {
                    0 => event["beforeInputTrusted"] = json!(false),
                    1 => event["beforeRevision"] = json!(1),
                    2 => event["caret"] = json!(1),
                    3 => event["activeIndex"] = json!(0),
                    4 => event["sections"][1]["placeholder"]["html"] = json!("<br><br>"),
                    5 => event["sections"][0]["html"] = json!("<b>Keep this</b>"),
                    6 => bad["split"]["sequence"]["events"][1]["start"] = json!(999),
                    7 => bad["split"]["generation"] = json!("2"),
                    _ => {
                        bad["blocks"][0]["elements"][1]["elements"] =
                            json!([{"type":"text","text":"drift"}])
                    }
                }
                assert!(super::super::parse(serde_json::from_value(bad).unwrap(), 1, 0).is_none());
            }
            let mut missing = capture.clone();
            missing["split"] = Value::Null;
            assert!(super::super::parse(serde_json::from_value(missing).unwrap(), 1, 0).is_none());
        }
    }

    #[test]
    fn event_budget_refuses_even_when_final_snapshot_is_unchanged() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-native.json")).unwrap();
        let mut capture = cases[0]["capture"].clone();
        let events = capture["split"]["sequence"]["events"]
            .as_array_mut()
            .unwrap();
        let last = events.last().unwrap().clone();
        events.resize(4100, last);
        assert!(super::super::parse(serde_json::from_value(capture).unwrap(), 1, 0).is_none());
    }
}
