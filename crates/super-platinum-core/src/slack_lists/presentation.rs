//! Read-only cell content from captured iLfW/FieldRenderer/HSOl consumers.
//! Record and column raw JSON remain authoritative; this projection never edits it.
use super::{CellValue, Column, Record};
use serde_json::{Number, Value};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    User,
    Channel,
    Attachment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectLabel {
    pub id: String,
    pub label: String,
    /// Captured metadata, not a trusted CSS value.
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellPresentation {
    Empty,
    Checkbox(bool),
    Number(Number),
    Text(String),
    RichText {
        blocks: Vec<Value>,
        fallback: String,
    },
    Select(Vec<SelectLabel>),
    References {
        kind: ReferenceKind,
        ids: Vec<String>,
        fallback: String,
    },
    /// Untrusted destination. The renderer must apply its normal URL policy.
    Link {
        url: String,
        label: String,
    },
    Unsupported {
        fallback: String,
    },
}

impl CellPresentation {
    pub fn plain_text(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Checkbox(value) => if *value { "Checked" } else { "Unchecked" }.into(),
            Self::Number(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::RichText { fallback, .. }
            | Self::References { fallback, .. }
            | Self::Unsupported { fallback } => fallback.clone(),
            Self::Select(options) => options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            Self::Link { label, .. } => label.clone(),
        }
    }
}

impl Column {
    pub fn presentation(&self, record: &Record) -> CellPresentation {
        let fallback = || record.fallback(&self.key).filter(|text| !text.is_empty());
        match self.cell(record) {
            CellValue::Empty => CellPresentation::Empty,
            CellValue::Boolean(value) => CellPresentation::Checkbox(value),
            CellValue::Number(value) => CellPresentation::Number(value),
            CellValue::Text(value) => CellPresentation::Text(value),
            CellValue::RichText(blocks) => {
                let text = fallback()
                    .map(str::to_owned)
                    .unwrap_or_else(|| rich_text_fallback(&blocks));
                CellPresentation::RichText {
                    blocks,
                    fallback: text,
                }
            }
            CellValue::Values(ids) => match self.kind.as_str() {
                "select" | "multi_select" => CellPresentation::Select(select_labels(self, ids)),
                "user" | "assignee" | "todo_assignee" | "vote" | "channel" | "attachment"
                | "canvas" => {
                    let kind = match self.kind.as_str() {
                        "channel" => ReferenceKind::Channel,
                        "attachment" | "canvas" => ReferenceKind::Attachment,
                        _ => ReferenceKind::User,
                    };
                    let text = fallback()
                        .map(str::to_owned)
                        .unwrap_or_else(|| ids.join(", "));
                    CellPresentation::References {
                        kind,
                        ids,
                        fallback: text,
                    }
                }
                _ => CellPresentation::Text(ids.join(", ")),
            },
            CellValue::Link(value) => {
                let Some(url) = value
                    .get("original_url")
                    .and_then(Value::as_str)
                    .filter(|url| !url.is_empty())
                else {
                    return CellPresentation::Unsupported {
                        fallback: fallback().unwrap_or("Unsupported link").into(),
                    };
                };
                // Captured LinkField: displayAsUrl overrides displayName.
                let label = if value["display_as_url"] == true {
                    url
                } else {
                    value
                        .get("display_name")
                        .and_then(Value::as_str)
                        .unwrap_or(url)
                };
                CellPresentation::Link {
                    url: url.into(),
                    label: label.into(),
                }
            }
            CellValue::Unsupported(_) => CellPresentation::Unsupported {
                fallback: fallback().unwrap_or("Unsupported field").into(),
            },
        }
    }
}

fn select_labels(column: &Column, ids: Vec<String>) -> Vec<SelectLabel> {
    // Captured FieldRenderer uses schema choices order for multi-selects. Do not
    // invent a label for unknown/ambiguous IDs; retain the ID as a visible fallback.
    let choices = column
        .raw
        .pointer("/options/choices")
        .and_then(Value::as_array);
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    if let Some(choices) = choices {
        let mut counts = HashMap::new();
        for id in choices
            .iter()
            .filter_map(|choice| choice.get("value").and_then(Value::as_str))
        {
            *counts.entry(id).or_insert(0usize) += 1;
        }
        for choice in choices {
            let Some(id) = choice.get("value").and_then(Value::as_str) else {
                continue;
            };
            if !wanted.contains(id) || seen.contains(id) || counts.get(id) != Some(&1) {
                continue;
            }
            let Some(label) = choice.get("label").and_then(Value::as_str) else {
                continue;
            };
            seen.insert(id.to_owned());
            output.push(SelectLabel {
                id: id.into(),
                label: label.into(),
                color: choice
                    .get("color")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });
        }
    }
    for id in ids {
        if seen.insert(id.clone()) {
            output.push(SelectLabel {
                label: id.clone(),
                id,
                color: None,
            });
        }
    }
    output
}

fn rich_text_fallback(blocks: &[Value]) -> String {
    fn append(value: &Value, output: &mut String, depth: usize) {
        if depth > 32 {
            return;
        }
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "text" => output.push_str(
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            "link" => output.push_str(
                value
                    .get("text")
                    .or_else(|| value.get("url"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            "emoji" => {
                if let Some(name) = value.get("name").and_then(Value::as_str) {
                    output.push(':');
                    output.push_str(name);
                    output.push(':');
                }
            }
            "user" | "channel" | "usergroup" => {
                let (field, prefix) = match kind {
                    "user" => ("user_id", "@"),
                    "channel" => ("channel_id", "#"),
                    _ => ("usergroup_id", "@"),
                };
                if let Some(id) = value.get(field).and_then(Value::as_str) {
                    output.push_str(prefix);
                    output.push_str(id);
                }
            }
            "rich_text"
            | "rich_text_section"
            | "rich_text_list"
            | "rich_text_quote"
            | "rich_text_preformatted" => {
                if let Some(elements) = value.get("elements").and_then(Value::as_array) {
                    for element in elements {
                        append(element, output, depth + 1);
                    }
                }
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
            _ => {}
        }
    }
    let mut output = String::new();
    for block in blocks {
        append(block, &mut output, 0);
    }
    output.trim_end().to_owned()
}

#[cfg(test)]
mod tests;
