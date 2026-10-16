//! Shared rich-text input for small compose dialogs. The DOM owns selection.
use crate::icons::{Icon, icon};
use dioxus::prelude::*;
const EDITOR: &str = include_str!("../rich_editor.js");

/// Resolve only identities referenced by this draft; the original blocks stay
/// untouched and unknown tokens keep the editor's existing inert fallback.
pub(super) fn mention_labels(
    workspace: &super_platinum_core::state::Workspace,
    blocks: &[serde_json::Value],
) -> String {
    let mut labels = std::collections::BTreeMap::new();
    let mut pending: Vec<_> = blocks.iter().collect();
    while let Some(value) = pending.pop() {
        if let Some(id) = value.get("user_id").and_then(serde_json::Value::as_str) {
            labels.insert(id, format!("@{}", workspace.display_name(id)));
        }
        if let Some(id) = value.get("channel_id").and_then(serde_json::Value::as_str)
            && let Some(channel) = workspace.channels.get(id)
        {
            labels.insert(
                id,
                format!(
                    "#{}",
                    super_platinum_core::state::channel_display_name(workspace, channel)
                ),
            );
        }
        if let Some(id) = value
            .get("usergroup_id")
            .and_then(serde_json::Value::as_str)
            && let Some(group) = workspace.usergroups.get(id)
        {
            labels.insert(id, format!("@{}", group.handle));
        }
        if let Some(children) = value.get("elements").and_then(serde_json::Value::as_array) {
            pending.extend(children);
        }
    }
    serde_json::to_string(&labels).unwrap_or_else(|_| "{}".into())
}

#[component]
pub(crate) fn RichEditor(
    id: &'static str,
    label: String,
    #[props(default = "[]".into())] blocks: String,
    #[props(default = "{}".into())] labels: String,
    #[props(default)] disabled: bool,
    #[props(default = true)] allow_lists: bool,
    #[props(default = true)] autofocus: bool,
    #[props(default)] expected_generation: Option<u64>,
) -> Element {
    let label_id = format!("{id}-label");
    let link_id = format!("{id}-link-url");
    let generation =
        serde_json::to_string(&expected_generation.map(|value| value.to_string())).unwrap();
    rsx! {
        div { class: "rich-editor",
            onmounted: move |_| {
                let id = serde_json::to_string(id).unwrap();
                dioxus::document::eval(&format!("{EDITOR}\nwindow.superPlatinumRichEditor.mount({id},{blocks},{labels},{autofocus},{generation});"));
            },
            label {id: "{label_id}", "{label}"}
            div {class: "format-toolbar", role: "toolbar", "aria-label": "Text formatting",
                for (name, command, symbol) in [("Bold", "bold", Icon::FormatBold), ("Italic", "italic", Icon::FormatItalic), ("Strikethrough", "strikeThrough", Icon::FormatStrike), ("Bulleted list", "insertUnorderedList", Icon::FormatListBulleted), ("Numbered list", "insertOrderedList", Icon::FormatListNumbered)].into_iter().filter(|(_, command, _)| allow_lists || !matches!(*command, "insertUnorderedList" | "insertOrderedList")) {
                    button {r#type: "button", class: "composer-icon-btn", title: name, "aria-label": name, disabled,
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| {
                            let id = serde_json::to_string(id).unwrap();
                            dioxus::document::eval(&format!("window.superPlatinumRichEditor.format({id},'{command}');"));
                        },
                        {icon(symbol, "icon")}
                    }
                }
                button {r#type:"button",class:"composer-icon-btn rich-code-toggle",title:"Inline code","aria-label":"Inline code",disabled,
                    onmousedown:move |event|event.prevent_default(),
                    {icon(Icon::Code,"icon")}
                }
                button {r#type:"button",class:"composer-icon-btn rich-link-open",title:"Edit link","aria-label":"Edit link","aria-expanded":"false",disabled,
                    onmousedown:move |event|event.prevent_default(),
                    {icon(Icon::Link,"icon")}
                }
            }
            p {class:"rich-code-status",role:"status",hidden:true}
            div {class:"rich-link-controls",hidden:true,role:"group","aria-label":"Edit link",
                label {r#for:link_id.clone(),"Link URL"}
                input {id:link_id,r#type:"text",class:"rich-link-url",autocomplete:"off",spellcheck:"false",placeholder:"https://example.com",disabled}
                p {class:"rich-link-error",role:"alert",hidden:true}
                div {class:"rich-link-actions",
                    button {r#type:"button",class:"rich-link-apply",disabled,"Apply link"}
                    button {r#type:"button",class:"rich-link-remove",hidden:true,disabled,"Remove link"}
                    button {r#type:"button",class:"rich-link-cancel","Cancel"}
                }
            }
            // Remain a focusable read-only textbox while the same draft is protected.
            div {id, class: "rich-reminder-editor", tabindex:"0", "aria-readonly":disabled, contenteditable: if disabled {"false"} else {"true"}, role: "textbox", "aria-multiline": "true", "aria-labelledby": "{label_id}"}
        }
    }
}
