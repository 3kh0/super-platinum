//! KhL1 section label projection over the separate rlfY rich-text source.
//! UTF-16 code units are authoritative: substring(0,25) can split a surrogate.
use super::{EmojiText, TitleDependencies, Unavailable, blocks, js_space, link};
use crate::{
    canvas::section_titles::Observation,
    slack::api::custom::texty::file_title_source::{ReferenceObservation, ReferenceValue},
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionVariant {
    Section,
    Missing,
    Private,
    Deleted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasSectionText {
    _english: (),
}
impl CanvasSectionText {
    /// Explicit application presentation, not an observation of Slack's locale.
    pub const fn english() -> Self {
        Self { _english: () }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanvasSectionLabel {
    units: Vec<u16>,
    preview: Vec<u16>,
    variant: SectionVariant,
}
impl CanvasSectionLabel {
    pub fn units(&self) -> &[u16] {
        &self.units
    }
    pub fn preview_units(&self) -> &[u16] {
        &self.preview
    }
    pub fn variant(&self) -> SectionVariant {
        self.variant
    }
    pub fn matches_units(&self, units: &[u16]) -> bool {
        self.units == units
    }
}

/// vHuS: clone the first top-level child, strip nested Canvas section IDs,
/// retain its opaque/style fields, and discard later top-level children only
/// when that first child has an elements property. No source mutation.
pub fn section_preview_block(section: &Value) -> Result<Value, Unavailable> {
    let children = blocks::elements(section, "rich_text")?;
    let Some(first) = children.first() else {
        return Ok(section.clone());
    };
    if first.get("elements").is_none() {
        return Ok(section.clone());
    }
    let leaves = first["elements"]
        .as_array()
        .ok_or(Unavailable::UnsupportedTitle)?;
    let leaves: Vec<_> = leaves
        .iter()
        .map(|leaf| {
            let mut leaf = leaf.clone();
            if leaf["type"] == "canvas" && leaf.get("section_id").is_some_and(truthy) {
                leaf.as_object_mut()
                    .expect("canvas object")
                    .remove("section_id");
            }
            leaf
        })
        .collect();
    let mut first = first.clone();
    first["elements"] = Value::Array(leaves);
    let mut section = section.clone();
    section["elements"] = Value::Array(vec![first]);
    Ok(section)
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}
/// Reuse the existing admitted title leaf grammar without whole-title Quip
/// override, Untitled fallback or leading-emoji trimming. Nested file/Canvas
/// leaves require their own source observation and are left to the caller.
pub fn section_leaf_label(
    leaf: &Value,
    emoji: Option<EmojiText>,
    mut user: impl FnMut(&str) -> Result<String, Unavailable>,
    mut channel: impl FnMut(&str) -> Result<String, Unavailable>,
    mut group: impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    Ok(match blocks::classify(leaf, emoji)? {
        blocks::Leaf::Text(text) => text.to_owned(),
        blocks::Leaf::User(id) => user(id)?,
        blocks::Leaf::Channel(id) => channel(id)?,
        blocks::Leaf::UserGroup(id) => group(id)?,
        blocks::Leaf::Emoji(text) => text,
    })
}
/// Dependencies of the actual first-child projection. File/Canvas IDs remain
/// separate source dependencies for the desktop's existing file path.
pub fn section_dependencies(
    section: &Value,
    emoji: Option<EmojiText>,
) -> Result<TitleDependencies, Unavailable> {
    let block = section_preview_block(section)?;
    let mut result = TitleDependencies::default();
    for child in blocks::elements(&block, "rich_text")? {
        let sections = if child["type"] == "rich_text_list" {
            child["elements"]
                .as_array()
                .ok_or(Unavailable::UnsupportedTitle)?
                .iter()
                .collect::<Vec<_>>()
        } else {
            vec![child]
        };
        for section in sections {
            for leaf in section["elements"]
                .as_array()
                .ok_or(Unavailable::UnsupportedTitle)?
            {
                if matches!(leaf["type"].as_str(), Some("canvas" | "file")) {
                    continue;
                }
                match blocks::classify(leaf, emoji)? {
                    blocks::Leaf::User(id) => {
                        result.users.insert(id.into());
                    }
                    blocks::Leaf::Channel(id) => {
                        result.channels.insert(id.into());
                    }
                    blocks::Leaf::UserGroup(id) => {
                        result.groups.insert(id.into());
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(result)
}
pub fn section_label(
    file: &ReferenceObservation,
    section: &Observation,
    canvas_name: &str,
    text: CanvasSectionText,
) -> Result<CanvasSectionLabel, Unavailable> {
    section_label_with_resolver(file, section, canvas_name, text, |_| {
        Err(Unavailable::UnsupportedTitle)
    })
}
/// Stateful leaves must use separately owned source observations, like whole
/// title resolution. The callback receives the vHuS-transformed leaf; it does
/// not gain navigation, insertion, or source authority from this call.
pub fn section_label_with_resolver(
    file: &ReferenceObservation,
    section: &Observation,
    canvas_name: &str,
    _text: CanvasSectionText,
    mut resolve: impl FnMut(&Value) -> Result<String, Unavailable>,
) -> Result<CanvasSectionLabel, Unavailable> {
    if file.id() != section.file_id() || canvas_name.len() > 1_048_576 {
        return Err(Unavailable::InvalidMetadata);
    }
    let status = match file.value() {
        ReferenceValue::PrivateCanvas => SectionVariant::Private,
        ReferenceValue::DeletedCanvas => SectionVariant::Deleted,
        ReferenceValue::Normal(_) => SectionVariant::Section,
    };
    format(canvas_name, section.block(), status, false, &mut resolve)
}
fn format(
    name: &str,
    section: Option<&Value>,
    status: SectionVariant,
    hide: bool,
    resolve: &mut impl FnMut(&Value) -> Result<String, Unavailable>,
) -> Result<CanvasSectionLabel, Unavailable> {
    let section = section.filter(|s| !s.is_null());
    let present = match section {
        None => false,
        Some(section) => !blocks::elements(section, "rich_text")?.is_empty(),
    };
    let mut preview: Vec<u16> = if present {
        let block = section_preview_block(section.unwrap())?;
        let value = extract(&block, resolve)?;
        value
            .trim_matches(js_space)
            .split('\n')
            .next()
            .unwrap_or("")
            .encode_utf16()
            .collect()
    } else {
        Vec::new()
    };
    if preview.len() > 25 {
        preview.truncate(25);
        preview.push(0x2026);
    }
    // KhL1 performs preview conversion even though a non-normal file status
    // ultimately owns the label. Preserve that independently observed preview.
    if matches!(status, SectionVariant::Private | SectionVariant::Deleted) {
        let label = if status == SectionVariant::Private {
            "Private canvas"
        } else {
            "Deleted canvas"
        };
        return Ok(CanvasSectionLabel {
            units: label.encode_utf16().collect(),
            preview,
            variant: status,
        });
    }
    // labelResolverParams retains the unsanitized preview; label sanitization
    // is after truncation, including a possible unpaired surrogate.
    let shown = if present {
        sanitize_units(&preview)
    } else {
        vec![0x2026]
    };
    let mut units = if hide {
        Vec::new()
    } else {
        let mut name = sanitize_units(&name.encode_utf16().collect::<Vec<_>>());
        name.extend([b':' as u16, b' ' as u16]);
        name
    };
    units.extend(shown);
    Ok(CanvasSectionLabel {
        units,
        preview,
        variant: if present {
            SectionVariant::Section
        } else {
            SectionVariant::Missing
        },
    })
}
fn extract(
    block: &Value,
    resolve: &mut impl FnMut(&Value) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    let children = blocks::elements(block, "rich_text")?;
    let mut runs = Vec::new();
    for child in children {
        if child["type"] == "rich_text_list" {
            // Executed vectors cover ordinary top-level ordered/bullet lists.
            // Nested alphabetic/Roman numbering has a separate contract.
            if !matches!(child.get("indent"), None | Some(Value::Null))
                && child["indent"].as_u64() != Some(0)
            {
                return Err(Unavailable::UnsupportedTitle);
            }
            let offset = match child.get("offset") {
                None | Some(Value::Null) => 0,
                Some(value) => value
                    .as_u64()
                    .filter(|v| *v <= 1_000_000)
                    .ok_or(Unavailable::UnsupportedTitle)?,
            };
            for (index, item) in child["elements"]
                .as_array()
                .ok_or(Unavailable::UnsupportedTitle)?
                .iter()
                .enumerate()
            {
                let body = extract(
                    &serde_json::json!({"type":"rich_text","elements":[item]}),
                    resolve,
                )?;
                let prefix = match child["style"].as_str() {
                    Some("ordered") => format!("{}.", offset + index as u64 + 1),
                    Some("bullet") => "\u{2022}".into(),
                    _ => return Err(Unavailable::UnsupportedTitle),
                };
                runs.push(format!("{prefix} {body}."));
            }
            continue;
        }
        if !matches!(
            child["type"].as_str(),
            Some("rich_text_section" | "rich_text_quote" | "rich_text_preformatted")
        ) {
            return Err(Unavailable::UnsupportedTitle);
        }
        let leaves = child["elements"]
            .as_array()
            .ok_or(Unavailable::UnsupportedTitle)?;
        let mut value = String::new();
        for leaf in leaves {
            match leaf["type"].as_str() {
                Some("text") => {
                    value.push_str(leaf["text"].as_str().ok_or(Unavailable::UnsupportedTitle)?)
                }
                Some("link") => value.push_str(link::text(leaf)?),
                _ => value.push_str(&resolve(leaf)?),
            }
            if value.len() > 1_048_576 {
                return Err(Unavailable::UnsupportedTitle);
            }
        }
        runs.push(value);
    }
    Ok(runs.join("\n"))
}
fn sanitize_units(units: &[u16]) -> Vec<u16> {
    let normalized: Vec<_> = units
        .iter()
        .map(|u| {
            if matches!(*u, 0x2028 | 0x2029) {
                10
            } else {
                *u
            }
        })
        .collect();
    let mut output = Vec::new();
    let mut start = 0;
    for end in 0..=normalized.len() {
        if end < normalized.len() && !matches!(normalized[end], 10 | 13) {
            continue;
        }
        let mut line = &normalized[start..end];
        while line.first().is_some_and(|u| matches!(*u, 0x200b | 0xfeff)) {
            line = &line[1..];
        }
        while line.last().is_some_and(|u| matches!(*u, 0x200b | 0xfeff)) {
            line = &line[..line.len() - 1];
        }
        output.extend(
            line.iter()
                .filter(|u| **u != 0xad)
                .map(|u| if *u == 0xa0 { 32 } else { *u }),
        );
        if end < normalized.len() {
            output.push(normalized[end]);
        }
        start = end + 1;
    }
    output
}

#[cfg(test)]
mod tests;
