//! B +E6Z.u1 at 2857014 calls S v+tG4587174 with its default flags.
//! Unlike the Link modal, it neither trims input nor substitutes a missing hit.
use super::*;
use serde_json::{Value, json};

pub(super) fn patterns() -> Result<&'static Patterns, ListError> {
    static PATTERNS: OnceLock<Result<Patterns, ListError>> = OnceLock::new();
    PATTERNS
        .get_or_init(Patterns::load)
        .as_ref()
        .map_err(Clone::clone)
}

impl crate::slack_lists::LinkValue {
    /// Source-backed rich-text seed, not URL-navigation authorization. Call on
    /// the creation/preflight worker: the existing detector can perform regex work.
    pub(crate) fn board_text(input: &str) -> Result<Value, ListError> {
        let p = patterns()?;
        let input_text = text(input);
        let mut hits = urls::detect(p, &input_text)?;
        merge(&mut hits, emails(p, &input_text));
        merge(&mut hits, phones(p, &input_text));
        hits.sort_by_key(|hit| hit.offset);
        // This deliberately compares URL length, not matched text length.
        // Bare domains/mail addresses acquire a prefix and remain text here.
        let element = match hits.first() {
            Some(hit) if hit.offset == 0 && hit.url.len() == input_text.len() => {
                json!({"type":"link", "url":string(&hit.url)?})
            }
            _ => json!({"type":"text", "text":input}),
        };
        Ok(json!([{"type":"rich_text","elements":[{
            "type":"rich_text_section","elements":[element]
        }]}]))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn whole_url_seed_matches_extracted_u1_source_not_link_modal_normalization() {
        // Oracle: exact B +E6Z.u1 expression evaluated with captured S v+tG.
        for (input, link) in [
            ("", false),
            ("plain text", false),
            ("https://example.com/a", true),
            ("example.com", false),
            ("a@example.com", false),
            ("202-555-0100", false),
            ("https://example.com/a ", false),
            (" https://example.com/a", false),
            ("https://example.com/a(b)", true),
            ("https://example.com/a(b)).", false),
            ("https://example.com/\u{1f680}", true),
            ("mailto:a@example.com", false),
            ("202-555-0100abc", false),
            ("https://example.com a", false),
        ] {
            let value = crate::slack_lists::LinkValue::board_text(input).unwrap();
            let element = &value[0]["elements"][0]["elements"][0];
            assert_eq!(
                element["type"],
                if link { "link" } else { "text" },
                "{input:?}"
            );
            assert_eq!(element[if link { "url" } else { "text" }], input);
        }
    }
}
