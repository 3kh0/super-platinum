//! Two sealed native end Enter events, followed by bounded third-only typing.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Repeat {
    revision: u64,
    pub(super) after: Vec<Value>,
    raw_after: Vec<Value>,
    html: String,
    before_input_trusted: bool,
    input_trusted: bool,
    input_type: String,
    caret_section: usize,
    caret_offset: usize,
    placeholders: Vec<Empty>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Empty {
    html: String,
    span_depth: usize,
}
pub(super) fn parse(
    w: &EventWitness,
    before: &TextDraft,
    drafts: &[TextDraft],
) -> Option<EndWitness> {
    let repeat = w.repeat.as_ref()?;
    if w.kind.as_deref() != Some("end")
        || w.empty_proof.is_some()
        || w.leading_placeholder.is_some()
        || w.caret_original_zero.is_some()
        || w.caret_child_zero != Some(true)
        || !end_projection(w)
        || repeat.revision != 2
        || !repeat.before_input_trusted
        || !repeat.input_trusted
        || repeat.input_type != "insertParagraph"
        || repeat.caret_section != 2
        || repeat.caret_offset != 0
        || repeat.placeholders.len() != 2
        || repeat.raw_after.len() != 1
        || repeat.after.len() != 1
    {
        return None;
    }
    EndWitness::new(before, w.offset, &drafts[0], &drafts[1]).ok()?;
    let mut projected = repeat.raw_after.clone();
    let sections = projected[0].get_mut("elements")?.as_array_mut()?;
    if sections.len() != 3 || sections[0] != w.after[0]["elements"][0] {
        return None;
    }
    for (index, proof) in repeat.placeholders.iter().enumerate() {
        if proof.span_depth > 16
            || proof.html
                != format!(
                    "{}<br>{}",
                    "<span>".repeat(proof.span_depth),
                    "</span>".repeat(proof.span_depth)
                )
        {
            return None;
        }
        let section = &mut sections[index + 1];
        let expected = if proof.span_depth == 0 {
            serde_json::json!([])
        } else {
            serde_json::json!([{"type":"text","text":"\n"}])
        };
        if section["type"] != "rich_text_section" || section["elements"] != expected {
            return None;
        }
        section["elements"] = serde_json::json!([]);
    }
    if projected != repeat.after || repeat.after[0]["type"] != "rich_text" {
        return None;
    }
    // Check the literal placeholder branches and plain original independently;
    // HTML is evidence, never inserted into the renderer.
    let suffix = format!(
        "<div class=\"rich-editor-section\">{}</div><div class=\"rich-editor-section\">{}</div></div>",
        repeat.placeholders[0].html, repeat.placeholders[1].html
    );
    if !repeat
        .html
        .starts_with("<div class=\"rich-editor-block\"><div class=\"rich-editor-section\">")
        || !repeat.html.ends_with(&suffix)
    {
        return None;
    }
    let witness = EndWitness::repeat(
        before,
        w.offset,
        &drafts[0],
        &TextDraft::default(),
        &TextDraft::default(),
    )
    .ok()?;
    if let Some(value) = &w.typing {
        let reports = super::typing::validate_repeat(value, &drafts[0], repeat)?;
        if reports.is_empty() {
            Some(witness)
        } else {
            witness.with_second_child_updates(&reports).ok()
        }
    } else {
        Some(witness)
    }
}

/// Reduce only a proven three-section capture to the existing ordered plain
/// typing validator. The first placeholder and original HTML remain immutable.
pub(super) fn typing_pair(
    raw: &[Value],
    after: &[Value],
    html: &str,
    proof: &Repeat,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let fixed_suffix = format!(
        "<div class=\"rich-editor-section\">{}</div></div>",
        proof.placeholders.get(1)?.html
    );
    let fixed_prefix = proof.html.strip_suffix(&fixed_suffix)?;
    let typed_html = html
        .strip_prefix(fixed_prefix)?
        .strip_suffix("</div></div>")?;
    if !typed_html.starts_with("<div class=\"rich-editor-section\">") {
        return None;
    }
    let reduce = |blocks: &[Value], raw: bool| -> Option<Vec<Value>> {
        if blocks.len() != 1 || blocks[0]["type"] != "rich_text" {
            return None;
        }
        let rows = blocks[0]["elements"].as_array()?;
        let initial = if raw { &proof.raw_after } else { &proof.after };
        if rows.len() != 3
            || rows[0] != initial[0]["elements"][0]
            || rows[1] != initial[0]["elements"][1]
        {
            return None;
        }
        Some(vec![
            serde_json::json!({"type":"rich_text","elements":[rows[0],rows[2]]}),
        ])
    };
    Some((reduce(raw, true)?, reduce(after, false)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn capture(text: &str) -> Value {
        let parent = json!({"type":"rich_text_section","elements":[{"type":"text","text":text}]});
        let empty = json!({"type":"rich_text_section","elements":[]});
        let raw_empty =
            json!({"type":"rich_text_section","elements":[{"type":"text","text":"\n"}]});
        let blocks = |rows: Vec<Value>| json!([{"type":"rich_text","elements":rows}]);
        let after = blocks(vec![parent.clone(), empty.clone()]);
        let final_blocks = blocks(vec![parent.clone(), empty.clone(), empty]);
        let escaped = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        json!({"blocks":final_blocks,"split":{"kind":"end","generation":"11","visit":"4",
            "offset":text.encode_utf16().count(),"before":blocks(vec![parent.clone()]),"after":after,
            "rawAfter":blocks(vec![parent.clone(),raw_empty.clone()]),"revision":1,"inputType":"insertParagraph",
            "beforeInputTrusted":true,"inputTrusted":true,"caretChildZero":true,
            "placeholder":{"blockCount":1,"sectionCount":2,"childHtml":"<span><br></span>","spanDepth":1,"caretOffset":0},
            "repeat":{"revision":2,"after":final_blocks,"rawAfter":blocks(vec![parent,raw_empty.clone(),raw_empty]),
                "html":format!("<div class=\"rich-editor-block\"><div class=\"rich-editor-section\"><span>{escaped}</span></div><div class=\"rich-editor-section\"><span><br></span></div><div class=\"rich-editor-section\"><span><br></span></div></div>"),
                "beforeInputTrusted":true,"inputTrusted":true,"inputType":"insertParagraph","caretSection":2,"caretOffset":0,
                "placeholders":[{"html":"<span><br></span>","spanDepth":1},{"html":"<span><br></span>","spanDepth":1}]}}})
    }
    fn parsed(value: Value) -> Option<Captured> {
        super::super::parse(serde_json::from_value(value).unwrap(), 11, 4)
    }
    #[test]
    fn actual_repeat_typing_captures_keep_order_and_refuse_fixed_member_drift() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("split_fixture/repeat-typing-native.json")).unwrap();
        assert_eq!(cases.len(), 24);
        for case in cases {
            let value = case["capture"].clone();
            let parse_native =
                |value: Value| super::super::parse(serde_json::from_value(value).unwrap(), 1, 0);
            let Some(Captured::End(witness)) = parse_native(value.clone()) else {
                panic!("rejected {}", case["name"])
            };
            assert!(witness.is_repeat_typing());
            for mutation in 0..7 {
                let mut bad = value.clone();
                match mutation {
                    0 => bad["split"]["typing"]["events"][0]["beforeRevision"] = json!(1),
                    1 => bad["split"]["typing"]["events"][0]["inputTrusted"] = json!(false),
                    2 => {
                        bad["split"]["typing"]["events"][0]["after"][0]["elements"][1]["elements"] =
                            json!([{"type":"text","text":"x"}])
                    }
                    3 => {
                        bad["split"]["typing"]["events"][0]["rawAfter"][0]["elements"][0]["elements"] =
                            json!([])
                    }
                    4 => bad["split"]["typing"]["events"][0]["html"] = json!("<div></div>"),
                    5 => bad["split"]["typing"]["events"][0]["start"] = json!(1),
                    _ => bad["split"]["typing"] = Value::Null,
                }
                assert!(
                    parse_native(bad).is_none(),
                    "mutation {mutation}: {}",
                    case["name"]
                );
            }
        }
    }
    #[test]
    fn repeat_capture_retains_two_native_events_and_three_sections() {
        for text in ["Keep this", "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}"] {
            let Some(Captured::End(w)) = parsed(capture(text)) else {
                panic!("repeat rejected");
            };
            assert!(w.is_repeat());
        }
    }
    #[test]
    fn repeat_capture_refuses_unbound_third_section_and_extra_input() {
        for mutate in 0..7 {
            let mut value = capture("Keep this");
            match mutate {
                0 => value["split"]["repeat"]["revision"] = json!(3),
                1 => value["split"]["repeat"]["inputTrusted"] = json!(false),
                2 => value["split"]["repeat"]["caretSection"] = json!(1),
                3 => value["split"]["repeat"]["placeholders"][1]["html"] = json!("<br><br>"),
                4 => value["split"]["repeat"]["rawAfter"][0]["elements"][1]["elements"] = json!([]),
                5 => {
                    value["blocks"][0]["elements"][2]["elements"] =
                        json!([{"type":"text","text":"x"}])
                }
                _ => value["split"] = Value::Null,
            }
            assert!(parsed(value).is_none(), "admitted mutation {mutate}");
        }
    }
}
