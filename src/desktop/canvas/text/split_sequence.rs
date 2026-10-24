//! Bounded end sequences and interior splits with exact per-row input evidence.
use super::{Captured, EndWitness, EventWitness, SplitWitness, TextDraft, Value};
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Sequence {
    pub(super) revision: u64,
    pub(super) after: Vec<Value>,
    raw_after: Vec<Value>,
    html: String,
    events: Vec<Event>,
}
#[derive(Clone, Deserialize, Serialize)]
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
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Section {
    html: String,
    text: String,
    placeholder: Option<Empty>,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
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
    if event.sections.is_empty()
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
fn edit_proof(event: &Event, prior_text: &str) -> Option<()> {
    if event.selection_start > event.selection_end || event.start > event.end {
        return None;
    }
    offset(prior_text, event.selection_start)?;
    offset(prior_text, event.selection_end)?;
    let start = offset(prior_text, event.start)?;
    let end = offset(prior_text, event.end)?;
    let selected = event.selection_start != event.selection_end;
    if selected && (event.start != event.selection_start || event.end != event.selection_end) {
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
    if event.after_text != format!("{}{inserted}{}", &prior_text[..start], &prior_text[end..])
        || event.caret != event.start + inserted.encode_utf16().count()
    {
        return None;
    }
    Some(())
}
#[path = "split_sequence_second.rs"]
mod second;
pub(super) fn parse(w: &EventWitness, blocks: &[Value]) -> Option<Captured> {
    if w.kind.as_deref() == Some("second-middle-sequence") {
        return second::parse(w, blocks);
    }
    let value = w.sequence.as_ref()?;
    let middle = w.kind.as_deref() == Some("middle-sequence");
    if (!middle && w.kind.as_deref() != Some("sequence"))
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
    if original.is_empty() || !scalar(&original) {
        return None;
    }
    let mut updates: Vec<Vec<TextDraft>> = Vec::new();
    let mut parent_updates = Vec::new();
    let mut pre_updates = Vec::new();
    let mut enter_draft = None;
    let mut first_enter = None;
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
        // The first edit starts from the immutable mounted source; a direct
        // first Enter must preserve it. Later Enter preserves the prior rows.
        if previous.is_none() && event.kind == "enter" && drafts.first() != Some(&before) {
            return None;
        }
        let active = if event.kind == "middle" {
            0
        } else if event.kind == "enter" {
            updates.len()
        } else {
            event.active_index
        };
        let prior_text = if let Some(prior) = previous {
            prior.sections.get(active)?.text.as_str()
        } else {
            original.as_str()
        };
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
        if event.kind == "middle" {
            // Exactly one live interior split, anchored to original or prelude.
            if !middle
                || first_enter.is_some()
                || !updates.is_empty()
                || event.input_type != "insertParagraph"
                || event.data.is_some()
                || event.active_index != 1
                || start == 0
                || start == prior_text.len()
                || start != end
                || event.selection_start != event.start
                || event.selection_end != event.end
                || event.caret != 0
                || event.sections.len() != 2
                || event.sections.iter().any(|s| s.placeholder.is_some())
                || event.sections[0].text != prior_text[..start]
                || event.after_text != prior_text[start..]
            {
                return None;
            }
            enter_draft = if let Some(prior) = previous {
                if prior.sections.len() != 1 {
                    return None;
                }
                snapshot(prior)?.into_iter().next()
            } else {
                Some(before.clone())
            };
            first_enter = Some(event);
            updates.push(Vec::new());
        } else if event.kind == "enter" {
            if middle {
                return None;
            }
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
            if first_enter.is_none() {
                first_enter = Some(event);
                enter_draft = drafts.first().cloned();
            }
            updates.push(Vec::new());
        } else if event.kind == "edit" {
            if active > updates.len() || event.sections.len() != updates.len() + 1 {
                return None;
            }
            if let Some(prior) = previous {
                if event.sections.len() != prior.sections.len() {
                    return None;
                }
                let after = event.after[0]["elements"].as_array()?;
                let prior_after = prior.after[0]["elements"].as_array()?;
                let raw_after = event.raw_after[0]["elements"].as_array()?;
                let prior_raw = prior.raw_after[0]["elements"].as_array()?;
                for (index, section) in event.sections.iter().enumerate() {
                    if index != active
                        && (section != &prior.sections[index]
                            || after.get(index) != prior_after.get(index)
                            || raw_after.get(index) != prior_raw.get(index))
                    {
                        return None;
                    }
                }
            } else if active != 0 {
                return None;
            }
            edit_proof(event, prior_text)?;
            if event.after_text != prior_text {
                let draft = drafts.get(active)?.clone();
                bytes = bytes.checked_add(draft.rtml().ok()?.len())?;
                reports += 1;
                if bytes > 8 * 1024 * 1024 || reports > 4096 {
                    return None;
                }
                if active == 0 {
                    if first_enter.is_none() {
                        pre_updates.push(draft);
                    } else {
                        parent_updates.push(draft);
                    }
                } else {
                    updates.get_mut(active - 1)?.push(draft);
                }
            }
        } else {
            return None;
        }
        if event.sections.get(event.active_index)?.text != event.after_text {
            return None;
        }
        previous = Some(event);
    }
    let first = first_enter?;
    let last = previous?;
    if w.offset != first.start
        || w.revision != first.revision
        || w.after != first.after
        || w.raw_after.as_ref() != Some(&first.raw_after)
        || value.after != blocks
        || value.after != last.after
        || value.raw_after != last.raw_after
        || value.html != last.html
        || value.revision != last.revision
    {
        return None;
    }
    let enter_draft = enter_draft?;
    if middle {
        let initial = snapshot(first)?;
        let witness = if pre_updates.is_empty() {
            if enter_draft != before {
                return None;
            }
            SplitWitness::new(&before, w.offset, &initial[0], &initial[1])
        } else {
            SplitWitness::new_with_pre_enter_updates(
                &before,
                &enter_draft,
                w.offset,
                &initial[0],
                &initial[1],
                &pre_updates,
            )
        };
        return witness
            .ok()?
            .with_parent_updates(&parent_updates)
            .ok()?
            .with_child_updates(updates.first()?)
            .ok()
            .map(Captured::Split);
    }
    let witness = if pre_updates.is_empty() {
        // Actual no-op inputs still advance renderer revisions. They do not
        // introduce a new core parent history or change legacy custody.
        if enter_draft != before {
            return None;
        }
        EndWitness::sequence_with_parent_updates(&before, w.offset, &parent_updates, &updates)
    } else {
        EndWitness::sequence_with_pre_enter_updates(
            &before,
            &enter_draft,
            w.offset,
            &pre_updates,
            &parent_updates,
            &updates,
        )
    };
    witness.ok().map(Captured::End)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn browser_pre_enter_input_preserves_original_and_ordered_reports() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-preenter-native.json"))
                .unwrap();
        assert_eq!(cases.len(), 5);
        for case in cases {
            let capture = case["capture"].clone();
            let Some(Captured::End(witness)) =
                super::super::parse(serde_json::from_value(capture).unwrap(), 1, 0)
            else {
                panic!("pre-Enter capture refused: {}", case["name"]);
            };
            let expected_parent: Vec<String> =
                serde_json::from_value(case["expectedParentReports"].clone()).unwrap();
            assert_eq!(
                witness
                    .parent_updates()
                    .iter()
                    .map(text)
                    .collect::<Vec<_>>(),
                expected_parent,
                "{}",
                case["name"]
            );
            let expected_members: Vec<Vec<String>> =
                serde_json::from_value(case["expectedReports"].clone()).unwrap();
            assert!(witness.is_sequence());
            assert_eq!(witness.append_count(), expected_members.len());
            assert_eq!(
                witness
                    .sequence_updates()
                    .unwrap()
                    .iter()
                    .map(|reports| reports.iter().map(text).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                expected_members,
                "{}",
                case["name"]
            );
            if case["name"] == "actual-pre-enter-no-op" {
                assert!(witness.parent_updates().is_empty());
                assert_eq!(case["capture"]["split"]["revision"], 2);
            }
        }
    }

    #[test]
    fn pre_enter_parser_refuses_forged_boundary_and_chronology() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-preenter-native.json"))
                .unwrap();
        for case in cases {
            for defect in 0..7 {
                let mut capture = case["capture"].clone();
                match defect {
                    0 => capture["split"]["revision"] = json!(1),
                    1 => {
                        let offset = capture["split"]["offset"].as_u64().unwrap();
                        capture["split"]["offset"] = json!(offset + 1);
                    }
                    2 => {
                        capture["split"]["sequence"]["events"][0]["beforeText"] =
                            json!("Unreported original");
                    }
                    3 => {
                        capture["split"]["sequence"]["events"][0]["beforeInputTrusted"] =
                            json!(false);
                    }
                    4 => {
                        capture["split"]["sequence"]["events"][1]["beforeRevision"] = json!(9);
                    }
                    5 => {
                        let events = capture["split"]["sequence"]["events"]
                            .as_array_mut()
                            .unwrap();
                        let first = events.iter_mut().find(|e| e["kind"] == "enter").unwrap();
                        first["beforeText"] = json!("Unreported live boundary");
                    }
                    _ => {
                        capture["split"]["sequence"]["events"]
                            .as_array_mut()
                            .unwrap()
                            .retain(|e| e["kind"] != "enter");
                    }
                }
                assert!(
                    super::super::parse(serde_json::from_value(capture).unwrap(), 1, 0).is_none(),
                    "{} defect {defect}",
                    case["name"]
                );
            }
        }
    }

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

    #[test]
    fn browser_revisits_preserve_each_members_ordered_reports() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-revisit-native.json"))
                .unwrap();
        assert_eq!(cases.len(), 4);
        for case in cases {
            let Some(Captured::End(witness)) = super::super::parse(
                serde_json::from_value(case["capture"].clone()).unwrap(),
                1,
                0,
            ) else {
                panic!("revisit capture refused: {}", case["name"])
            };
            let actual: Vec<Vec<String>> = witness
                .sequence_updates()
                .unwrap()
                .iter()
                .map(|reports| reports.iter().map(text).collect())
                .collect();
            let expected: Vec<Vec<String>> =
                serde_json::from_value(case["expectedReports"].clone()).unwrap();
            assert_eq!(actual, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn browser_parent_revisits_preserve_parent_and_member_reports() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-parent-native.json"))
                .unwrap();
        assert_eq!(cases.len(), 4);
        for case in cases {
            let Some(Captured::End(witness)) = super::super::parse(
                serde_json::from_value(case["capture"].clone()).unwrap(),
                1,
                0,
            ) else {
                panic!("parent revisit refused: {}", case["name"])
            };
            let parent: Vec<String> = witness.parent_updates().iter().map(text).collect();
            let expected_parent: Vec<String> =
                serde_json::from_value(case["expectedParentReports"].clone()).unwrap();
            assert_eq!(parent, expected_parent, "{}", case["name"]);
            let members: Vec<Vec<String>> = witness
                .sequence_updates()
                .unwrap()
                .iter()
                .map(|reports| reports.iter().map(text).collect())
                .collect();
            let expected_members: Vec<Vec<String>> =
                serde_json::from_value(case["expectedReports"].clone()).unwrap();
            assert_eq!(members, expected_members, "{}", case["name"]);
        }
    }

    #[test]
    fn parent_revisit_refuses_unreported_rows_and_initial_seed_changes() {
        fn change_row(event: &mut Value, index: usize) {
            event["sections"][index] = json!({
                "html": "Unreported", "text": "Unreported", "placeholder": null
            });
            let changed = json!({
                "type": "rich_text_section", "elements": [{"type":"text","text":"Unreported"}]
            });
            event["after"][0]["elements"][index] = changed.clone();
            event["rawAfter"][0]["elements"][index] = changed;
            let mut html = String::from("<div class=\"rich-editor-block\">");
            for section in event["sections"].as_array().unwrap() {
                html.push_str("<div class=\"rich-editor-section\">");
                html.push_str(section["html"].as_str().unwrap());
                html.push_str("</div>");
            }
            html.push_str("</div>");
            event["html"] = json!(html);
        }
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-parent-native.json"))
                .unwrap();
        for case in cases {
            for victim in [0, 1] {
                let mut capture = case["capture"].clone();
                let sequence = &mut capture["split"]["sequence"];
                let event = sequence["events"]
                    .as_array_mut()
                    .unwrap()
                    .last_mut()
                    .unwrap();
                assert_eq!(event["kind"], "edit");
                assert!(event["activeIndex"].as_u64().unwrap() > 1);
                change_row(event, victim);
                let final_event = event.clone();
                for key in ["after", "rawAfter", "html"] {
                    sequence[key] = final_event[key].clone();
                }
                capture["blocks"] = final_event["after"].clone();
                assert!(
                    super::super::parse(serde_json::from_value(capture).unwrap(), 1, 0).is_none()
                );
            }
            // A later parent report does not authorize changing the seed during
            // the initial Enter, even when all first-event representations agree.
            let mut capture = case["capture"].clone();
            let event = &mut capture["split"]["sequence"]["events"][0];
            change_row(event, 0);
            let first_event = event.clone();
            capture["split"]["after"] = first_event["after"].clone();
            capture["split"]["rawAfter"] = first_event["rawAfter"].clone();
            assert!(super::super::parse(serde_json::from_value(capture).unwrap(), 1, 0).is_none());
        }
    }

    #[test]
    fn revisit_refuses_a_consistent_but_unreported_change_to_another_member() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/sequence-revisit-native.json"))
                .unwrap();
        for case in cases {
            let mut capture = case["capture"].clone();
            let sequence = &mut capture["split"]["sequence"];
            let event = sequence["events"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap();
            assert_eq!(event["kind"], "edit");
            assert_eq!(event["activeIndex"], 4);
            // Keep all final representations consistent: only the missing input
            // history for member 1 distinguishes this forged final snapshot.
            event["sections"][1] = json!({
                "html": "Unobserved", "text": "Unobserved", "placeholder": null
            });
            let changed = json!({
                "type": "rich_text_section", "elements": [{"type":"text","text":"Unobserved"}]
            });
            event["after"][0]["elements"][1] = changed.clone();
            event["rawAfter"][0]["elements"][1] = changed;
            let mut html = String::from("<div class=\"rich-editor-block\">");
            for section in event["sections"].as_array().unwrap() {
                html.push_str("<div class=\"rich-editor-section\">");
                html.push_str(section["html"].as_str().unwrap());
                html.push_str("</div>");
            }
            html.push_str("</div>");
            event["html"] = json!(html);
            let final_event = event.clone();
            for key in ["after", "rawAfter", "html"] {
                sequence[key] = final_event[key].clone();
            }
            capture["blocks"] = final_event["after"].clone();
            assert!(
                super::super::parse(serde_json::from_value(capture).unwrap(), 1, 0).is_none(),
                "{}",
                case["name"]
            );
        }
    }
}

#[cfg(test)]
#[path = "split_sequence_middle_tests.rs"]
mod middle_tests;
