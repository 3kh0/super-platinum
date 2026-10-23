//! Renderer-owned native split witness. Core validates the source/history pair.
use super::*;
use serde::Deserialize;
use serde_json::Value;
use super_platinum_core::canvas::edit::{
    structure::{EndWitness, SplitWitness},
    text::TextDraft,
};

const SEQUENCE_INPUT: &str = include_str!("split_sequence_input.js");
#[path = "split_sequence.rs"]
mod sequence;
const REPEAT_INPUT: &str = include_str!("split_repeat_input.js");
const INPUT: &str = include_str!("split_input.js");
#[path = "split_empty.rs"]
mod empty;
#[path = "split_repeat.rs"]
mod repeat;
#[path = "split_start.rs"]
mod start;
#[path = "split_typing.rs"]
mod typing;
pub(super) use start::Start;

pub(super) fn mount(generation: u64, visit: u64, blocks: &str) -> String {
    format!(
        "{SEQUENCE_INPUT};{REPEAT_INPUT};{INPUT};window.superPlatinumCanvasSplit.mount({generation},{visit},{blocks});"
    )
}

#[derive(Deserialize)]
pub(super) struct Capture {
    blocks: Vec<Value>,
    split: Option<EventWitness>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EventWitness {
    #[serde(default)]
    sequence: Option<sequence::Sequence>,
    #[serde(default)]
    repeat: Option<repeat::Repeat>,
    #[serde(default)]
    empty_proof: Option<empty::Proof>,
    #[serde(default)]
    typing: Option<typing::Typing>,
    #[serde(default)]
    caret_original_zero: Option<bool>,
    #[serde(default)]
    leading_placeholder: Option<start::LeadingPlaceholder>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    caret_child_zero: Option<bool>,
    #[serde(default)]
    raw_after: Option<Vec<Value>>,
    #[serde(default)]
    placeholder: Option<Placeholder>,
    generation: String,
    visit: String,
    offset: usize,
    before: Vec<Value>,
    after: Vec<Value>,
    revision: u64,
    before_input_trusted: bool,
    input_trusted: bool,
    input_type: String,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Placeholder {
    block_count: usize,
    section_count: usize,
    child_html: String,
    span_depth: usize,
    caret_offset: usize,
}

fn projection(raw: &[Value], proof: Option<&Placeholder>, after: &[Value]) -> bool {
    let Some(proof) = proof else {
        return raw == after;
    };
    if raw.is_empty() {
        return false;
    }
    if proof.block_count != 1
        || proof.section_count != 2
        || proof.span_depth > 16
        || proof.caret_offset != 0
        || proof.child_html
            != format!(
                "{}<br>{}",
                "<span>".repeat(proof.span_depth),
                "</span>".repeat(proof.span_depth)
            )
        || raw.len() != 1
    {
        return false;
    }
    let mut projected = raw.to_vec();
    let Some(sections) = projected[0]
        .get_mut("elements")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    if sections.len() != 2 {
        return false;
    }
    let Some(runs) = sections[1].get_mut("elements") else {
        return false;
    };
    if *runs != serde_json::json!([]) && *runs != serde_json::json!([{"type":"text","text":"\n"}]) {
        return false;
    }
    *runs = serde_json::json!([]);
    projected == after
}
fn end_projection(witness: &EventWitness) -> bool {
    let (Some(raw), Some(proof)) = (&witness.raw_after, &witness.placeholder) else {
        return false;
    };
    projection(raw, Some(proof), &witness.after)
}

pub(super) enum Captured {
    Text(TextDraft),
    Start(Start),
    Split(SplitWitness),
    End(EndWitness),
}

pub(super) fn parse(capture: Capture, generation: u64, visit: u64) -> Option<Captured> {
    let Some(witness) = capture.split else {
        return TextDraft::from_rich_blocks(&capture.blocks)
            .ok()
            .map(Captured::Text);
    };
    if witness.sequence.is_some() {
        if witness.generation != generation.to_string()
            || witness.visit != visit.to_string()
            || witness.revision != 1
            || !witness.before_input_trusted
            || !witness.input_trusted
            || witness.input_type != "insertParagraph"
        {
            return None;
        }
        return sequence::parse(&witness, &capture.blocks);
    }
    if witness.generation != generation.to_string()
        || witness.visit != visit.to_string()
        || witness.revision != 1
        || !witness.before_input_trusted
        || !witness.input_trusted
        || witness.input_type != "insertParagraph"
        || witness
            .typing
            .as_ref()
            .map(|t| &t.after)
            .unwrap_or_else(|| witness.repeat.as_ref().map_or(&witness.after, |r| &r.after))
            != &capture.blocks
        || witness.after.len() != 1
    {
        return None;
    }
    let before = TextDraft::from_rich_blocks(&witness.before).ok()?;
    let root = &witness.after[0];
    if root.get("type")?.as_str()? != "rich_text" {
        return None;
    }
    let sections = root.get("elements")?.as_array()?;
    if sections.len() != 2 {
        return None;
    }
    let mut drafts = Vec::new();
    for section in sections {
        let value = serde_json::json!({"type":"rich_text","elements":[section]});
        drafts.push(TextDraft::from_rich_blocks(&[value]).ok()?);
    }
    if witness.repeat.is_some() {
        return repeat::parse(&witness, &before, &drafts).map(Captured::End);
    }
    if witness.kind.as_deref() == Some("empty") {
        return empty::parse(&witness, &before, &drafts).map(Captured::End);
    }
    if witness.empty_proof.is_some() {
        return None;
    }
    if witness.kind.as_deref() == Some("start") {
        return start::parse(&witness, &before, &drafts).map(Captured::Start);
    }
    if witness.caret_original_zero.is_some() || witness.leading_placeholder.is_some() {
        return None;
    }
    if witness.kind.as_deref() == Some("end")
        && witness.caret_child_zero == Some(true)
        && end_projection(&witness)
    {
        let initial = EndWitness::new(&before, witness.offset, &drafts[0], &drafts[1]).ok()?;
        let result = if let Some(typing) = &witness.typing {
            let updates = typing::validate(typing, &drafts[0])?;
            initial.with_child_updates(&updates).ok()?
        } else {
            initial
        };
        return Some(Captured::End(result));
    }
    if witness.kind.as_deref() == Some("middle") {
        if witness.caret_child_zero != Some(true)
            || witness.placeholder.is_some()
            || witness.raw_after.as_ref() != Some(&witness.after)
        {
            return None;
        }
        let initial = SplitWitness::new(&before, witness.offset, &drafts[0], &drafts[1]).ok()?;
        let updates = typing::validate_from(witness.typing.as_ref()?, &drafts[0], &drafts[1])?;
        return initial
            .with_child_updates(&updates)
            .ok()
            .map(Captured::Split);
    }
    if witness.kind.is_some()
        || witness.caret_child_zero.is_some()
        || witness.raw_after.is_some()
        || witness.placeholder.is_some()
        || witness.typing.is_some()
    {
        return None;
    }
    SplitWitness::new(&before, witness.offset, &drafts[0], &drafts[1])
        .ok()
        .map(Captured::Split)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blocks(parts: &[&str]) -> Vec<Value> {
        vec![json!({"type":"rich_text","elements":parts.iter().map(|text|
            json!({"type":"rich_text_section","elements":[{"type":"text","text":text}]})).collect::<Vec<_>>()})]
    }
    fn wire() -> Value {
        let after = blocks(&["Keep this", "Second paragraph"]);
        json!({"blocks":after,"split":{"generation":"11","visit":"4","offset":9,
            "before":blocks(&["Keep thisSecond paragraph"]),"after":after,"revision":1,
            "beforeInputTrusted":true,"inputTrusted":true,"inputType":"insertParagraph"}})
    }
    #[test]
    fn native_split_capture_binds_owner_and_exact_pair() {
        assert!(matches!(
            parse(serde_json::from_value(wire()).unwrap(), 11, 4),
            Some(Captured::Split(_))
        ));
        assert!(parse(serde_json::from_value(wire()).unwrap(), 12, 4).is_none());
        assert!(parse(serde_json::from_value(wire()).unwrap(), 11, 5).is_none());
        let mut stale = wire();
        stale["blocks"] = json!(blocks(&["Keep this", "Other tail"]));
        assert!(parse(serde_json::from_value(stale).unwrap(), 11, 4).is_none());
    }
    #[test]
    fn native_split_capture_refuses_missing_event_or_later_revision() {
        for (field, value) in [
            ("beforeInputTrusted", json!(false)),
            ("inputTrusted", json!(false)),
            ("revision", json!(2)),
            ("inputType", json!("insertText")),
        ] {
            let mut changed = wire();
            changed["split"][field] = value;
            assert!(parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none());
        }
        let mut unwitnessed = wire();
        unwitnessed["split"] = Value::Null;
        assert!(parse(serde_json::from_value(unwitnessed).unwrap(), 11, 4).is_none());
    }
    #[test]
    fn ordinary_single_paragraph_capture_still_uses_text_path() {
        let capture =
            serde_json::from_value(json!({"blocks":blocks(&["Ordinary edit"]),"split":null}))
                .unwrap();
        assert!(matches!(parse(capture, 11, 4), Some(Captured::Text(_))));
    }
    fn unicode_wire() -> Value {
        let mut value = wire();
        let after = blocks(&["Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}", "Second paragraph"]);
        value["blocks"] = json!(after);
        value["split"]["after"] = json!(after);
        value["split"]["before"] = json!(blocks(&[
            "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph"
        ]));
        value["split"]["offset"] = json!(11);
        value
    }
    #[test]
    fn unicode_capture_uses_visible_utf16_and_preserves_owner_and_exact_halves() {
        assert!(matches!(
            parse(serde_json::from_value(unicode_wire()).unwrap(), 11, 4),
            Some(Captured::Split(_))
        ));
        assert!(parse(serde_json::from_value(unicode_wire()).unwrap(), 12, 4).is_none());
        for offset in [0, 9, 10, 15, 18, 27, usize::MAX] {
            let mut changed = unicode_wire();
            changed["split"]["offset"] = json!(offset);
            assert!(
                parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none(),
                "offset {offset}"
            );
        }
    }
    #[test]
    fn unicode_capture_never_normalizes_canonically_equivalent_halves() {
        let mut changed = unicode_wire();
        let after = blocks(&[
            "Cafe\u{301} & \u{65e5}\u{672c}\u{1f600}",
            "Second paragraph",
        ]);
        changed["blocks"] = json!(after);
        changed["split"]["after"] = json!(after);
        assert!(parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none());
    }
    #[test]
    fn unicode_capture_refuses_controls_and_terminal_sentinel() {
        for excluded in [
            '\u{0}', '\u{1f}', '\u{7f}', '\u{85}', '\u{9f}', '\u{2028}', '\u{2029}', '\u{feff}',
        ] {
            let left = format!("A{excluded}");
            let source = format!("{left}Tail");
            let after = blocks(&[&left, "Tail"]);
            let mut changed = wire();
            changed["blocks"] = json!(after);
            changed["split"]["after"] = json!(after);
            changed["split"]["before"] = json!(blocks(&[&source]));
            changed["split"]["offset"] = json!(2);
            assert!(
                parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none(),
                "excluded {excluded:?}"
            );
        }
    }
    #[test]
    fn malformed_utf16_json_cannot_deserialize_as_a_capture() {
        let raw = r#"{"blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"\ud800"}]}]}],"split":null}"#;
        assert!(serde_json::from_str::<Capture>(raw).is_err());
    }
    fn end_wire(unicode: bool) -> Value {
        let source = if unicode {
            "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}"
        } else {
            "Keep this"
        };
        let mut after = blocks(&[source, ""]);
        after[0]["elements"][1]["elements"] = json!([]);
        json!({"blocks":after,"split":{"kind":"end","caretChildZero":true,
            "generation":"11","visit":"4","offset":source.encode_utf16().count(),
            "before":blocks(&[source]),"after":after,"rawAfter":blocks(&[source,"\n"]),
            "placeholder":{"blockCount":1,"sectionCount":2,"childHtml":"<span><br></span>","spanDepth":1,"caretOffset":0},
            "revision":1,"beforeInputTrusted":true,"inputTrusted":true,"inputType":"insertParagraph"}})
    }
    #[test]
    fn end_capture_binds_exact_native_placeholder_and_canonical_projection() {
        for unicode in [false, true] {
            assert!(matches!(
                parse(serde_json::from_value(end_wire(unicode)).unwrap(), 11, 4),
                Some(Captured::End(_))
            ));
            for (field, value) in [
                ("rawAfter", Value::Null),
                ("caretChildZero", json!(false)),
                ("offset", json!(1)),
                ("revision", json!(2)),
            ] {
                let mut changed = end_wire(unicode);
                changed["split"][field] = value;
                assert!(parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none());
            }
            for html in [
                "<b><br></b>",
                "<span style=\"color:red\"><br></span>",
                "<span><br><br></span>",
                "<span>\n</span>",
            ] {
                let mut changed = end_wire(unicode);
                changed["split"]["placeholder"]["childHtml"] = json!(html);
                assert!(parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none());
            }
            let mut changed = end_wire(unicode);
            changed["split"]["rawAfter"] = json!(blocks(&["Other parent", "\n"]));
            assert!(parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none());
        }
    }
    fn typed_end_wire() -> Value {
        let mut value = end_wire(false);
        let after = blocks(&["Keep this", "x"]);
        let event = json!({"inputType":"insertText","data":"x","selectionStart":0,"selectionEnd":0,
            "start":0,"end":0,"beforeText":"","afterText":"x","beforeRevision":1,"revision":2,"caret":1,
            "beforeInputTrusted":true,"inputTrusted":true,"after":after,"rawAfter":after,
            "html":"<div class=\"rich-editor-block\"><div class=\"rich-editor-section\"><span>Keep this</span></div><div class=\"rich-editor-section\"><span>x</span></div></div>","placeholder":null});
        value["split"]["typing"] = json!({"revision":2,"after":after,"rawAfter":after,
            "html":event["html"],"placeholder":null,"events":[event]});
        value["blocks"] = json!(after);
        value
    }
    #[test]
    fn typed_end_keeps_initial_enter_and_authorizes_final_child_together() {
        assert!(matches!(
            parse(serde_json::from_value(typed_end_wire()).unwrap(), 11, 4),
            Some(Captured::End(_))
        ));
        let mut changed = typed_end_wire();
        changed["blocks"] = changed["split"]["after"].clone();
        assert!(parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none());
        let mut changed = typed_end_wire();
        changed["split"]["rawAfter"] = Value::Null;
        assert!(parse(serde_json::from_value(changed).unwrap(), 11, 4).is_none());
        let mut middle = wire();
        middle["split"]["typing"] = typed_end_wire()["split"]["typing"].clone();
        middle["blocks"] = typed_end_wire()["blocks"].clone();
        assert!(parse(serde_json::from_value(middle).unwrap(), 11, 4).is_none());
    }
    fn typed_middle_wire() -> Value {
        let mut value = wire();
        let after = blocks(&["Keep this", "New"]);
        let mut event = typed_end_wire()["split"]["typing"]["events"][0].clone();
        event["data"] = json!("New");
        event["beforeText"] = json!("Second paragraph");
        event["afterText"] = json!("New");
        event["selectionEnd"] = json!(16);
        event["end"] = json!(16);
        event["caret"] = json!(3);
        event["after"] = json!(after);
        event["rawAfter"] = json!(after);
        event["html"] = json!(
            "<div class=\"rich-editor-block\"><div class=\"rich-editor-section\"><span>Keep this</span></div><div class=\"rich-editor-section\"><span>New</span></div></div>"
        );
        value["split"]["kind"] = json!("middle");
        value["split"]["caretChildZero"] = json!(true);
        value["split"]["rawAfter"] = value["split"]["after"].clone();
        value["split"]["typing"] = json!({"revision":2,"after":after,"rawAfter":after,
            "html":event["html"],"placeholder":null,"events":[event]});
        value["blocks"] = json!(after);
        value
    }
    #[test]
    fn typed_middle_requires_initial_halves_and_separate_final_child() {
        assert!(matches!(
            parse(serde_json::from_value(typed_middle_wire()).unwrap(), 11, 4),
            Some(Captured::Split(_))
        ));
        for (field, value) in [
            ("caretChildZero", json!(false)),
            ("rawAfter", Value::Null),
            ("typing", Value::Null),
            ("kind", json!("end")),
        ] {
            let mut bad = typed_middle_wire();
            bad["split"][field] = value;
            assert!(
                parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none(),
                "{field}"
            );
        }
        let mut bad = typed_middle_wire();
        bad["split"]["after"] = bad["blocks"].clone();
        bad["split"]["rawAfter"] = bad["blocks"].clone();
        assert!(parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
        let mut bad = typed_middle_wire();
        bad["blocks"] = bad["split"]["after"].clone();
        assert!(parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
    }
}
