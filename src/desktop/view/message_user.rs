//! Ordinary user selection uses the message-owned picker and confirmation flow.
use crate::{
    icons::{Icon, icon},
    message_buttons::{
        inputs::{self, user},
        source::Source,
    },
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::block_actions::BlockElementMetadata;

#[component]
pub(crate) fn MessageUserSelect(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    accessibility_label: String,
) -> Element {
    let mounted = dioxus::core::use_hook_with_cleanup(
        inputs::display::Mounted::new,
        inputs::display::Mounted::unmount,
    );
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            inputs::ensure(state, source.clone());
            crate::message_buttons::hydration::ensure(state, source);
        },
    ));
    let view = user::presentation(&state.read(), &source, &metadata);
    let selected = inputs::display_selected(&state.read(), &source, &metadata, &mounted);
    let label = selected
        .as_deref()
        .map(|id| user::label(&state.read(), &source, id))
        .unwrap_or_else(|| view.placeholder.clone());
    let aria = if accessibility_label.is_empty() {
        view.placeholder.clone()
    } else {
        accessibility_label
    };
    let control_class = if user::is_conversation(&metadata) {
        "block-select block-conversations-select"
    } else {
        "block-select block-users-select"
    };
    let title = view.reason.as_deref().unwrap_or(&label);
    let hover_source = source.clone();
    rsx! {button {class:control_class,r#type:"button",disabled:!view.enabled,
        title:"{title}","aria-label":"{aria}","aria-haspopup":"dialog","aria-busy":view.busy.to_string(),
        "data-action-id":metadata.action_id.as_deref().unwrap_or_default(),"data-block-id":metadata.block_id.as_deref().unwrap_or_default(),
        "data-input-revision":view.revision.to_string(),
        onmouseenter:move |_|crate::message_buttons::hydration::retry(state,hover_source.clone()),
        onclick:move |event|{event.stop_propagation();user::open(state,source.clone(),metadata.clone(),mounted.clone());},
        ondoubleclick:move |event|event.stop_propagation(),span {"{label}"},{icon(Icon::ChevronRight,"icon block-multi-chevron")}
    }}
}
pub(crate) fn dialog(state: Signal<ShellState>) -> Element {
    let Some(revision) = user::picker(&state.read()).map(|p| p.revision) else {
        return rsx! {};
    };
    rsx! {UserDialog {key:"message-user-{revision}",state}}
}
#[component]
fn UserDialog(state: Signal<ShellState>) -> Element {
    let Some(picker) = user::picker(&state.read()) else {
        return rsx! {};
    };
    let revision = picker.revision;
    let query_revision = picker.query_revision;
    let search_label = if picker.conversation {
        "Find a conversation"
    } else {
        "Find a user"
    };
    let group_label = if picker.conversation {
        "Conversations"
    } else {
        "Users"
    };
    let empty_label = if picker.conversation {
        "No matching conversations"
    } else {
        "No matching users"
    };
    let close_label = if picker.conversation {
        "Close conversation selection"
    } else {
        "Close user selection"
    };
    rsx! {
        div {class:"scrim",onclick:move |_|user::cancel(state,revision)}
        section {id:"shell-overlay-dialog",class:"modal button-confirmation message-multi-dialog message-user-dialog",role:"dialog",tabindex:"-1",
            "aria-modal":"true","aria-labelledby":"message-user-title","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":revision,
            onkeydown:move |event|{
                if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();user::cancel(state,revision);}
                else if matches!(event.key(),Key::ArrowDown|Key::ArrowUp) {
                    event.prevent_default();event.stop_propagation();
                    let delta=if event.key()==Key::ArrowDown {1}else{-1};
                    dioxus::document::eval(&format!("(()=>{{const d=document.getElementById('shell-overlay-dialog');if(d?.dataset.dialogGeneration!=='{revision}')return;const es=[d.querySelector('#message-user-search'),...d.querySelectorAll('[data-entity-id]:not(:disabled)')].filter(Boolean);const i=es.indexOf(document.activeElement);es[(i+{delta}+es.length)%es.length]?.focus();}})();"));
                }
            },
            onmounted:move |_| {dioxus::document::eval(&format!("(()=>{{const d=document.getElementById('shell-overlay-dialog');if(d?.dataset.dialogGeneration!=='{revision}')return;const old=document.activeElement;requestAnimationFrame(()=>{{if(d.isConnected&&document.activeElement===old)d.querySelector('#message-user-search')?.focus({{preventScroll:true}});}});}})();"));},
            header {h2 {id:"message-user-title","{picker.placeholder}"}
                button {class:"icon-btn",r#type:"button","aria-label":close_label,onclick:move |_|user::cancel(state,revision),{icon(Icon::Close,"icon")}}
            }
            div {class:"message-multi-content",
                input {id:"message-user-search",r#type:"search",initial_value:"",autocomplete:"off",placeholder:search_label,"aria-label":search_label,
                    oninput:move |event|user::search(state,revision,event.value())}
                div {class:"message-user-options",role:"group","aria-label":group_label,
                    for choice in &picker.candidates {
                        {let id=choice.id.clone();rsx!{button {key:"{id}",r#type:"button",class:"message-user-option","data-entity-id":"{id}","data-user-id":(!picker.conversation).then(||id.clone()),"data-conversation-id":picker.conversation.then(||id.clone()),
                            onclick:move |_|user::choose(state,revision,query_revision,id.clone()),"{choice.label}"}}}
                    }
                    if picker.candidates.is_empty() {p {class:"muted","{empty_label}"}}
                }
            }
            div {class:"message-multi-status",role:"status",if picker.loading {"Searching…"}else if let Some(error)=&picker.error {"{error}"}}
            if picker.agent_unavailable {p {id:"message-user-availability",class:"message-multi-status","Some app users are unavailable in this picker."}}
            footer {
                if picker.error.is_some() {button {id:"message-user-retry",r#type:"button",disabled:picker.loading,onclick:move |_|user::search(state,revision,picker.query.clone()),"Retry search"}}
                button {id:"message-user-cancel",r#type:"button",onclick:move |_|user::cancel(state,revision),"Cancel"}}
        }
    }
}
