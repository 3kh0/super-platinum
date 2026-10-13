//! A deliberately small, reversible projection for ordinary text composers.
//!
//! Accepted snapshots have one top-level destination, no files, and exactly one
//! rich_text / rich_text_section containing canonical text runs. Decode succeeds
//! only if encoding the projected text reproduces the complete Snapshot exactly
//! (JSON object ordering excepted). Unknown fields are rejected, not discarded.
//! The original raw snapshot is retained separately and never rewritten on read.
//!
//! Supported local syntax follows ComposerState's nonnested *bold*, _italic_,
//! ~strike~ and `code` delimiters. Styling may differ between runs, but combined
//! styles, nested markup, quotes, fences, entities and shortcode emoji are not
//! projected. Literal unmatched delimiters remain literal when unambiguous.
//! This is a supported subset, not a general Slack markdown parser or a promise
//! about server normalization. Empty text is content here, not a delete command.
use super::{DraftKey, sync::Snapshot};
use crate::ComposerState;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Unsupported {
    #[error("only one top-level conversation is supported")]
    Destination,
    #[error("file attachments require a different composer")]
    Files,
    #[error("this rich content or its fields are not supported")]
    Shape,
    #[error("this formatting is not supported without changing its meaning")]
    Formatting,
    #[error("Slack entities and shortcode emoji require a richer composer")]
    Entity,
    #[error("the content cannot round-trip exactly through this composer")]
    NotReversible,
}

#[derive(Clone, Debug)]
pub struct Projection {
    key: DraftKey,
    composer: ComposerState,
    raw: Snapshot,
}
impl Projection {
    pub fn composer(&self) -> &ComposerState {
        &self.composer
    }
    pub fn raw_snapshot(&self) -> &Snapshot {
        &self.raw
    }
    /// Selection changes do not change Slack content. An unchanged projection
    /// returns its original snapshot; edited text must pass the same strict codec.
    pub fn snapshot_for(&self, composer: &ComposerState) -> Result<Snapshot, Unsupported> {
        if composer.text == self.composer.text {
            Ok(self.raw.clone())
        } else {
            encode(&self.key, &composer.text)
        }
    }
}

fn destination(key: &DraftKey) -> Result<Value, Unsupported> {
    if key.thread.is_some()
        || key.team.trim().is_empty()
        || key.channel.trim().is_empty()
        || key.team.trim() != key.team
        || key.channel.trim() != key.channel
    {
        return Err(Unsupported::Destination);
    }
    Ok(json!({"channel_id":key.channel}))
}
fn exact_keys(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
    })
}
fn unsupported_entities(text: &str) -> bool {
    text.find('<')
        .is_some_and(|start| text[start..].contains('>'))
        || !crate::state::emoji_names_in_text(text).is_empty()
}
fn marker_style(marker: u8) -> &'static str {
    match marker {
        b'*' => "bold",
        b'_' => "italic",
        b'~' => "strike",
        _ => "code",
    }
}
fn append_plain(runs: &mut Vec<Value>, text: &str) -> Result<(), Unsupported> {
    if text.is_empty() {
        return Ok(());
    }
    if unsupported_entities(text) {
        return Err(Unsupported::Entity);
    }
    if let Some(previous) = runs.last_mut().filter(|v| v.get("style").is_none()) {
        let merged = format!("{}{text}", previous["text"].as_str().unwrap());
        previous["text"] = merged.into();
    } else {
        runs.push(json!({"type":"text","text":text}));
    }
    Ok(())
}
fn line(runs: &mut Vec<Value>, text: &str) -> Result<(), Unsupported> {
    if text.starts_with('>') || text.contains("```") {
        return Err(Unsupported::Formatting);
    }
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let marker = bytes[index];
        if !matches!(marker, b'*' | b'_' | b'~' | b'`') {
            index += 1;
            continue;
        }
        if index > 0 && bytes[index - 1] == b'\\' {
            return Err(Unsupported::Formatting);
        }
        let rest = &text[index + 1..];
        if rest.starts_with(marker as char) {
            return Err(Unsupported::Formatting);
        }
        let close = rest.find(marker as char);
        let can_open = if marker == b'`' {
            close.is_some()
        } else {
            rest.chars().next().is_some_and(|c| !c.is_whitespace())
                && rest.char_indices().skip(1).any(|(i, c)| {
                    c == marker as char
                        && rest[..i]
                            .chars()
                            .next_back()
                            .is_some_and(|c| !c.is_whitespace())
                })
        };
        if !can_open {
            index += 1;
            continue;
        }
        let content = &rest[..close.ok_or(Unsupported::Formatting)?];
        if content.is_empty()
            || (marker != b'`'
                && (content.ends_with(char::is_whitespace)
                    || content
                        .bytes()
                        .any(|c| matches!(c, b'*' | b'_' | b'~' | b'`'))))
        {
            return Err(Unsupported::Formatting);
        }
        if marker != b'`' && unsupported_entities(content) {
            return Err(Unsupported::Entity);
        }
        append_plain(runs, &text[start..index])?;
        let mut style = serde_json::Map::new();
        style.insert(marker_style(marker).into(), true.into());
        runs.push(json!({"type":"text","text":content,"style":style}));
        index += content.len() + 2;
        start = index;
    }
    append_plain(runs, &text[start..])
}

/// Encode only explicitly supported local syntax. The caller remains responsible
/// for enrollment/clear/send intent; this function performs no mutation or I/O.
pub fn encode(key: &DraftKey, text: &str) -> Result<Snapshot, Unsupported> {
    let destination = destination(key)?;
    // Do not silently normalize platform line endings or embedded controls.
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(Unsupported::Formatting);
    }
    let mut runs = Vec::new();
    for (index, part) in text.split('\n').enumerate() {
        if index > 0 {
            append_plain(&mut runs, "\n")?;
        }
        line(&mut runs, part)?;
    }
    Ok(Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":runs}]}),
        ],
        file_ids: vec![],
        destinations: vec![destination],
    })
}

/// Reject anything whose raw representation cannot be reproduced exactly. Even
/// semantically redundant fields/partitions are kept opaque rather than dropped.
pub fn decode(key: &DraftKey, snapshot: &Snapshot) -> Result<Projection, Unsupported> {
    if snapshot.destinations != vec![destination(key)?] {
        return Err(Unsupported::Destination);
    }
    if !snapshot.file_ids.is_empty() {
        return Err(Unsupported::Files);
    }
    if snapshot.blocks.len() != 1 {
        return Err(Unsupported::Shape);
    }
    let block = &snapshot.blocks[0];
    if !exact_keys(block, &["type", "elements"]) || block["type"] != "rich_text" {
        return Err(Unsupported::Shape);
    }
    let sections = block["elements"].as_array().ok_or(Unsupported::Shape)?;
    if sections.len() != 1 {
        return Err(Unsupported::Shape);
    }
    let section = &sections[0];
    if !exact_keys(section, &["type", "elements"]) || section["type"] != "rich_text_section" {
        return Err(Unsupported::Shape);
    }
    let mut text = String::new();
    for run in section["elements"].as_array().ok_or(Unsupported::Shape)? {
        if run["type"] != "text" {
            return Err(Unsupported::Shape);
        }
        let content = run["text"].as_str().ok_or(Unsupported::Shape)?;
        if run.get("style").is_none() {
            if !exact_keys(run, &["type", "text"]) {
                return Err(Unsupported::Shape);
            }
            text.push_str(content);
        } else {
            if !exact_keys(run, &["type", "text", "style"]) {
                return Err(Unsupported::Shape);
            }
            let style = run["style"]
                .as_object()
                .filter(|style| style.len() == 1)
                .ok_or(Unsupported::Formatting)?;
            let (name, value) = style.iter().next().unwrap();
            if value != true || content.is_empty() || content.contains('\n') {
                return Err(Unsupported::Formatting);
            }
            let marker = match name.as_str() {
                "bold" => '*',
                "italic" => '_',
                "strike" => '~',
                "code" => '`',
                _ => return Err(Unsupported::Formatting),
            };
            text.push(marker);
            text.push_str(content);
            text.push(marker);
        }
    }
    if encode(key, &text)? != *snapshot {
        return Err(Unsupported::NotReversible);
    }
    Ok(Projection {
        key: key.clone(),
        composer: ComposerState::with_text(text),
        raw: snapshot.clone(),
    })
}

#[cfg(test)]
mod tests;
