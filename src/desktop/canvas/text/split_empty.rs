//! Empty Enter proof, with optional ordered edits to only the second paragraph.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Proof {
    before_html: String,
    after_html: String,
    before: Leaf,
    sections: Vec<Leaf>,
    caret_section: usize,
    caret_offset: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Leaf {
    html: String,
    span_depth: usize,
}
fn valid(leaf: &Leaf) -> bool {
    leaf.span_depth <= 16
        && leaf.html
            == format!(
                "{}<br>{}",
                "<span>".repeat(leaf.span_depth),
                "</span>".repeat(leaf.span_depth)
            )
}
fn html(leaves: &[&Leaf]) -> String {
    format!(
        "<div class=\"rich-editor-block\">{}</div>",
        leaves
            .iter()
            .map(|leaf| format!("<div class=\"rich-editor-section\">{}</div>", leaf.html))
            .collect::<String>()
    )
}
pub(super) fn parse(
    w: &EventWitness,
    before: &TextDraft,
    drafts: &[TextDraft],
) -> Option<EndWitness> {
    let p = w.empty_proof.as_ref()?;
    if w.before != serde_json::json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[]}]}]).as_array()?.to_vec() {return None;}
    if w.offset != 0
        || w.placeholder.is_some()
        || w.leading_placeholder.is_some()
        || w.caret_original_zero.is_some()
        || w.caret_child_zero != Some(true)
        || before != &TextDraft::default()
        || drafts.len() != 2
        || drafts.iter().any(|d| d != &TextDraft::default())
        || p.caret_section != 1
        || p.caret_offset != 0
        || p.sections.len() != 2
        || !valid(&p.before)
        || p.sections.iter().any(|leaf| !valid(leaf))
        || p.before_html != html(&[&p.before])
        || p.after_html != html(&p.sections.iter().collect::<Vec<_>>())
    {
        return None;
    }
    let mut raw = w.raw_after.as_ref()?.clone();
    if raw.len() != 1 || raw[0]["type"] != "rich_text" {
        return None;
    }
    let sections = raw[0]["elements"].as_array_mut()?;
    if sections.len() != 2 {
        return None;
    }
    for (index, section) in sections.iter_mut().enumerate() {
        let expected = if p.sections[index].span_depth == 0 {
            serde_json::json!([])
        } else {
            serde_json::json!([{ "type":"text", "text":"\n" }])
        };
        if section["elements"] != expected {
            return None;
        }
        if section["type"] != "rich_text_section"
            || (section["elements"] != serde_json::json!([])
                && section["elements"] != serde_json::json!([{"type":"text","text":"\n"}]))
        {
            return None;
        }
        section["elements"] = serde_json::json!([]);
    }
    if raw != w.after {
        return None;
    }
    let initial = EndWitness::empty();
    if let Some(value) = &w.typing {
        let updates = super::typing::validate_empty(value, p)?;
        initial.with_child_updates(&updates).ok()
    } else {
        Some(initial)
    }
}

/// The original first placeholder remains byte-for-byte fixed while the
/// selected second section receives native edits. This projection is local to
/// a previously verified empty Enter, never a general newline normalization.
pub(super) fn typing_projection(
    raw: &[Value],
    proof: &Proof,
    current_html: &str,
) -> Option<Vec<Value>> {
    let first = proof.sections.first()?;
    if !valid(first) || !current_html.starts_with(&format!(
        "<div class=\"rich-editor-block\"><div class=\"rich-editor-section\">{}</div><div class=\"rich-editor-section\">", first.html
    )) || !current_html.ends_with("</div></div>") { return None; }
    let mut projected = raw.to_vec();
    if projected.len() != 1 || projected[0]["type"] != "rich_text" {
        return None;
    }
    let sections = projected[0]["elements"].as_array_mut()?;
    if sections.len() != 2 || sections[0]["type"] != "rich_text_section" {
        return None;
    }
    let expected = if first.span_depth == 0 {
        serde_json::json!([])
    } else {
        serde_json::json!([{"type":"text","text":"\n"}])
    };
    if sections[0]["elements"] != expected {
        return None;
    }
    sections[0]["elements"] = serde_json::json!([]);
    Some(projected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn wire() -> Value {
        let before =
            json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[]}]}]);
        let after = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[]},{"type":"rich_text_section","elements":[]}]}]);
        let leaf = json!({"html":"<br>","spanDepth":0});
        json!({"blocks":after,"split":{"kind":"empty","offset":0,"generation":"11","visit":"4","before":before,"after":after,"rawAfter":after,"revision":1,"beforeInputTrusted":true,"inputTrusted":true,"inputType":"insertParagraph","caretChildZero":true,"emptyProof":{"beforeHtml":"<div class=\"rich-editor-block\"><div class=\"rich-editor-section\"><br></div></div>","afterHtml":"<div class=\"rich-editor-block\"><div class=\"rich-editor-section\"><br></div><div class=\"rich-editor-section\"><br></div></div>","before":leaf,"sections":[leaf,leaf],"caretSection":1,"caretOffset":0}}})
    }
    #[test]
    fn empty_enter_requires_exact_initial_placeholders() {
        assert!(matches!(
            super::super::parse(serde_json::from_value(wire()).unwrap(), 11, 4),
            Some(Captured::End(_))
        ));
        for (key, value) in [
            ("offset", json!(1)),
            ("revision", json!(2)),
            ("kind", json!("start")),
            ("emptyProof", Value::Null),
            ("caretChildZero", json!(false)),
        ] {
            let mut bad = wire();
            bad["split"][key] = value;
            assert!(super::super::parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
        }
        for leaf in ["<br><br>", "<span>x</span>", "<b><br></b>"] {
            let mut bad = wire();
            bad["split"]["emptyProof"]["sections"][0]["html"] = json!(leaf);
            assert!(super::super::parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
        }
        let mut bad = wire();
        bad["split"] = Value::Null;
        assert!(super::super::parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
    }

    fn typed(states: &[&str], depth: usize) -> Value {
        let mut v = wire();
        let first = format!("{}<br>{}", "<span>".repeat(depth), "</span>".repeat(depth));
        v["split"]["emptyProof"]["sections"][0] = json!({"html":first,"spanDepth":depth});
        v["split"]["emptyProof"]["afterHtml"] = json!(format!(
            r#"<div class="rich-editor-block"><div class="rich-editor-section">{first}</div><div class="rich-editor-section"><br></div></div>"#
        ));
        if depth > 0 {
            v["split"]["rawAfter"][0]["elements"][0]["elements"] =
                json!([{"type":"text","text":"\n"}]);
        }
        let mut previous = "";
        let mut events = Vec::new();
        for (index, text) in states.iter().enumerate() {
            let mut after = v["split"]["after"].clone();
            after[0]["elements"][1]["elements"] = if text.is_empty() {
                json!([])
            } else {
                json!([{"type":"text","text":text}])
            };
            let mut raw = after.clone();
            if depth > 0 {
                raw[0]["elements"][0]["elements"] = json!([{"type":"text","text":"\n"}]);
            }
            let placeholder = text.is_empty().then(|| json!({"blockCount":1,"sectionCount":2,"childHtml":"<br>","spanDepth":0,"caretOffset":0}));
            let escaped = text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let body = if text.is_empty() {
                "<br>".to_string()
            } else {
                format!("<span>{escaped}</span>")
            };
            let html = format!(
                r#"<div class="rich-editor-block"><div class="rich-editor-section">{first}</div><div class="rich-editor-section">{body}</div></div>"#
            );
            let len = previous.encode_utf16().count();
            events.push(json!({"inputType":if text.is_empty(){"deleteContentBackward"}else{"insertText"},"data":if text.is_empty(){Value::Null}else{json!(text)},"selectionStart":0,"selectionEnd":len,"start":0,"end":len,"beforeText":previous,"afterText":text,"beforeRevision":index+1,"revision":index+2,"caret":text.encode_utf16().count(),"beforeInputTrusted":true,"inputTrusted":true,"after":after,"rawAfter":raw,"html":html,"placeholder":placeholder}));
            previous = text;
        }
        let last = events.last().unwrap();
        v["blocks"] = last["after"].clone();
        v["split"]["typing"] = json!({"revision":last["revision"],"after":last["after"],"rawAfter":last["rawAfter"],"html":last["html"],"placeholder":last["placeholder"],"events":events});
        v
    }

    #[test]
    fn empty_typing_preserves_initial_pair_and_ordered_return_to_empty() {
        for depth in [0, 1, 3] {
            for states in [
                &["New", "New paragraph"][..],
                &["New \u{65e5}\u{672c}\u{1f600} &", ""][..],
                &["New", "New draftx", "New", ""][..],
            ] {
                let Some(Captured::End(w)) = super::super::parse(
                    serde_json::from_value(typed(states, depth)).unwrap(),
                    11,
                    4,
                ) else {
                    panic!("typed empty rejected")
                };
                assert!(w.is_empty_enter() && w.is_empty_typing());
            }
        }
    }

    #[test]
    fn empty_typing_refuses_changed_first_placeholder_or_unbound_event() {
        let v = typed(&["New", ""], 1);
        for (field, value) in [
            ("beforeInputTrusted", json!(false)),
            ("revision", json!(9)),
            ("selectionEnd", json!(1)),
            ("caret", json!(0)),
            ("html", json!("<div>unbound</div>")),
        ] {
            let mut bad = v.clone();
            bad["split"]["typing"]["events"][0][field] = value;
            assert!(super::super::parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
        }
        let mut bad = v.clone();
        bad["split"]["typing"]["events"][0]["rawAfter"][0]["elements"][0]["elements"] = json!([]);
        assert!(super::super::parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
        let mut bad = v;
        bad["split"]["typing"]["events"][0]["after"][0]["elements"][0]["elements"] =
            json!([{"type":"text","text":"x"}]);
        assert!(super::super::parse(serde_json::from_value(bad).unwrap(), 11, 4).is_none());
    }
}
