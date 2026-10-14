//! Lists content only: no message owner, app actions, embeds, or hydration.
use crate::{
    blocks::{BlockCtx, block_nodes},
    media::MediaRegistry,
    model::RichNode,
};
use serde_json::Value;
use super_platinum_core::{
    slack_lists::{CellPresentation, Column, Record, ReferenceKind},
    state::Workspace,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CellContent {
    Plain,
    Timestamp { tooltip: String },
    Rich(Vec<RichNode>),
    Labels(Vec<String>),
}

pub(super) fn project(
    workspace: Option<&Workspace>,
    media: &MediaRegistry,
    column: &Column,
    record: &Record,
) -> (String, CellContent) {
    // Bound borrowed rich input before Column::presentation/cell can clone it.
    if matches!(column.kind.as_str(), "text" | "rich_text") {
        let field = record.fields.get(&column.key);
        let raw = field.and_then(|field| field.get("rich_text").or_else(|| field.get("richText")));
        let fallback = record.fallback(&column.key).filter(|text| !text.is_empty());
        if let Some(blocks) = raw.and_then(Value::as_array) {
            return rich_project(workspace, media, blocks, fallback);
        }
        // Malformed rich values and unknown payload members stay inert without
        // cloning their arbitrarily nested raw JSON either.
        if fallback.is_some_and(|text| text.len() > MAX_RICH_BYTES) {
            return (TOO_LARGE.into(), CellContent::Plain);
        }
        let unsupported = raw.is_some_and(|value| !value.is_null())
            || field.and_then(Value::as_object).is_some_and(|field| {
                field.iter().any(|(key, value)| {
                    !matches!(key.as_str(), "key" | "column_id" | "row_id" | "text")
                        && !value.is_null()
                })
            });
        return (
            fallback
                .unwrap_or(if unsupported { "Unsupported field" } else { "" })
                .into(),
            CellContent::Plain,
        );
    }
    let presentation = column.presentation(record);
    let fallback = presentation.plain_text();
    match presentation {
        CellPresentation::Number(value) => (
            super_platinum_core::slack_lists::numbers::format(
                &value,
                column.raw.get("options"),
                workspace.and_then(|workspace| workspace.locale.as_deref()),
            ),
            CellContent::Plain,
        ),
        CellPresentation::Select(options) => (
            fallback,
            CellContent::Labels(options.into_iter().map(|option| option.label).collect()),
        ),
        CellPresentation::Link { url, label } => {
            let node = link(label, url);
            (node.plain_text(), CellContent::Rich(vec![node]))
        }
        CellPresentation::RichText { .. } => {
            unreachable!("rich fields use bounded borrowed projection")
        }
        CellPresentation::References { kind, ids, .. } => {
            let Some(workspace) = workspace else {
                return (fallback, CellContent::Plain);
            };
            // The Workspace has user/channel indexes, but no file index. Keep
            // attachment display fallback rather than scan all messages per cell.
            if kind == ReferenceKind::Attachment {
                return (fallback, CellContent::Plain);
            }
            let ctx = BlockCtx::new(workspace, media);
            let mut nodes = Vec::new();
            for id in ids {
                if !nodes.is_empty() {
                    nodes.push(RichNode::Text(", ".into()));
                }
                let raw = match kind {
                    ReferenceKind::User if workspace.users.contains_key(&id) => {
                        Some(serde_json::json!({"type":"user","user_id":id}))
                    }
                    ReferenceKind::Channel if workspace.channels.contains_key(&id) => {
                        Some(serde_json::json!({"type":"channel","channel_id":id}))
                    }
                    _ => None,
                };
                if let Some(raw) = raw {
                    nodes.extend(block_nodes(ctx, &raw));
                } else {
                    nodes.push(RichNode::Text(id));
                }
            }
            let text = nodes
                .iter()
                .map(|node| match node {
                    RichNode::Text(text) => text.clone(),
                    other => other.plain_text(),
                })
                .collect::<String>();
            (text, CellContent::Rich(nodes))
        }
        _ => (fallback, CellContent::Plain),
    }
}

fn is_container(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some(
            "rich_text"
                | "rich_text_section"
                | "rich_text_list"
                | "rich_text_quote"
                | "rich_text_preformatted"
        )
    )
}

const MAX_RICH_NODES: usize = 2048;
const MAX_RICH_BYTES: usize = 64 * 1024;
const MAX_RICH_DEPTH: usize = 32;
const TOO_LARGE: &str = "This cell is too large to display in full.";

#[derive(Default)]
struct Budget {
    nodes: usize,
    bytes: usize,
    incomplete: bool,
}
impl Budget {
    fn string(&mut self, text: &str) -> Option<Value> {
        if text.len() > MAX_RICH_BYTES.saturating_sub(self.bytes) {
            self.incomplete = true;
            return None;
        }
        self.bytes += text.len();
        Some(Value::String(text.into()))
    }
}
fn rich_project(
    workspace: Option<&Workspace>,
    media: &MediaRegistry,
    blocks: &[Value],
    fallback: Option<&str>,
) -> (String, CellContent) {
    let mut budget = Budget::default();
    let mut bounded = Vec::new();
    for block in blocks {
        if let Some(block) = restricted_block(block, 0, &mut budget).filter(is_container) {
            bounded.push(block);
        }
        if budget.incomplete {
            break;
        }
    }
    // Never render a partial link, task label, or other changed meaning. The
    // explicit fallback replaces the cell when any display budget is exceeded.
    if budget.incomplete || fallback.is_some_and(|text| text.len() > MAX_RICH_BYTES) {
        return (TOO_LARGE.into(), CellContent::Plain);
    }
    let fallback = fallback
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| bounded_plain(&bounded));
    let Some(workspace) = workspace else {
        return (fallback, CellContent::Plain);
    };
    let ctx = BlockCtx::new(workspace, media);
    let nodes: Vec<_> = bounded
        .iter()
        .flat_map(|block| block_nodes(ctx, block))
        .map(safe_node)
        .collect();
    if nodes.is_empty() || nodes.iter().all(|node| node.plain_text().is_empty()) {
        return (fallback, CellContent::Plain);
    }
    (
        nodes
            .iter()
            .map(RichNode::plain_text)
            .collect::<Vec<_>>()
            .join("\n"),
        CellContent::Rich(nodes),
    )
}
fn restricted_block(value: &Value, depth: usize, budget: &mut Budget) -> Option<Value> {
    if budget.incomplete {
        return None;
    }
    if depth > MAX_RICH_DEPTH || budget.nodes == MAX_RICH_NODES {
        budget.incomplete = true;
        return None;
    }
    budget.nodes += 1;
    let kind = value.get("type")?.as_str()?;
    if !is_container(value)
        && !matches!(
            kind,
            "text" | "link" | "emoji" | "user" | "usergroup" | "channel" | "broadcast" | "date"
        )
    {
        return None;
    }
    let mut output = serde_json::Map::new();
    output.insert("type".into(), Value::String(kind.into()));
    // Copy only scalar attributes consumed by the rich renderer. Unknown raw
    // payload subtrees never enter a clone or the recursive renderer.
    for key in [
        "text",
        "url",
        "name",
        "user_id",
        "usergroup_id",
        "channel_id",
        "range",
        "fallback",
    ] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            output.insert(key.into(), budget.string(text)?);
        }
    }
    if let Some(style) = value.get("style") {
        if let Some(style) = style.as_str() {
            output.insert("style".into(), budget.string(style)?);
        } else if style.is_object() {
            let mut flags = serde_json::Map::new();
            for key in ["bold", "italic", "strike", "code"] {
                if let Some(flag) = style.get(key).and_then(Value::as_bool) {
                    flags.insert(key.into(), Value::Bool(flag));
                }
            }
            output.insert("style".into(), Value::Object(flags));
        }
    }
    for key in ["indent", "offset"] {
        if let Some(number) = value.get(key).and_then(Value::as_u64) {
            output.insert(key.into(), Value::from(number));
        }
    }
    if is_container(value) {
        let children = value.get("elements")?.as_array()?;
        let mut bounded = Vec::new();
        for child in children {
            if let Some(child) = restricted_block(child, depth + 1, budget) {
                bounded.push(child);
            }
            if budget.incomplete {
                break;
            }
        }
        output.insert("elements".into(), Value::Array(bounded));
    }
    Some(Value::Object(output))
}
fn bounded_plain(blocks: &[Value]) -> String {
    fn append(value: &Value, text: &mut String) {
        match value["type"].as_str().unwrap_or_default() {
            "text" => text.push_str(value["text"].as_str().unwrap_or_default()),
            "link" => text.push_str(
                value
                    .get("text")
                    .or_else(|| value.get("url"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            "emoji" => {
                text.push(':');
                text.push_str(value["name"].as_str().unwrap_or_default());
                text.push(':');
            }
            "user" | "channel" | "usergroup" => {
                let (key, prefix) = match value["type"].as_str() {
                    Some("user") => ("user_id", "@"),
                    Some("channel") => ("channel_id", "#"),
                    _ => ("usergroup_id", "@"),
                };
                text.push_str(prefix);
                text.push_str(value[key].as_str().unwrap_or_default());
            }
            "date" => text.push_str(value["fallback"].as_str().unwrap_or_default()),
            "broadcast" => {
                text.push('@');
                text.push_str(value["range"].as_str().unwrap_or("channel"));
            }
            _ => {
                if let Some(children) = value["elements"].as_array() {
                    for child in children {
                        append(child, text);
                    }
                    if !text.ends_with('\n') {
                        text.push('\n');
                    }
                }
            }
        }
    }
    let mut text = String::new();
    for block in blocks {
        append(block, &mut text);
    }
    text.trim_end().into()
}

fn link(label: String, url: String) -> RichNode {
    match super_platinum_core::canvas::link_url(&url) {
        Some(url) => RichNode::Link { label, url },
        None => RichNode::Text(label),
    }
}

fn safe_node(node: RichNode) -> RichNode {
    match node {
        RichNode::Link { label, url } => link(label, url),
        RichNode::Paragraph(nodes) => {
            RichNode::Paragraph(nodes.into_iter().map(safe_node).collect())
        }
        RichNode::Quote(nodes) => RichNode::Quote(nodes.into_iter().map(safe_node).collect()),
        RichNode::List {
            ordered,
            indent,
            offset,
            items,
        } => RichNode::List {
            ordered,
            indent,
            offset,
            items: items
                .into_iter()
                .map(|nodes| nodes.into_iter().map(safe_node).collect())
                .collect(),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests;
