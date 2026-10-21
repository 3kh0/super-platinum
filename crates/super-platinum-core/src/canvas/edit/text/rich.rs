//! Explicit message-rich-text adapter for the renderer's existing editor.
//! Multiple blocks/paragraphs are not a single canonical canvas section edit.
use serde_json::{Value, json};

use super::{Error, TextDraft, TextRun, TextStyle};

fn keys(value: &Value, allowed: &[&str]) -> Result<(), Error> {
    let object = value.as_object().ok_or(Error::Unsupported)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::Unsupported);
    }
    Ok(())
}

impl TextDraft {
    pub fn from_rich_blocks(blocks: &[Value]) -> Result<Self, Error> {
        let [block] = blocks else {
            return Err(Error::Unsupported);
        };
        keys(block, &["type", "elements"])?;
        if block["type"] != "rich_text" {
            return Err(Error::Unsupported);
        }
        let sections = block["elements"].as_array().ok_or(Error::Unsupported)?;
        let [section] = sections.as_slice() else {
            return Err(Error::Unsupported);
        };
        keys(section, &["type", "elements"])?;
        if section["type"] != "rich_text_section" {
            return Err(Error::Unsupported);
        }
        let elements = section["elements"].as_array().ok_or(Error::Unsupported)?;
        if elements.len() > 4096 {
            return Err(Error::Unsupported);
        }
        let mut runs = Vec::with_capacity(elements.len());
        for element in elements {
            keys(element, &["type", "text", "url", "style"])?;
            let mut style = TextStyle::default();
            if let Some(value) = element.get("style") {
                keys(value, &["bold", "italic", "underline", "strike", "code"])?;
                for (key, target) in [
                    ("bold", &mut style.bold),
                    ("italic", &mut style.italic),
                    ("underline", &mut style.underline),
                    ("strike", &mut style.strike),
                    ("code", &mut style.code),
                ] {
                    if let Some(value) = value.get(key) {
                        *target = value.as_bool().ok_or(Error::Unsupported)?;
                    }
                }
            }
            match element["type"].as_str() {
                Some("text") if element.get("url").is_none() => {}
                Some("link") => {
                    style.link = Some(element["url"].as_str().ok_or(Error::Unsupported)?.into())
                }
                _ => return Err(Error::Unsupported),
            }
            runs.push(TextRun {
                text: element["text"].as_str().ok_or(Error::Unsupported)?.into(),
                style,
            });
        }
        let draft = Self { runs };
        draft.validate()?;
        Ok(draft)
    }

    pub fn rich_blocks(&self) -> Vec<Value> {
        let elements = self
            .runs
            .iter()
            .map(|run| {
                let mut element = json!({"type":"text", "text":run.text});
                if let Some(href) = &run.style.link {
                    element["type"] = json!("link");
                    element["url"] = json!(href);
                }
                let mut style = serde_json::Map::new();
                for (key, set) in [
                    ("bold", run.style.bold),
                    ("italic", run.style.italic),
                    ("underline", run.style.underline),
                    ("strike", run.style.strike),
                    ("code", run.style.code),
                ] {
                    if set {
                        style.insert(key.into(), json!(true));
                    }
                }
                if !style.is_empty() {
                    element["style"] = Value::Object(style);
                }
                element
            })
            .collect::<Vec<_>>();
        vec![
            json!({"type":"rich_text", "elements":[{"type":"rich_text_section", "elements":elements}]}),
        ]
    }
}
