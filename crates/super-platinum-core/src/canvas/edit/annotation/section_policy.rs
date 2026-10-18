//! Actual ordinary TextContent section-command admission. No operation IDs,
//! write authority, saved-result validation or recovery behavior lives here.
use super::{CanvasCollabSnapshot, Error, Section};
use scraper::{Html, Node};

pub(super) fn admit(snapshot: &CanvasCollabSnapshot, section: &Section) -> Result<(), Error> {
    // Retained ordinary createInitConfig never enables DISALLOW_SECTION_COMMENTS
    // (external feature 37 -> editor feature 19). The remaining actual input is
    // syncer.Document.uses_only_inline_text_comments, canonical field 70.
    if snapshot.document.boolean(70)?.unwrap_or(false) && empty(section)? {
        return Err(Error::Unsupported);
    }
    Ok(())
}

fn empty(section: &Section) -> Result<bool, Error> {
    // Keep the existing closed canonical grammar. In particular an arbitrary
    // CONTROL or unknown HTML node cannot become an empty paragraph by omission.
    super::inline::original(section)?;
    let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
    let text = content.message(1)?.ok_or(Error::Unsupported)?;
    let parsed = Html::parse_fragment(text.string(1)?.unwrap_or(""));
    if !parsed.errors.is_empty() {
        return Err(Error::Unsupported);
    }
    let mut breaks = 0usize;
    for node in parsed.root_element().descendants() {
        match node.value() {
            Node::Text(value) if !value.chars().all(whitespace) => return Ok(false),
            Node::Element(value) if value.name() == "br" => breaks += 1,
            _ => {}
        }
    }
    // YLF4.isEmpty without checkForWhitespace tolerates at most one BR. Spaces
    // are checked on original DOM text, not normalized editable TextDraft text.
    Ok(breaks <= 1)
}

fn whitespace(c: char) -> bool {
    matches!(c as u32, 9..=13 | 32 | 160 | 5760 | 8192..=8203 | 8232 | 8233 | 8239 | 8287 | 12288 | 65279)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::edit::annotation::{
        AnnotationIntent, AnnotationPresentation, F, RawMessage, section_previews,
    };
    fn snapshot(rtml: &str, policy: Option<bool>) -> CanvasCollabSnapshot {
        let mut s = super::super::tests::fixture();
        let slot = s.sections.iter_mut().find(|v| v.id() == "item").unwrap();
        let text = RawMessage::from_fields(&[F::bytes(1, rtml.as_bytes()).unwrap()]).unwrap();
        let content = RawMessage::from_fields(&[F::message(1, &text).unwrap()]).unwrap();
        *slot = Section::from_raw(
            slot.raw()
                .remove(38)
                .unwrap()
                .replace_fields(12, &[F::message(12, &content).unwrap()])
                .unwrap(),
        )
        .unwrap();
        s.document = s.document.remove(70).unwrap();
        if let Some(policy) = policy {
            s.document = s
                .document
                .replace_fields(70, &[F::uint(70, u64::from(policy)).unwrap()])
                .unwrap();
        }
        s
    }
    #[test]
    fn canonical_document_inline_only_policy_gates_actual_section_choices_and_intents() {
        let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
        for raw in ["", "<br>", " ", "<b> </b>", "&#8203;", "&#65279;<br>"] {
            for policy in [None, Some(false), Some(true)] {
                let s = snapshot(raw, policy);
                let expected = policy != Some(true);
                assert_eq!(
                    AnnotationIntent::for_section(&s, "item", &presentation).is_ok(),
                    expected,
                    "{raw:?}/{policy:?}"
                );
                assert_eq!(
                    section_previews(&s, &presentation)
                        .unwrap()
                        .iter()
                        .any(|p| p.id == "item"),
                    expected
                );
            }
        }
        for raw in ["<br><br>", "x", "<i>&#8204;</i>", "<b>\u{1f600}</b>"] {
            let s = snapshot(raw, Some(true));
            assert!(
                AnnotationIntent::for_section(&s, "item", &presentation).is_ok(),
                "{raw:?}"
            );
        }
    }
    #[test]
    fn changed_document_policy_refuses_captured_intent_without_modifying_original() {
        let s = snapshot("", Some(false));
        let p = AnnotationPresentation::ordinary(None, None).unwrap();
        let super::super::SectionAnnotationChoice::New(intent) =
            AnnotationIntent::for_section(&s, "item", &p).unwrap()
        else {
            panic!("new")
        };
        let mut changed = s.clone();
        changed.document = changed
            .document
            .replace_fields(70, &[F::uint(70, 1).unwrap()])
            .unwrap();
        assert!(intent.prepare(&changed).is_err());
        assert_eq!(s.document.boolean(70).unwrap(), Some(false));
        assert_eq!(s.sections, changed.sections);
    }
    #[test]
    fn raw_canonical_empty_checks_match_actual_retained_method_goldens() {
        let rows: serde_json::Value =
            serde_json::from_str(include_str!("section_policy_cases.json")).unwrap();
        for row in rows.as_array().unwrap() {
            let s = snapshot(row["rtml"].as_str().unwrap(), Some(true));
            assert_eq!(
                empty(s.section("item").unwrap()).unwrap(),
                row["empty"].as_bool().unwrap(),
                "{}",
                row["name"]
            );
        }
    }
}
