//! Shared rich-text input for small compose dialogs. The DOM owns selection.
use crate::icons::{Icon, icon};
use dioxus::prelude::*;
const EDITOR: &str = include_str!("../rich_editor.js");

#[component]
pub(crate) fn RichEditor(
    id: &'static str,
    label: String,
    #[props(default = "[]".into())] blocks: String,
    #[props(default = "{}".into())] labels: String,
    #[props(default)] disabled: bool,
) -> Element {
    let label_id = format!("{id}-label");
    rsx! {
        div { class: "rich-editor",
            onmounted: move |_| {
                let id = serde_json::to_string(id).unwrap();
                dioxus::document::eval(&format!("{EDITOR}\nwindow.superPlatinumRichEditor.mount({id},{blocks},{labels});"));
            },
            label {id: "{label_id}", "{label}"}
            div {class: "format-toolbar", role: "toolbar", "aria-label": "Text formatting",
                for (name, command, symbol) in [("Bold", "bold", Icon::FormatBold), ("Italic", "italic", Icon::FormatItalic), ("Strikethrough", "strikeThrough", Icon::FormatStrike)] {
                    button {r#type: "button", class: "composer-icon-btn", title: name, "aria-label": name, disabled,
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| {
                            let id = serde_json::to_string(id).unwrap();
                            dioxus::document::eval(&format!("window.superPlatinumRichEditor.format({id},'{command}');"));
                        },
                        {icon(symbol, "icon")}
                    }
                }
            }
            div {id, class: "rich-reminder-editor", contenteditable: if disabled {"false"} else {"true"}, role: "textbox", "aria-multiline": "true", "aria-labelledby": "{label_id}"}
        }
    }
}
