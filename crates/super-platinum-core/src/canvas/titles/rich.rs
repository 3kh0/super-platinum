//! Exact first-block chunk selection. Locale/time-zone text and final emoji DOM
//! are renderer work; this plan never substitutes the object's plain name.
use super::Error;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "source", rename_all = "camelCase")]
pub enum Chunk {
    Text(Value),
    Link(Value),
    User(Value),
    Channel(Value),
    Emoji(Value),
    Date(Value),
    Salesforce(Value),
}

/// Only constructed from a captured rich name; original chunks retain unknown
/// fields and optionality for the source-compatible renderer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RichPlan {
    raw: String,
    chunks: Vec<Chunk>,
}
impl RichPlan {
    pub fn raw(&self) -> &str {
        &self.raw
    }
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    pub(super) fn parse(raw: &str) -> Result<Self, Error> {
        let mut plan = Self {
            raw: raw.into(),
            chunks: vec![],
        };
        // Source JSON parse failure becomes an empty rich title, not plain name.
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return Ok(plan);
        };
        if value.is_null() {
            return Ok(plan);
        }
        let Some(blocks) = value.as_array() else {
            // Source's optional length guard also skips plain objects/numbers.
            if !value.is_string() {
                return Ok(plan);
            }
            if value.as_str() == Some("") {
                return Ok(plan);
            }
            return Err(Error::Unsupported);
        };
        let Some(first) = blocks.first() else {
            return Ok(plan);
        };
        let sections = first
            .get("elements")
            .and_then(Value::as_array)
            .ok_or(Error::Unsupported)?;
        for section in sections {
            if section.get("type").and_then(Value::as_str) != Some("rich_text_section") {
                continue;
            }
            let Some(elements) = section.get("elements") else {
                continue;
            };
            if elements.is_null() || elements == &Value::Bool(false) {
                continue;
            }
            let elements = elements.as_array().ok_or(Error::Unsupported)?;
            for chunk in elements {
                if chunk.is_null() {
                    return Err(Error::Unsupported);
                }
                let projected = match chunk.get("type").and_then(Value::as_str) {
                    Some("text") => Chunk::Text(chunk.clone()),
                    Some("link") => Chunk::Link(chunk.clone()),
                    Some("user") => Chunk::User(chunk.clone()),
                    Some("channel") => Chunk::Channel(chunk.clone()),
                    Some("emoji") => Chunk::Emoji(chunk.clone()),
                    Some("date") => Chunk::Date(chunk.clone()),
                    Some("salesforce_data_field") => Chunk::Salesforce(chunk.clone()),
                    _ => continue,
                };
                plan.chunks.push(projected);
            }
        }
        Ok(plan)
    }
}

/// JS split(" ") keeps empty words; do not replace it with split_whitespace.
pub(super) fn truncate(text: &str) -> String {
    let words: Vec<_> = text.split(' ').collect();
    if words.len() > 10 {
        format!("{}…", words[..10].join(" "))
    } else {
        text.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn malformed_and_empty_never_become_plain_fallback() {
        for raw in ["", "not JSON", "[]", "null", "{}", "42", "false"] {
            assert!(RichPlan::parse(raw).unwrap().chunks().is_empty());
        }
        assert!(RichPlan::parse("[{}]").is_err());
        assert!(RichPlan::parse(r#"[{"elements":"not array"}]"#).is_err());
    }
    #[test]
    fn only_first_block_rich_sections_and_known_chunks_participate() {
        let input = json!([{"elements":[
            {"type":"rich_text_quote","elements":[{"type":"text","text":"ignored"}]},
            {"type":"rich_text_section","elements":[
                {"type":"text","text":"Title","future":true},
                {"type":"unknown","text":"ignored"},
                {"type":"emoji","name":"party","url":"https://example.test/emoji"},
                {"type":"date","timestamp":123,"future":"kept"}]}]},
            {"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"ignored"}]}]}]);
        let plan = RichPlan::parse(&input.to_string()).unwrap();
        assert_eq!(plan.chunks().len(), 3);
        assert!(matches!(&plan.chunks()[0],Chunk::Text(v) if v["future"]==true));
        assert!(matches!(&plan.chunks()[2],Chunk::Date(v) if v["future"]=="kept"));
    }
    #[test]
    fn truncation_counts_literal_spaces_and_preserves_other_whitespace() {
        assert_eq!(truncate("a b c d e f g h i j k"), "a b c d e f g h i j…");
        assert_eq!(truncate("a  b c d e f g h i j"), "a  b c d e f g h i…");
        assert_eq!(truncate("a\nb\tc"), "a\nb\tc");
        assert_eq!(truncate("a b c d e f g h i j"), "a b c d e f g h i j");
    }
}
