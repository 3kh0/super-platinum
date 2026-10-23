//! Sealed renderer-only proof for one empty insertion before the unchanged seed.
use super::{EventWitness, TextDraft, Value};
use serde::Deserialize;

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LeadingPlaceholder {
    block_count: usize,
    section_count: usize,
    first_html: String,
    span_depth: usize,
    caret_section: usize,
    caret_offset: usize,
}

pub(in crate::canvas::text) struct Start {
    original: TextDraft,
    updates: Vec<TextDraft>,
}
impl Start {
    pub(in crate::canvas::text) fn original(&self) -> &TextDraft {
        &self.original
    }
    pub(in crate::canvas::text) fn updates(&self) -> &[TextDraft] {
        &self.updates
    }
    pub(in crate::canvas::text) fn matches(&self, seed: &TextDraft) -> bool {
        self.original == *seed
    }
}

pub(super) fn parse(w: &EventWitness, before: &TextDraft, parts: &[TextDraft]) -> Option<Start> {
    let proof = w.leading_placeholder.as_ref()?;
    let raw = w.raw_after.as_ref()?;
    if w.offset != 0
        || w.caret_original_zero != Some(true)
        || w.caret_child_zero.is_some()
        || w.placeholder.is_some()
        || parts.len() != 2
        || parts[0] != TextDraft::default()
        || parts[1] != *before
        || before.runs.is_empty()
        || before.runs.iter().any(|r| r.style != Default::default())
        || proof.block_count != 1
        || proof.section_count != 2
        || proof.span_depth > 16
        || proof.caret_section != 1
        || proof.caret_offset != 0
        || proof.first_html
            != format!(
                "{}<br>{}",
                "<span>".repeat(proof.span_depth),
                "</span>".repeat(proof.span_depth)
            )
        || raw.len() != 1
    {
        return None;
    }
    let text = before
        .runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<String>();
    if text.is_empty()
        || text
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}' | '\u{feff}'))
    {
        return None;
    }
    if leading_projection(raw, proof)? != w.after {
        return None;
    }
    let updates = if let Some(typing) = &w.typing {
        super::typing::validate_start(typing, before, proof)?
    } else {
        Vec::new()
    };
    Some(Start {
        original: before.clone(),
        updates,
    })
}

pub(super) fn leading_projection(raw: &[Value], proof: &LeadingPlaceholder) -> Option<Vec<Value>> {
    if raw.len() != 1
        || proof.block_count != 1
        || proof.section_count != 2
        || proof.span_depth > 16
        || proof.caret_section != 1
        || proof.caret_offset != 0
        || proof.first_html
            != format!(
                "{}<br>{}",
                "<span>".repeat(proof.span_depth),
                "</span>".repeat(proof.span_depth)
            )
    {
        return None;
    }
    let mut canonical = raw.to_vec();
    let sections = canonical[0].get_mut("elements")?.as_array_mut()?;
    if sections.len() != 2 {
        return None;
    }
    let runs = sections[0].get_mut("elements")?;
    if *runs != serde_json::json!([]) && *runs != serde_json::json!([{"type":"text","text":"\n"}]) {
        return None;
    }
    *runs = serde_json::json!([]);
    Some(canonical)
}

#[cfg(test)]
mod tests {
    use super::super::{Capture, Captured, parse};
    use super::*;
    use serde_json::{Value, json};
    fn blocks(text: &str) -> Vec<Value> {
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text}]}]}),
        ]
    }
    fn capture(text: &str) -> Value {
        let before = blocks(text);
        let mut after = before.clone();
        after[0]["elements"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"type":"rich_text_section","elements":[]}));
        let mut raw = after.clone();
        raw[0]["elements"][0]["elements"] = json!([{"type":"text","text":"\n"}]);
        json!({"blocks":after,"split":{"kind":"start","generation":"11","visit":"4","offset":0,
          "before":before,"after":after,"rawAfter":raw,"revision":1,"beforeInputTrusted":true,
          "inputTrusted":true,"inputType":"insertParagraph","caretOriginalZero":true,
          "leadingPlaceholder":{"blockCount":1,"sectionCount":2,"firstHtml":"<span><br></span>","spanDepth":1,"caretSection":1,"caretOffset":0}}})
    }
    fn parsed(value: Value) -> Option<Captured> {
        parse(serde_json::from_value::<Capture>(value).unwrap(), 11, 4)
    }
    #[test]
    fn start_proof_preserves_exact_original_seed_and_projects_only_leading_placeholder() {
        for text in [
            "Keep thisSecond paragraph",
            "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
        ] {
            let Some(Captured::Start(start)) = parsed(capture(text)) else {
                panic!("valid start rejected")
            };
            let seed = TextDraft::from_rich_blocks(&blocks(text)).unwrap();
            assert!(start.matches(&seed));
            assert!(!start.matches(&TextDraft::default()));
            let mut direct = capture(text);
            direct["split"]["rawAfter"] = direct["split"]["after"].clone();
            direct["split"]["leadingPlaceholder"]["firstHtml"] = json!("<br>");
            direct["split"]["leadingPlaceholder"]["spanDepth"] = json!(0);
            assert!(matches!(parsed(direct), Some(Captured::Start(_))));
        }
    }
    #[test]
    fn invalidated_start_never_falls_back_to_saving_only_modified_original() {
        let mut value = capture("Seed");
        value["split"] = Value::Null;
        value["blocks"][0]["elements"][1]["elements"][0]["text"] = json!("Changed Seed");
        assert!(parsed(value.clone()).is_none());
        value["blocks"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"type":"rich_text_section","elements":[]}));
        assert!(parsed(value).is_none());
    }
    fn typed_capture(source: &str, erase: bool) -> Value {
        let mut value = capture(source);
        let first = format!("x{source}");
        let final_text = if erase { "" } else { source };
        let mut events = Vec::new();
        for (i, before, after, start, end, selection_start, selection_end, data) in [
            (0, source, first.as_str(), 0, 0, 0, 0, Some("x")),
            (
                1,
                first.as_str(),
                final_text,
                0,
                if erase {
                    first.encode_utf16().count()
                } else {
                    1
                },
                if erase { 0 } else { 1 },
                if erase {
                    first.encode_utf16().count()
                } else {
                    1
                },
                None,
            ),
        ] {
            let mut canonical = value["split"]["after"].clone();
            canonical[0]["elements"][1]["elements"] = if after.is_empty() {
                json!([])
            } else {
                json!([{"type":"text","text":after}])
            };
            let mut raw = canonical.clone();
            raw[0]["elements"][0]["elements"] = json!([{"type":"text","text":"\n"}]);
            if after.is_empty() {
                raw[0]["elements"][1]["elements"] = json!([{"type":"text","text":"\n"}]);
            }
            let html = format!(
                "<div class=\"rich-editor-block\"><div class=\"rich-editor-section\"><span><br></span></div><div class=\"rich-editor-section\"><span>{}</span></div></div>",
                if after.is_empty() { "<br>" } else { after }
            );
            events.push(json!({"inputType":if i==0{"insertText"}else{"deleteContentBackward"},"data":data,"selectionStart":selection_start,"selectionEnd":selection_end,"start":start,"end":end,"beforeText":before,"afterText":after,"beforeRevision":i+1,"revision":i+2,"caret":if i==0{1}else{0},"beforeInputTrusted":true,"inputTrusted":true,"after":canonical,"rawAfter":raw,"html":html,"placeholder":if after.is_empty(){json!({"blockCount":1,"sectionCount":2,"childHtml":"<span><br></span>","spanDepth":1,"caretOffset":0})}else{Value::Null}}));
        }
        let last = events.last().unwrap();
        value["blocks"] = last["after"].clone();
        value["split"]["typing"] = json!({"revision":3,"after":last["after"],"rawAfter":last["rawAfter"],"html":last["html"],"placeholder":last["placeholder"],"events":events});
        value
    }
    #[test]
    fn start_typing_keeps_initial_custody_and_empty_or_exact_revert_reports() {
        for source in ["Seed", "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}"] {
            for erase in [false, true] {
                let Some(Captured::Start(start)) = parsed(typed_capture(source, erase)) else {
                    panic!("typed start refused")
                };
                assert_eq!(start.original().runs[0].text, source);
                assert_eq!(start.updates().len(), 2);
                assert_eq!(start.updates()[0].runs[0].text, format!("x{source}"));
                let final_text = start.updates()[1]
                    .runs
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>();
                assert_eq!(final_text, if erase { "" } else { source });
            }
        }
    }
    #[test]
    fn start_typing_refuses_changed_first_row_and_unbound_final_ledger() {
        for field in ["after", "rawAfter"] {
            let mut value = typed_capture("Seed", false);
            value["split"]["typing"]["events"][0][field][0]["elements"][0]["elements"] =
                json!([{"type":"text","text":"Changed"}]);
            assert!(parsed(value).is_none());
        }
        for (field, changed) in [
            ("beforeInputTrusted", json!(false)),
            ("revision", json!(9)),
            ("beforeText", json!("Wrong")),
            ("selectionEnd", json!(2)),
        ] {
            let mut value = typed_capture("Seed", false);
            value["split"]["typing"]["events"][1][field] = changed;
            assert!(parsed(value).is_none(), "{field}");
        }
        let mut value = typed_capture("Seed", true);
        value["blocks"] = value["split"]["after"].clone();
        assert!(parsed(value).is_none());
    }
    #[test]
    fn start_rejects_cross_branch_caret_revision_trust_and_source_mutation() {
        for (key, value) in [
            ("offset", json!(1)),
            ("caretOriginalZero", json!(false)),
            ("caretChildZero", json!(true)),
            ("revision", json!(2)),
            ("inputTrusted", json!(false)),
            ("beforeInputTrusted", json!(false)),
            ("generation", json!("12")),
            ("visit", json!("5")),
            ("inputType", json!("insertText")),
        ] {
            let mut v = capture("Seed");
            v["split"][key] = value;
            assert!(parsed(v).is_none(), "{key}");
        }
        for (key, value) in [
            ("caretSection", json!(0)),
            ("caretOffset", json!(1)),
            ("spanDepth", json!(17)),
            ("firstHtml", json!("<span class='x'><br></span>")),
            ("sectionCount", json!(3)),
        ] {
            let mut v = capture("Seed");
            v["split"]["leadingPlaceholder"][key] = value;
            assert!(parsed(v).is_none(), "{key}");
        }
        let mut v = capture("Seed");
        v["split"]["before"] = json!(blocks("Different"));
        assert!(parsed(v).is_none());
        let mut v = capture("Seed");
        v["split"]["rawAfter"][0]["elements"][0]["elements"][0]["text"] = json!("\n\n");
        assert!(parsed(v).is_none());
        for text in ["", "A\u{feff}", "A\u{2028}", "A\nB"] {
            assert!(parsed(capture(text)).is_none());
        }
    }
}
