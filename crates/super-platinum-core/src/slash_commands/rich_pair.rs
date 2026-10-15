//! Pure explicit-rich content correspondence, not a protected Review/Claim.
//! No legacy LocalDraft or textarea is reinterpreted. Raw desired is retained.
use crate::drafts::{DraftKey, codec, sync::Snapshot};
use serde_json::{Map, Value, json};
use std::sync::Arc;

pub(crate) struct RichPair {
    key: DraftKey,
    desired: Snapshot,
    delta: Value,
    arguments: super::arguments::Converted,
}
impl RichPair {
    pub(crate) fn desired(&self) -> &Snapshot {
        &self.desired
    }
    pub(crate) fn delta(&self) -> &Value {
        &self.delta
    }
    pub(crate) fn arguments(&self) -> &super::arguments::Converted {
        &self.arguments
    }
    pub(crate) fn matches(&self, key: &DraftKey, desired: &Snapshot, emoji: &Arc<Value>) -> bool {
        self.key == *key && self.desired == *desired && self.arguments.same_emoji_catalog(emoji)
    }
}
fn exact(v: &Value, required: &[&str], optional: &[&str]) -> bool {
    v.as_object().is_some_and(|o| {
        required.iter().all(|k| o.contains_key(*k))
            && o.keys()
                .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str()))
    })
}
fn text(v: &Value, key: &str) -> Result<String, super::Failure> {
    v[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or(super::Failure::Unsupported)
}
fn inline(v: &Value) -> Result<Value, super::Failure> {
    let mut attrs = Map::new();
    if let Some(style) = v.get("style") {
        attrs = style
            .as_object()
            .cloned()
            .ok_or(super::Failure::Unsupported)?;
    }
    let insert = match v["type"].as_str() {
        Some("text") if exact(v, &["type", "text"], &["style"]) => Value::String(text(v, "text")?),
        Some("link") if exact(v, &["type", "url"], &["text", "style"]) => {
            let url = text(v, "url")?;
            attrs.insert("link".into(), url.clone().into());
            Value::String(if v.get("text").is_some() {
                text(v, "text")?
            } else {
                url
            })
        }
        Some("user" | "usergroup" | "channel" | "broadcast") => {
            let (key, prefix) = match v["type"].as_str().unwrap() {
                "user" => ("user_id", ""),
                "usergroup" => ("usergroup_id", ""),
                "channel" => ("channel_id", ""),
                _ => ("range", "BK"),
            };
            if !exact(v, &["type", key], &["style", "tab_id"]) {
                return Err(super::Failure::Unsupported);
            }
            let id = format!("{prefix}{}", text(v, key)?);
            // Internal canonical label only; never used as native visible text.
            let mut mention = json!({"id":id,"label":id});
            if let Some(tab) = v.get("tab_id") {
                mention["tabId"] = tab.clone();
            }
            attrs.insert("slackmention".into(), mention);
            Value::String(id)
        }
        Some("emoji") if exact(v, &["type", "name"], &["unicode", "skin_tone", "style"]) => {
            let name = text(v, "name")?;
            let suffix = if let Some(tone) = v.get("skin_tone") {
                let tone = tone
                    .as_u64()
                    .filter(|n| (2..=6).contains(n))
                    .ok_or(super::Failure::Unsupported)?;
                format!("::skin-tone-{tone}:")
            } else {
                ":".into()
            };
            json!({"slackemoji":{"text":format!(":{name}{suffix}")}})
        }
        Some("color") if exact(v, &["type", "value"], &["style"]) => {
            Value::String(text(v, "value")?)
        }
        _ => return Err(super::Failure::Unsupported),
    };
    let mut op = json!({"insert":insert});
    if !attrs.is_empty() {
        op["attributes"] = attrs.into();
    }
    Ok(op)
}
fn leaves(
    v: &Value,
    line: Option<&Map<String, Value>>,
    out: &mut Vec<Value>,
) -> Result<(), super::Failure> {
    let leaves = v["elements"]
        .as_array()
        .ok_or(super::Failure::Unsupported)?;
    for leaf in leaves {
        let op = inline(leaf)?;
        if let Some(attrs) = line {
            if let Some(value) = op["insert"].as_str() {
                let mut offset = 0;
                for (index, _) in value.match_indices('\n') {
                    if index > offset {
                        let mut piece = op.clone();
                        piece["insert"] = value[offset..index].into();
                        out.push(piece);
                    }
                    out.push(json!({"insert":"\n","attributes":attrs}));
                    offset = index + 1;
                }
                if offset < value.len() {
                    let mut piece = op.clone();
                    piece["insert"] = value[offset..].into();
                    out.push(piece);
                }
            } else {
                out.push(op);
            }
        } else {
            out.push(op);
        }
    }
    Ok(())
}
fn newline(out: &mut Vec<Value>, attrs: &Map<String, Value>) {
    if attrs.is_empty()
        && out
            .last()
            .and_then(|v| v["insert"].as_str())
            .is_some_and(|s| s.ends_with('\n'))
    {
        return;
    }
    let mut value = json!({"insert":"\n"});
    if !attrs.is_empty() {
        value["attributes"] = attrs.clone().into();
    }
    out.push(value);
}
fn elements(block: &Value) -> Result<Value, super::Failure> {
    if !exact(block, &["type", "elements"], &[]) || block["type"] != "rich_text" {
        return Err(super::Failure::Unsupported);
    }
    let mut out = Vec::new();
    for item in block["elements"]
        .as_array()
        .ok_or(super::Failure::Unsupported)?
    {
        let mut attrs = Map::new();
        match item["type"].as_str() {
            Some("rich_text_section") if exact(item, &["type", "elements"], &[]) => {
                leaves(item, None, &mut out)?;
                newline(&mut out, &attrs);
            }
            Some("rich_text_list")
                if exact(
                    item,
                    &["type", "style", "indent", "border", "elements"],
                    &["offset"],
                ) =>
            {
                for key in ["style", "indent", "border"] {
                    attrs.insert(
                        if key == "style" {
                            "list".into()
                        } else {
                            key.into()
                        },
                        item[key].clone(),
                    );
                }
                for row in item["elements"]
                    .as_array()
                    .ok_or(super::Failure::Unsupported)?
                {
                    if !exact(row, &["type", "elements"], &[]) || row["type"] != "rich_text_section"
                    {
                        return Err(super::Failure::Unsupported);
                    }
                    let start = out.len();
                    leaves(row, None, &mut out)?;
                    if out[start..]
                        .iter()
                        .any(|op| op["insert"].as_str().is_some_and(|s| s.contains('\n')))
                    {
                        return Err(super::Failure::Unsupported);
                    }
                    newline(&mut out, &attrs);
                }
            }
            Some("rich_text_quote" | "rich_text_preformatted" | "rich_text_header") => {
                let ty = item["type"].as_str().unwrap();
                if !exact(
                    item,
                    &["type", "elements"],
                    if ty == "rich_text_header" {
                        &["level"]
                    } else {
                        &["border"]
                    },
                ) {
                    return Err(super::Failure::Unsupported);
                }
                match ty {
                    "rich_text_quote" => {
                        attrs.insert("blockquote".into(), true.into());
                    }
                    "rich_text_preformatted" => {
                        attrs.insert("code-block".into(), true.into());
                    }
                    _ => {
                        attrs.insert("header".into(), item["level"].clone());
                    }
                }
                if let Some(border) = item.get("border") {
                    attrs.insert("border".into(), border.clone());
                }
                leaves(item, Some(&attrs), &mut out)?;
                newline(&mut out, &attrs);
            }
            _ => return Err(super::Failure::Unsupported),
        }
    }
    Ok(out.into())
}
pub(crate) fn derive(
    key: &DraftKey,
    desired: &Snapshot,
    emoji: Arc<Value>,
) -> Result<RichPair, super::Failure> {
    let mut destination = codec::encode(key, "")
        .map_err(|_| super::Failure::Unsupported)?
        .destinations;
    if key.thread.is_some()
        && desired
            .destinations
            .first()
            .and_then(|v| v.get("broadcast"))
            == Some(&Value::Bool(false))
    {
        destination[0]["broadcast"] = false.into();
    }
    if !desired.file_ids.is_empty()
        || desired.destinations != destination
        || desired.blocks.len() != 1
        || serde_json::to_vec(desired)
            .map_err(|_| super::Failure::Unsupported)?
            .len()
            > 256 * 1024
    {
        return Err(super::Failure::Unsupported);
    }
    let delta = elements(&desired.blocks[0])?;
    let complete =
        super::arguments::full_blocks(&delta, &emoji).map_err(|_| super::Failure::Unsupported)?;
    if complete != Some(Value::Array(desired.blocks.clone())) {
        return Err(super::Failure::Unsupported);
    }
    let arguments =
        super::arguments::convert(&delta, emoji).map_err(|_| super::Failure::Unsupported)?;
    Ok(RichPair {
        key: key.clone(),
        desired: desired.clone(),
        delta,
        arguments,
    })
}

#[cfg(test)]
#[path = "rich_pair_tests.rs"]
mod tests;
