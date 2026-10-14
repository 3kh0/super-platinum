//! Two-phase confirmation keeps the ordinary picker and its staged choices alive.
use crate::{
    icons::{Icon, icon},
    message_buttons::{
        external::Owner,
        inputs::external::{self, multiple},
    },
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::block_actions::BlockTextFormat;

pub(super) fn dialog(state: Signal<ShellState>, owner: Owner) -> Element {
    let Some(view) = multiple::picker(&state.read()) else {
        return rsx! {};
    };
    let id = owner.id();
    let confirming = view.confirming;
    let confirmation = view.confirmation.as_ref().filter(|_| confirming);
    let title = confirmation
        .and_then(|c| c.title.as_ref())
        .map(super::button_confirmation::plain_label)
        .unwrap_or_else(|| "Choose options".into());
    let apply = confirmation
        .and_then(|c| c.confirm.as_ref())
        .map(super::button_confirmation::plain_label)
        .unwrap_or_else(|| "Apply".into());
    let cancel = confirmation
        .and_then(|c| c.deny.as_ref())
        .map(super::button_confirmation::plain_label)
        .unwrap_or_else(|| "Cancel".into());
    let body = confirmation.and_then(|c|c.text.as_ref()).map(|text|serde_json::json!({"type":if text.format==BlockTextFormat::Mrkdwn {"mrkdwn"} else {"plain_text"},"text":text.text,"emoji":text.emoji}));
    let submit_class = if confirmation.and_then(|c| c.style.as_deref()) == Some("danger") {
        "button-confirmation-submit danger"
    } else {
        "button-confirmation-submit"
    };
    rsx! {
        div {class:"scrim",onclick:move |_|external::cancel(state,id)}
        section {id:"shell-overlay-dialog",class:"modal button-confirmation message-multi-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"external-title",tabindex:"-1",
            "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":id.to_string(),
            onkeydown:move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();external::cancel(state,id);},
            header {h2 {id:"external-title","{title}"}button {r#type:"button",class:"icon-btn","aria-label":"Close selection",onclick:move |_|external::cancel(state,id),{icon(Icon::Close,"icon")}}}
            div {class:"message-multi-content",
                if let Some(body)=body.as_ref(){div {class:"button-confirmation-body",{super::message_radio::composition(state,Some(body),"")}}}
                if !confirming {super::external_choices::ExternalChoices {key:"external-{id}",state,owner,id:"external-search".to_owned(),inline_done:false,onclose:move |_|external::cancel(state,id)}}
            }
            if let Some(reason)=view.reason {div {class:"message-multi-status",role:"alert","{reason}"}}
            footer {
                button {id:"external-cancel",r#type:"button",onclick:move |_|if confirming{multiple::confirmation_cancel(state,id)}else{external::cancel(state,id)},"{cancel}"}
                button {id:"external-apply",r#type:"button",class:submit_class,disabled:!view.enabled,onclick:move |_|multiple::apply(state,id),"{apply}"}
            }
        }
    }
}
