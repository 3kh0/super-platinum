//! Shared member selection rows; owners decide when a selection becomes state.
use crate::{
    icons::{Icon, icon},
    message_buttons::inputs::user::directory::Choice,
};
use dioxus::prelude::*;

#[component]
pub(crate) fn SelectedUsers(
    selected: Vec<(String, String)>,
    disabled: bool,
    #[props(default = true)] removable: bool,
    #[props(default = false)] conversation: bool,
    #[props(default = false)] channel: bool,
    onremove: EventHandler<String>,
) -> Element {
    let group_label = if channel {
        "Selected channels"
    } else if conversation {
        "Selected conversations"
    } else {
        "Selected users"
    };
    rsx! {
        if !selected.is_empty() {
            div {class:"message-multi-selected",role:"group","aria-label":group_label,
                for (id,label) in selected {
                    {let remove_id=id.clone();rsx! {
                    div {key:"{id}",class:"message-multi-selected-option","data-selected-user":"{id}",span {"{label}"}
                        if removable {
                            button {r#type:"button",class:"icon-btn",disabled,"aria-label":"Remove {label}","data-remove-user":"{id}",
                                onclick:move |_|onremove.call(remove_id.clone()),{icon(Icon::Close,"icon")}}
                        }
                    }
                    }}
                }
            }
        }
    }
}
#[component]
pub(crate) fn UserChoices(
    choices: Vec<Choice>,
    selected: Vec<(String, String)>,
    multiple: bool,
    conversation: bool,
    #[props(default = false)] channel: bool,
    disabled: bool,
    onchoose: EventHandler<String>,
) -> Element {
    let group_label = if channel {
        "Channels"
    } else if conversation {
        "Conversations"
    } else {
        "Users"
    };
    let empty_label = if channel {
        "No matching channels"
    } else if conversation {
        "No matching conversations"
    } else {
        "No matching users"
    };
    rsx! {div {class:"message-user-options",role:"group","aria-label":group_label,
        for choice in &choices {
            {let id=choice.id.clone();let checked=selected.iter().any(|(old,_)|old==&id);rsx! {
                button {key:"{id}",r#type:"button",class:"message-user-option",disabled,role:if multiple {"checkbox"}else{"button"},
                    "aria-checked":multiple.then(||checked.to_string()),"data-entity-id":"{id}",
                    "data-user-id":(!conversation).then(||id.clone()),"data-conversation-id":conversation.then(||id.clone()),
                    onclick:move |_|onchoose.call(id.clone()),
                    if multiple {span {style:if checked {"visibility:visible"}else{"visibility:hidden"},{icon(Icon::Check,"icon")}}}
                    "{choice.label}"
                }
            }}
        }
        if choices.is_empty(){p {class:"muted","{empty_label}"}}
    }}
}
