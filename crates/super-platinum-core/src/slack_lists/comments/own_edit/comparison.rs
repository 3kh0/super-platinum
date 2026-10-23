//! Conservative rich-only subset of E1L/ text insertion, mM05 Delta.push and
//! AGzE equality. Comparison copies never replace captured or durable blocks.
use super::content;
use serde_json::Value;

pub(super) fn unchanged(original: &[Value], edited: &[Value]) -> bool {
    if original == edited {
        return true;
    }
    // The seed already validated original. Bound and validate edited content
    // before allocating copies; malformed content cannot become a local finish.
    if content::fallback(edited).is_err() {
        return false;
    }
    normalized(original) == normalized(edited)
}

fn normalized(blocks: &[Value]) -> Vec<Value> {
    let mut blocks = blocks.to_vec();
    for block in &mut blocks {
        normalize_container(block);
    }
    blocks
}

fn normalize_container(value: &mut Value) {
    match value.get("type").and_then(Value::as_str) {
        Some("rich_text" | "rich_text_list") => {
            if let Some(children) = value.get_mut("elements").and_then(Value::as_array_mut) {
                for child in children {
                    normalize_container(child);
                }
            }
        }
        Some("rich_text_section") => {
            let Some(children) = value.get_mut("elements").and_then(Value::as_array_mut) else {
                return;
            };
            let mut result: Vec<Value> = Vec::with_capacity(children.len());
            let mut previous_run = false;
            for child in std::mem::take(children) {
                let text = ordinary_text(&child);
                if previous_run
                    && text.is_some()
                    && result.last().unwrap().get("style") == child.get("style")
                {
                    if let Some(Value::String(previous)) =
                        result.last_mut().unwrap().get_mut("text")
                    {
                        previous.push_str(text.unwrap());
                    }
                } else {
                    previous_run = text.is_some();
                    result.push(child);
                }
            }
            *children = result;
        }
        // Quotes, preformatted content and semantic leaves remain exact. Never
        // traverse unknown/opaque metadata looking for nested text-like shapes.
        _ => {}
    }
}

fn ordinary_text(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    if object.get("type")?.as_str()? != "text"
        || object
            .keys()
            .any(|key| !matches!(key.as_str(), "type" | "text" | "style"))
    {
        return None;
    }
    if let Some(style) = object.get("style") {
        if style.as_object()?.iter().any(|(key, value)| {
            !matches!(key.as_str(), "bold" | "italic" | "strike" | "code") || !value.is_boolean()
        }) {
            return None;
        }
    }
    let text = object.get("text")?.as_str()?;
    (!text.is_empty() && !text.contains(['\r', '\n'])).then_some(text)
}

#[cfg(test)]
mod tests;
