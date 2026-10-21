//! A mounted edit has its own native binding; it never uses the reply journal.
use crate::{slack_lists::comments::own_edit as controller, state::ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) mod bridge;

pub(crate) fn panel(state: Signal<ShellState>) -> Element {
    let Some(view) = controller::presentation(&state.read()) else {
        return rsx! {};
    };
    let scope = view.binding.scope();
    rsx! { Editor { key: "list-comment-edit-{scope}", state } }
}

#[component]
fn Editor(state: Signal<ShellState>) -> Element {
    // Only the controller's positively admitted rich seed may create this view.
    // RichEditor retains opaque token bytes; it is not an eligibility decoder.
    let Some(view) = controller::presentation(&state.read()) else {
        return rsx! {};
    };
    let binding = view.binding.clone();
    let generation = binding.generation();
    let scope = binding.scope();
    let labels = {
        let shell = state.read();
        let blocks = serde_json::from_str::<Vec<Value>>(&view.blocks).unwrap_or_default();
        let team = binding.owner_team();
        let mut labels = shell
            .core
            .workspaces
            .get(team)
            .and_then(|workspace| {
                serde_json::from_str::<BTreeMap<String, String>>(
                    &crate::view::rich_editor::mention_labels(workspace, &blocks),
                )
                .ok()
            })
            .unwrap_or_default();
        labels.extend(
            crate::profile_edit::sections::rich::search::draft_file_labels(&shell, team, &blocks),
        );
        serde_json::to_string(&labels).unwrap_or_else(|_| "{}".into())
    };
    rsx! {
        section {
            id: "slack-list-comment-edit",
            class: "slack-list-comment-compose",
            role: "region",
            "aria-label": "Edit comment",
            "aria-busy": if view.busy { "true" } else { "false" },
            "data-list-comments-edit-scope": "{scope}",
            // Shared exact-node bridge selector; scope and editor ID are distinct
            // from the independently mounted new-reply composer.
            "data-list-comments-compose-scope": "{scope}",
            "data-editor-generation": generation,
            "data-edit-revision": view.revision,
            div {
                key: "list-comment-edit-editor-{generation}",
                class: "slack-list-comment-compose-editor",
                inert: view.finishing.then_some(true),
                onmounted: { let binding = binding.clone(); move |_| bridge::mount(state, binding.clone()) },
                crate::view::rich_editor::RichEditor {
                    id: "slack-list-comment-edit-draft",
                    label: "Edit comment".to_owned(),
                    blocks: view.blocks,
                    labels,
                    // The exact-node lease makes the wrapper inert while saving.
                    // Disabling this node would invalidate that held lease.
                    disabled: false,
                    autofocus: true,
                    expected_generation: Some(generation),
                }
            }
            if let Some(notice) = view.notice {
                p { id: "slack-list-comment-edit-status", class: "slack-list-comment-compose-status", role: "status", "{notice}" }
            } else if view.busy {
                p { id: "slack-list-comment-edit-status", class: "slack-list-comment-compose-status", role: "status", "Saving comment…" }
            }
            div { class: "slack-list-comment-compose-actions",
                if view.can_retry_capture {
                    button {
                        id: "slack-list-comment-edit-retry", r#type: "button", class: "slack-list-action",
                        onclick: { let binding = binding.clone(); move |_| controller::retry_capture(state, binding.clone()) },
                        "Retry capture"
                    }
                }
                if view.can_check {
                    button {
                        id: "slack-list-comment-edit-check", r#type: "button", class: "slack-list-action",
                        disabled: view.busy,
                        onclick: { let binding = binding.clone(); move |_| controller::check_result(state, binding.clone()) },
                        "Check result"
                    }
                }
                button {
                    id: "slack-list-comment-edit-cancel", r#type: "button", class: "slack-list-action",
                    disabled: view.busy || !view.can_cancel,
                    onclick: { let binding = binding.clone(); move |_| controller::cancel(state, binding.clone()) },
                    if view.finishing { "Retry return" } else { "Cancel" }
                }
                button {
                    id: "slack-list-comment-edit-save", r#type: "button", class: "slack-list-action",
                    disabled: view.busy || !view.can_save,
                    onclick: move |_| controller::save(state, binding.clone()),
                    "Save"
                }
            }
        }
    }
}
