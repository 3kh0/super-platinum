//! File-token display with explicit original List occurrence ownership.
use crate::slack_lists::comments::{file_acquisition, file_navigation};
use crate::state::{MessageVm, RichNode, ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
#[cfg(test)]
mod tests;

/// This map only associates borrowed render nodes with their original traversal
/// ordinal. Stored click authority is the controller Token, never the address.
pub(crate) struct ListSource {
    source: Option<file_navigation::Source>,
    ordinals: HashMap<usize, usize>,
}
impl ListSource {
    pub(crate) fn capture(shell: &ShellState, generation: u64, message: &MessageVm) -> Self {
        let ordinals = occurrences(&message.body);
        let source = if ordinals.is_empty() {
            None
        } else {
            file_navigation::Source::capture(shell, generation, message)
        };
        Self { source, ordinals }
    }
    fn token(
        &self,
        shell: &ShellState,
        node: &RichNode,
        team: &str,
        raw: &Value,
    ) -> Option<file_navigation::Token> {
        let ordinal = *self.ordinals.get(&(node as *const RichNode as usize))?;
        self.source.as_ref()?.occurrence(shell, team, raw, ordinal)
    }
    fn demand(
        &self,
        shell: &ShellState,
        node: &RichNode,
        team: &str,
        raw: &Value,
    ) -> Option<file_acquisition::Demand> {
        let ordinal = *self.ordinals.get(&(node as *const RichNode as usize))?;
        self.source
            .as_ref()?
            .cold_occurrence(shell, team, raw, ordinal)
    }
}

/// Acquisition status never grants navigation authority. A ready metadata read
/// still needs the original occurrence's independently captured file target.
fn inline_state(
    can_open: bool,
    cold: Option<file_acquisition::Presentation>,
) -> file_acquisition::Presentation {
    if can_open {
        return file_acquisition::Presentation {
            state: "ready",
            notice: None,
            retryable: false,
        };
    }
    match cold {
        Some(mut view) if view.state == "loading" => {
            view.retryable = false;
            view.notice.get_or_insert_with(|| "Loading file…".into());
            view
        }
        view => file_acquisition::Presentation {
            state: "unavailable",
            notice: Some(
                view.as_ref()
                    .and_then(|view| view.notice.clone())
                    .unwrap_or_else(|| "File unavailable".into()),
            ),
            retryable: view.is_some_and(|view| view.retryable),
        },
    }
}
fn occurrences(nodes: &[RichNode]) -> HashMap<usize, usize> {
    fn walk(node: &RichNode, out: &mut HashMap<usize, usize>) {
        match node {
            RichNode::FileMention { .. } => {
                out.insert(node as *const RichNode as usize, out.len());
            }
            RichNode::Paragraph(nodes)
            | RichNode::Quote(nodes)
            | RichNode::Header(nodes)
            | RichNode::Context(nodes)
            | RichNode::Actions(nodes) => {
                for node in nodes {
                    walk(node, out);
                }
            }
            RichNode::Section {
                text,
                fields,
                accessory,
            } => {
                for node in text {
                    walk(node, out);
                }
                for field in fields {
                    for node in field {
                        walk(node, out);
                    }
                }
                if let Some(node) = accessory {
                    walk(node, out);
                }
            }
            RichNode::List { items, .. } => {
                for item in items {
                    for node in item {
                        walk(node, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = HashMap::new();
    for node in nodes {
        walk(node, &mut out);
    }
    out
}

pub(super) fn render(
    team: &str,
    raw: &Value,
    fallback: &str,
    state: Signal<ShellState>,
    node: &RichNode,
    list: Option<&ListSource>,
) -> Element {
    let (label, url) = {
        let shell = state.read();
        (
            label(&shell, team, raw, fallback),
            destination(&shell, team, raw),
        )
    };
    let class = style_class(raw);
    let id = raw["file_id"].as_str().unwrap_or_default();
    let kind = raw["type"].as_str().unwrap_or_default();
    if let Some(source) = list {
        let (token, demand, cold) = {
            let shell = state.read();
            let token = source.token(&shell, node, team, raw);
            let demand = source.demand(&shell, node, team, raw);
            let cold = demand.as_ref().map(|demand| demand.presentation(&shell));
            (token, demand, cold)
        };
        let disabled = token.is_none();
        let view = inline_state(!disabled, cold);
        let loading = view.state == "loading";
        let title = if disabled {
            view.notice
                .clone()
                .unwrap_or_else(|| "File unavailable".into())
        } else {
            "Open file".into()
        };
        let retry_label = format!("Retry loading {label}");
        return rsx! {
            button {
                r#type: "button", class,
                style: "padding:0;color:var(--link);text-decoration:underline;text-align:inherit;line-height:inherit;",
                disabled,
                title,
                "aria-disabled": if disabled { "true" } else { "false" },
                "aria-busy": if loading { "true" } else { "false" },
                "data-rich-file-id": id,
                "data-rich-file-kind": kind,
                "data-rich-file-state": view.state,
                onclick: move |event| {
                    event.prevent_default();
                    event.stop_propagation();
                    if let Some(token) = token.as_ref() { file_navigation::open(state, token.clone()); }
                },
                "{label}"
            }
            if let Some(notice) = view.notice {
                span {
                    role: "status",
                    style: "margin-inline-start:var(--space-1);font-size:var(--text-sm);color:var(--muted);",
                    "data-rich-file-status": id,
                    "{notice}"
                }
            }
            if view.retryable {
                if let Some(demand) = demand {
                    button {
                        r#type: "button",
                        style: "margin-inline-start:var(--space-1);padding:0;color:var(--link);font-size:var(--text-sm);text-decoration:underline;",
                        "data-rich-file-retry": id,
                        "aria-label": retry_label,
                        onclick: move |event| {
                            event.prevent_default();
                            event.stop_propagation();
                            file_acquisition::retry(state, demand.clone());
                        },
                        "Retry"
                    }
                }
            }
        };
    }
    if let Some(url) = url {
        let href = url.clone();
        let team = team.to_owned();
        rsx! {
            a {
                class, href,
                "data-rich-file-id": id,
                "data-rich-file-kind": kind,
                onclick: move |event| {
                    event.prevent_default();
                    let current = { allowed(&state.read(), &team, &url) };
                    if current { crate::slack_links::open(state, &url); }
                },
                "{label}"
            }
        }
    } else {
        rsx! { span { class, "data-rich-file-id": id, "data-rich-file-kind": kind, "{label}" } }
    }
}
fn label(shell: &ShellState, team: &str, raw: &Value, fallback: &str) -> String {
    let labels = crate::profile_edit::sections::rich::search::draft_file_labels(
        shell,
        team,
        std::slice::from_ref(raw),
    );
    serde_json::to_string(raw)
        .ok()
        .and_then(|key| labels.get(&key).cloned())
        .unwrap_or_else(|| fallback.to_owned())
}
fn style_class(raw: &Value) -> String {
    let mut class = String::from("rich-file-mention");
    for (key, css) in [
        ("bold", " bold"),
        ("italic", " italic"),
        ("strike", " strike"),
        ("code", " inline-code"),
    ] {
        if raw["style"][key] == true {
            class.push_str(css);
        }
    }
    class
}
fn destination(shell: &ShellState, team: &str, raw: &Value) -> Option<String> {
    let url = raw["url"].as_str()?;
    allowed(shell, team, url).then(|| url.to_owned())
}
fn allowed(shell: &ShellState, team: &str, input: &str) -> bool {
    if shell.core.active_team.as_deref() != Some(team)
        || !shell.core.workspaces.contains_key(team)
        || input.trim() != input
        || input.chars().any(char::is_control)
    {
        return false;
    }
    let Ok(url) = url::Url::parse(input) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && crate::slack_links::target_team(shell, input).is_ok()
}
