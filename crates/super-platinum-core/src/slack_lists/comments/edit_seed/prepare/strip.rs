//! LUrm default options. Unknown workflow macro output is an observation gap,
//! never an invented non-text classification.
use super::*;
use serde_json::Map;

#[derive(Clone, Debug, Default)]
pub struct SyntaxObservations {
    /// Exact workflowtoken object paired with the result of the actual b/gA
    /// macro-output predicate. The literal type `string` requires no lookup.
    pub workflow_text: Vec<(Value, bool)>,
}

pub(super) fn formatting(document: &mut Document, observations: &SyntaxObservations) -> Result<()> {
    let mut result = Document::default();
    for (start, mut line, newline) in document.lines() {
        let block = truthy(newline.get("code-block"));
        // e0ER visits a synthetic final newline too. LUrm merely retains that
        // synthetic unit; Delta.compose chops it, so only real units are copied.
        if start + line.len() < document.len() {
            line.0.push(document.0[start + line.len()].clone());
        }
        for op in line.ops()? {
            let attributes = op
                .get("attributes")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if !(block || truthy(attributes.get("code"))) {
                result.0.extend(Document::capture(&[op])?.0);
                continue;
            }
            let mut attributes = if block { Map::new() } else { attributes };
            let replacement = if let Some(embed) = op["insert"]
                .as_object()
                .filter(|e| truthy(e.get("slackemoji")))
            {
                let emoji = &embed["slackemoji"];
                let text = if emoji.is_boolean() || emoji.as_object().is_some_and(|o| o.is_empty())
                {
                    ""
                } else {
                    emoji
                        .get("text")
                        .and_then(Value::as_str)
                        .ok_or(SeedUnavailable::InvalidObservation)?
                };
                Some(text.to_owned())
            } else if let Some(text) = op["insert"].as_str() {
                if truthy(op.get("attributes").and_then(|a| a.get("slackslug"))) {
                    let slug = &op["attributes"]["slackslug"];
                    if !slug.is_object() && !slug.is_array() {
                        return Err(SeedUnavailable::MalformedMessage);
                    }
                    let text = if truthy(slug.get("url")) {
                        slug["url"]
                            .as_str()
                            .ok_or(SeedUnavailable::MalformedMessage)?
                    } else {
                        text
                    };
                    attributes.remove("slackslug");
                    Some(text.to_owned())
                } else if truthy(op.get("attributes").and_then(|a| a.get("slackmention"))) {
                    attributes.remove("slackmention");
                    Some(text.to_owned())
                } else if truthy(op.get("attributes").and_then(|a| a.get("link"))) {
                    attributes.remove("link");
                    Some(text.to_owned())
                } else {
                    None
                }
            } else if let Some(token) = op["insert"]
                .get("workflowtoken")
                .filter(|v| truthy(Some(v)))
            {
                let text = if token.get("type").and_then(Value::as_str) == Some("string") {
                    true
                } else {
                    let values: Vec<_> = observations
                        .workflow_text
                        .iter()
                        .filter(|(raw, _)| raw == token)
                        .collect();
                    if values.len() != 1 {
                        return Err(SeedUnavailable::InvalidObservation);
                    }
                    values[0].1
                };
                if !text {
                    continue;
                }
                None
            } else {
                None
            };
            if let Some(text) = replacement {
                if !text.is_empty() {
                    let mut replacement =
                        Document::capture(&[json!({"insert":text,"attributes":attributes})])?;
                    // LUrm constructs replacements with Delta.insert, which
                    // omits empty attributes; retained raw operations do not.
                    replacement.normalize_insert_attributes();
                    result.0.extend(replacement.0);
                }
            } else {
                // Source clears code-block attrs only on the replacement
                // branches above. Ordinary text and text tokens are retained.
                result.0.extend(Document::capture(&[op])?.0);
            }
        }
    }
    if result.len() > 262_144 {
        return Err(SeedUnavailable::Limit);
    }
    *document = result;
    Ok(())
}
