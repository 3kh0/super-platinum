//! Inert argument conversion port. No renderer/admission/store/transport hooks.
//! Input is a captured Delta, NOT textarea markup or a protected Review proof.
//! Retain the entire input and immutable emoji table; unknown content is refused.
use serde_json::{Map, Value, json};
use std::sync::Arc;
mod blocks;
mod chunks;
#[cfg(test)]
mod tests;
mod text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    Shape,
    NotCommand,
    Content,
    Emoji,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Op {
    insert: Value,
    attrs: Map<String, Value>,
}
pub(crate) struct Converted {
    original: Value,
    emoji: Arc<Value>,
    display: String,
    blocks: Option<Value>,
}
impl Converted {
    pub(crate) fn original(&self) -> &Value {
        &self.original
    }
    pub(crate) fn blocks(&self) -> Option<&Value> {
        self.blocks.as_ref()
    }
    pub(crate) fn display(&self) -> &str {
        &self.display
    }
    pub(crate) fn same_emoji_catalog(&self, table: &Arc<Value>) -> bool {
        Arc::ptr_eq(&self.emoji, table)
    }
}

fn space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|
        '\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn exact(value: &Value, required: &[&str], optional: &[&str]) -> bool {
    value.as_object().is_some_and(|v| {
        required.iter().all(|key| v.contains_key(*key))
            && v.keys()
                .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str()))
    })
}
fn parse(value: &Value) -> Result<Vec<Op>, Unsupported> {
    let rows = value.as_array().ok_or(Unsupported::Shape)?;
    if rows.is_empty() || rows.len() > 100 || value.to_string().len() > 256 * 1024 {
        return Err(Unsupported::Shape);
    }
    let mut ops = Vec::new();
    for row in rows {
        if !exact(row, &["insert"], &["attributes"]) {
            return Err(Unsupported::Shape);
        }
        let attrs = match row.get("attributes") {
            None => Map::new(),
            Some(v) => v.as_object().cloned().ok_or(Unsupported::Shape)?,
        };
        for (key, value) in &attrs {
            match key.as_str() {
                "bold" | "italic" | "underline" | "strike" | "code" | "unlink"
                    if value.is_boolean() => {}
                "list"
                    if value.as_str().is_some_and(|v| {
                        matches!(v, "bullet" | "ordered" | "checked" | "unchecked")
                    }) => {}
                "blockquote" | "code-block" if value == true => {}
                "indent" | "border" if value.as_u64().is_some_and(|v| v <= 8) => {}
                "header" if value.as_u64().is_some_and(|v| (1..=3).contains(&v)) => {}
                "link"
                    if value
                        .as_str()
                        .is_some_and(|v| !v.is_empty() && v.len() <= 8192) => {}
                "slackmention" if exact(value, &["id", "label"], &["tabId"]) => {}
                _ => return Err(Unsupported::Content),
            }
        }
        let insert = row["insert"].clone();
        match &insert {
            Value::String(s) if !s.is_empty() && s.encode_utf16().count() <= 131072 => {}
            Value::Object(_)
                if exact(&insert, &["slackemoji"], &[])
                    && exact(&insert["slackemoji"], &["text"], &[])
                    && insert["slackemoji"]["text"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty() && s.len() <= 256) => {}
            _ => return Err(Unsupported::Content),
        }
        // Conflicting structural attributes or line attributes are not erased.
        if attrs.contains_key("slackmention") && attrs.contains_key("link")
            || !insert.is_string()
                && (attrs.contains_key("slackmention") || attrs.contains_key("link"))
            || insert.as_str().is_some_and(|s| s.contains('\n'))
                && (attrs.contains_key("slackmention") || attrs.contains_key("link"))
        {
            return Err(Unsupported::Content);
        }
        let block_keys = [
            "list",
            "blockquote",
            "code-block",
            "header",
            "indent",
            "border",
        ];
        let structural = ["list", "blockquote", "code-block", "header"];
        if block_keys.iter().any(|key| attrs.contains_key(*key)) {
            if insert != Value::String("\n".into())
                || structural
                    .iter()
                    .filter(|key| attrs.contains_key(**key))
                    .count()
                    != 1
                || attrs.keys().any(|key| !block_keys.contains(&key.as_str()))
                || attrs.contains_key("indent") && !attrs.contains_key("list")
                || attrs.contains_key("border") && attrs.contains_key("header")
            {
                return Err(Unsupported::Content);
            }
        }
        ops.push(Op { insert, attrs });
    }
    Ok(ops)
}
fn push(ops: &mut Vec<Op>, op: Op) {
    if let Some(last) = ops.last_mut() {
        if last.attrs == op.attrs {
            if let (Some(a), Some(b)) = (last.insert.as_str(), op.insert.as_str()) {
                last.insert = Value::String(format!("{a}{b}"));
                return;
            }
        }
    }
    ops.push(op);
}
fn trim_start(ops: &mut Vec<Op>) {
    while let Some(first) = ops.first_mut() {
        let Some(value) = first.insert.as_str() else {
            break;
        };
        let trimmed = value.trim_start_matches(space).to_owned();
        if trimmed.is_empty() {
            ops.remove(0);
        } else {
            first.insert = trimmed.into();
            break;
        }
    }
}

/// Convert supported captured Delta ops through the observed command-removal and
/// rich-section path. This accepts real arguments, styles, mentions, links and
/// emoji; unsupported blocks/fields return an error without mutation or fallback.
pub(crate) fn convert(input: &Value, emoji: Arc<Value>) -> Result<Converted, Unsupported> {
    if !emoji.is_object() {
        return Err(Unsupported::Emoji);
    }
    let mut ops = parse(input)?;
    // s4/N qualifies the first actual line, including its newline attributes.
    if ops
        .iter()
        .find(|op| op.insert.as_str().is_some_and(|s| s.contains('\n')))
        .is_some_and(|op| !op.attrs.is_empty())
    {
        return Err(Unsupported::NotCommand);
    }
    let first = ops[0].insert.as_str().ok_or(Unsupported::NotCommand)?;
    if ops[0].attrs.get("code") == Some(&Value::Bool(true))
        || ops[0].attrs.contains_key("slackmention")
        || ops[0].attrs.contains_key("link")
    {
        return Err(Unsupported::NotCommand);
    }
    let end = first.find(space).unwrap_or(first.len());
    let display = first[..end].to_owned();
    if !super::catalog::name(&display) {
        return Err(Unsupported::NotCommand);
    }
    let remaining = first[end..].to_owned();
    if remaining.is_empty() {
        ops.remove(0);
    } else {
        ops[0].insert = remaining.into();
    }
    let mut normalized = Vec::new();
    for op in ops {
        push(&mut normalized, op);
    }
    trim_start(&mut normalized);
    let blocks = blocks::render(normalized, &emoji)?;
    if blocks
        .as_ref()
        .is_some_and(|v| v.to_string().len() > 128 * 1024)
    {
        return Err(Unsupported::Shape);
    }
    Ok(Converted {
        original: input.clone(),
        emoji,
        display,
        blocks,
    })
}

/// Pure canonical full-Delta conversion for explicit rich-mode correspondence.
/// Never use this as protected admission or reinterpret a legacy textarea.
pub(crate) fn full_blocks(input: &Value, emoji: &Arc<Value>) -> Result<Option<Value>, Unsupported> {
    if !emoji.is_object() {
        return Err(Unsupported::Emoji);
    }
    let mut normalized = Vec::new();
    for op in parse(input)? {
        push(&mut normalized, op);
    }
    trim_start(&mut normalized);
    blocks::render(normalized, emoji)
}
