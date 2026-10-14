use crate::{
    icons::{Icon, icon},
    message_buttons::{
        inputs::{display, external},
        source::Source,
    },
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::block_actions::BlockElementMetadata;
#[component]
pub(crate) fn MessageExternalSelect(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
) -> Element {
    let mounted =
        dioxus::core::use_hook_with_cleanup(display::Mounted::new, display::Mounted::unmount);
    let hydration = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive((&source, &hydration), move |(source, _)| {
        crate::message_buttons::inputs::ensure(state, source.clone());
        crate::message_buttons::hydration::ensure(state, source);
    }));
    let (enabled, label, reason) =
        external::presentation(&state.read(), &source, &metadata, &mounted);
    let title = reason.as_deref().unwrap_or(&label);
    let aria = metadata
        .accessibility_label
        .clone()
        .unwrap_or_else(|| label.clone());
    rsx! {button {r#type:"button",class:"block-select block-users-select block-external-select",disabled:!enabled,title,"aria-label":aria,"aria-haspopup":"dialog",
        "data-action-id":metadata.action_id.as_deref().unwrap_or_default(),"data-block-id":metadata.block_id.as_deref().unwrap_or_default(),
        onclick:move |event|{event.stop_propagation();external::open(state,source.clone(),metadata.clone(),mounted.clone());},
        ondoubleclick:move |event|event.stop_propagation(),span {"{label}"}{icon(Icon::ChevronRight,"icon block-multi-chevron")}
    }}
}
pub(crate) fn dialog(state: Signal<ShellState>) -> Element {
    let Some(owner) = external::owner(&state.read()) else {
        return rsx! {};
    };
    let id = owner.id();
    if owner.selected(&state.read()).is_some() {
        return super::message_external_multiple::dialog(state, owner);
    }
    rsx! {
        div {class:"scrim",onclick:move |_|external::cancel(state,id)}
        section {id:"shell-overlay-dialog",class:"modal button-confirmation message-multi-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"external-title",tabindex:"-1",
            "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":id.to_string(),
            onkeydown:move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();external::cancel(state,id);},
            header {h2 {id:"external-title","Choose an option"}button {r#type:"button",class:"icon-btn","aria-label":"Close selection",onclick:move |_|external::cancel(state,id),{icon(Icon::Close,"icon")}}}
            div {class:"message-multi-content",super::external_choices::ExternalChoices {key:"external-{id}",state,owner,id:"external-search".to_owned(),inline_done:false,onclose:move |_|external::cancel(state,id)}}
            footer {button {id:"external-cancel",r#type:"button",onclick:move |_|external::cancel(state,id),"Cancel"}}
        }
    }
}
