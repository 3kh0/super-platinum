//! Exact rich comment content. The fallback is for accessibility and previews;
//! it is never used as proof that a formatted message was delivered unchanged.
use super::{Invalid, reply::valid_text};
use serde_json::{Value, json};

pub const MAX_BLOCK_BYTES: usize = 96 * 1024;
const MAX_NODES: usize = 2048;

/// Validate the shapes emitted by the shared native rich editor. Unknown fields
/// are retained byte-for-value; an unknown node kind is refused, not flattened.
/// Empty sections are valid local drafts, but not sendable messages.
pub fn fallback(blocks: &[Value]) -> Result<String, Invalid> {
    if blocks.len() > 50
        || serde_json::to_vec(blocks)
            .map_err(|_| Invalid::Message)?
            .len()
            > MAX_BLOCK_BYTES
    {
        return Err(Invalid::Limit);
    }
    let mut count = 0;
    let mut pieces = Vec::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("rich_text") {
            return Err(Invalid::Message);
        }
        if let Some(id) = block.get("block_id") {
            bounded_string(id, 255)?;
        }
        pieces.push(render(block, 0, &mut count)?);
    }
    let text = pieces.join("\n");
    if !valid_text(&text) {
        return Err(Invalid::Limit);
    }
    Ok(text)
}

pub fn plain(text: &str) -> Vec<Value> {
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[{"type":"text","text":text}]}]}),
    ]
}

fn bounded_string(value: &Value, max: usize) -> Result<&str, Invalid> {
    let text = value.as_str().ok_or(Invalid::Message)?;
    if text.len() > max || !valid_text(text) {
        return Err(Invalid::Limit);
    }
    Ok(text)
}
fn member<'a>(value: &'a Value, key: &str) -> Result<&'a str, Invalid> {
    bounded_string(value.get(key).ok_or(Invalid::Message)?, 12_000 * 4)
}
fn identity(value: &Value, key: &str) -> Result<String, Invalid> {
    let text = member(value, key)?;
    if text.is_empty() || text.len() > 256 || text.chars().any(char::is_whitespace) {
        return Err(Invalid::Identity);
    }
    Ok(text.into())
}
fn render(value: &Value, depth: usize, count: &mut usize) -> Result<String, Invalid> {
    *count += 1;
    if depth > 5 || *count > MAX_NODES || !value.is_object() {
        return Err(Invalid::Limit);
    }
    let kind = member(value, "type")?;
    if let Some(style) = value.get("style").filter(|_| kind != "rich_text_list") {
        let style = style.as_object().ok_or(Invalid::Message)?;
        for flag in ["bold", "italic", "strike", "code"] {
            if style.get(flag).is_some_and(|v| !v.is_boolean()) {
                return Err(Invalid::Message);
            }
        }
    }
    match kind {
        "rich_text"
        | "rich_text_section"
        | "rich_text_quote"
        | "rich_text_preformatted"
        | "rich_text_list" => {
            let elements = value
                .get("elements")
                .and_then(Value::as_array)
                .ok_or(Invalid::Message)?;
            if kind == "rich_text_list" {
                if !matches!(member(value, "style")?, "bullet" | "ordered") {
                    return Err(Invalid::Message);
                }
                for key in ["indent", "offset", "border"] {
                    if value
                        .get(key)
                        .is_some_and(|v| v.as_u64().is_none_or(|n| n > 1_000_000))
                    {
                        return Err(Invalid::Message);
                    }
                }
            }
            let mut parts = Vec::new();
            for element in elements {
                let child = member(element, "type")?;
                let allowed = match kind {
                    "rich_text" => matches!(
                        child,
                        "rich_text_section"
                            | "rich_text_list"
                            | "rich_text_quote"
                            | "rich_text_preformatted"
                    ),
                    "rich_text_list" => child == "rich_text_section",
                    _ => !child.starts_with("rich_text"),
                };
                if !allowed {
                    return Err(Invalid::Message);
                }
                parts.push(render(element, depth + 1, count)?);
            }
            Ok(
                parts.join(if matches!(kind, "rich_text" | "rich_text_list") {
                    "\n"
                } else {
                    ""
                }),
            )
        }
        "text" => Ok(member(value, "text")?.into()),
        "link" => {
            let url = member(value, "url")?;
            if !["https:", "http:", "mailto:", "slack:"]
                .iter()
                .any(|s| url.to_ascii_lowercase().starts_with(s))
            {
                return Err(Invalid::Message);
            }
            Ok(value
                .get("text")
                .map(|v| bounded_string(v, 48_000))
                .transpose()?
                .unwrap_or(url)
                .into())
        }
        "user" => Ok(format!("@{}", identity(value, "user_id")?)),
        "channel" => Ok(format!("#{}", identity(value, "channel_id")?)),
        "usergroup" => Ok(format!("@{}", identity(value, "usergroup_id")?)),
        "emoji" => Ok(format!(":{}:", identity(value, "name")?)),
        "broadcast" => {
            let range = member(value, "range")?;
            if !matches!(range, "here" | "channel" | "everyone") {
                return Err(Invalid::Message);
            }
            Ok(format!("@{range}"))
        }
        // Retained B8J14.createSlugChunkFromOp emits typed native file slugs.
        // Their label is an accessibility fallback, never a replacement for the
        // file/canvas/list identity or its exact serialized token provenance.
        "file" | "canvas" | "list_record" => {
            identity(value, "file_id")?;
            for key in ["section_id", "record_id", "view_id"] {
                if value.get(key).is_some() {
                    identity(value, key)?;
                }
            }
            if value
                .get("is_skill_invocation")
                .is_some_and(|v| !v.is_boolean())
            {
                return Err(Invalid::Message);
            }
            if let Some(url) = value.get("url") {
                bounded_string(url, 48_000)?;
            }
            Ok(member(value, "text")?.into())
        }
        _ => Err(Invalid::Message),
    }
}

/// Slack documents generated block_id values when callers omit them. That is
/// the only response addition admitted here; every rich leaf/field stays exact.
pub fn confirmed(expected: &[Value], observed: &[Value]) -> bool {
    if expected.len() != observed.len() {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    expected.iter().zip(observed).all(|(expected, observed)| {
        let mut observed = observed.clone();
        if let Some(id) = observed.get("block_id") {
            let Ok(id) = bounded_string(id, 255) else {
                return false;
            };
            if id.is_empty() || !ids.insert(id.to_owned()) {
                return false;
            }
            if expected.get("block_id").is_none() {
                observed.as_object_mut().unwrap().remove("block_id");
            }
        }
        expected == &observed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_block_ids_cannot_collide_with_an_explicit_sibling() {
        let mut expected = plain("one");
        expected[0]["block_id"] = json!("explicit");
        expected.extend(plain("two"));
        let mut observed = expected.clone();
        observed[1]["block_id"] = json!("explicit");
        assert!(!confirmed(&expected, &observed));
        observed[1]["block_id"] = json!("generated");
        assert!(confirmed(&expected, &observed));
        expected[1]["block_id"] = json!("explicit");
        assert!(!confirmed(&expected, &expected));
    }
    #[test]
    fn rich_content_is_not_reduced_to_the_same_fallback() {
        let a = plain("Same text");
        let mut b = a.clone();
        b[0]["elements"][0]["elements"][0]["style"] = json!({"bold":true});
        assert_eq!(fallback(&a).unwrap(), fallback(&b).unwrap());
        assert!(!confirmed(&a, &b));
        let mut accepted = b.clone();
        accepted[0]["block_id"] = json!("generated");
        assert!(confirmed(&b, &accepted));
        accepted[0]["elements"][0]["elements"][0]["style"]["bold"] = json!(false);
        assert!(!confirmed(&b, &accepted));
    }
    #[test]
    fn lists_links_unicode_and_opaque_tokens_remain_structured() {
        let blocks = vec![json!({"type":"rich_text","elements":[
            {"type":"rich_text_list","style":"ordered","indent":1,"elements":[
                {"type":"rich_text_section","elements":[{"type":"text","text":"\u{1f600}","style":{"italic":true}},
                {"type":"link","url":"https://example.invalid","text":"link"},{"type":"user","user_id":"U1"}]}]},
            {"type":"rich_text_preformatted","elements":[{"type":"text","text":"x\ny"}]}]})];
        assert_eq!(fallback(&blocks).unwrap(), "\u{1f600}link@U1\nx\ny");
        assert!(confirmed(&blocks, &blocks));
        let mut bad = blocks.clone();
        bad[0]["elements"][0]["elements"][0]["elements"][0]["type"] = json!("future_content");
        assert!(fallback(&bad).is_err());
    }
    #[test]
    fn limits_and_invalid_nodes_do_not_gain_sendable_text() {
        assert!(fallback(&plain("")).unwrap().is_empty());
        assert!(fallback(&plain(&"x".repeat(12_001))).is_err());
        assert!(fallback(&plain("\0")).is_err());
        let mut invalid = plain("x");
        invalid[0]["elements"][0]["elements"][0]["style"] = json!({"bold":1});
        assert!(fallback(&invalid).is_err());
    }
    #[test]
    fn native_file_canvas_and_list_tokens_retain_exact_identity() {
        for token in [
            json!({"type":"file","file_id":"F1","text":"F1"}),
            json!({"type":"canvas","file_id":"F2","text":"Plan","section_id":"temp:C:abc0123456789abcdef012345678","is_skill_invocation":true}),
            json!({"type":"list_record","file_id":"F3","text":"Item","record_id":"Rec1","view_id":"View1","url":"https://fixture.slack.com/lists/F3"}),
        ] {
            let mut blocks = plain("");
            blocks[0]["elements"][0]["elements"][0] = token.clone();
            assert_eq!(fallback(&blocks).unwrap(), token["text"].as_str().unwrap());
            let mut changed = blocks.clone();
            changed[0]["elements"][0]["elements"][0]["file_id"] = json!("FOTHER");
            assert!(!confirmed(&blocks, &changed));
        }
    }
}
