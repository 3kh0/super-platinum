//! Status draft UI; ownership, expiration and network outcomes stay in controller.
use crate::{
    icons::{Icon, icon},
    self_status::Expiration,
    state::ShellState,
};
use dioxus::prelude::*;

pub(crate) fn self_status(state: Signal<ShellState>) -> Element {
    let Some(revision) = crate::self_status::presentation(&state.read()).map(|view| view.revision)
    else {
        return rsx! {};
    };
    rsx! {StatusEditor {key:"self-status-{revision}",state}}
}

#[component]
fn StatusEditor(state: Signal<ShellState>) -> Element {
    let mut choosing_emoji = use_signal(|| false);
    if choosing_emoji() {
        return rsx! {crate::reaction_picker::EmojiPicker {
            state,title:"Choose status emoji".to_owned(),
            onchoose:move |name:String|{
                crate::self_status::change_emoji(state,format!(":{name}:"));
                choosing_emoji.set(false);
            },
            onclose:move |_|choosing_emoji.set(false),
        }};
    }
    rsx! {StatusForm {state,onemoji:move |_|choosing_emoji.set(true)}}
}

#[component]
fn StatusForm(state: Signal<ShellState>, onemoji: EventHandler) -> Element {
    let Some(view) = crate::self_status::presentation(&state.read()) else {
        return rsx! {};
    };
    // Returning from the picker mounts a new form seeded from the current draft.
    // Ordinary text, expiry and async state renders keep the existing input node.
    let initial_text = use_hook(|| view.text.clone());
    let locked = view.busy || view.uncertain;
    let revision = view.revision;
    let expiry = view.expiration.token();
    let emoji_title = if view.emoji.is_empty() {
        "Choose status emoji".to_owned()
    } else {
        format!("Change status emoji ({})", view.emoji)
    };
    rsx! {
        div {class:"scrim",onclick:move |_|crate::self_status::close(state)}
        section {id:"shell-overlay-dialog",class:"modal self-status-editor",role:"dialog","aria-modal":"true",
            "aria-labelledby":"self-status-title",tabindex:"-1","data-keyboard-dialog":"true",
            "data-dialog-kind":"other","data-dialog-generation":revision,
            onkeydown:move |event|if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();crate::self_status::close(state);},
            header {
                h2 {id:"self-status-title","Set a status"}
                button {r#type:"button",class:"icon-btn","aria-label":"Close status editor",disabled:view.busy,
                    onclick:move |_|crate::self_status::close(state),{icon(Icon::Close,"icon")}}
            }
            form {id:"self-status-form",onsubmit:move |event|{event.prevent_default();crate::self_status::save(state);},
                div {class:"self-status-body",
                    label {r#for:"self-status-text","Status"}
                    div {class:"self-status-input-row",
                        button {id:"self-status-emoji",r#type:"button",class:"self-status-emoji",disabled:locked,
                            "aria-label":"Choose status emoji",title:"{emoji_title}",onclick:move |_|onemoji.call(()),
                            {emoji_preview(state,&view.emoji)}
                        }
                        input {id:"self-status-text",r#type:"text",initial_value:"{initial_text}",maxlength:100,
                            placeholder:"What's your status?",disabled:locked,autocomplete:"off",
                            "aria-describedby":"self-status-hint self-status-error","data-dialog-input":"true","data-focus-at-end":"true",
                            oninput:move |event|crate::self_status::change_text(state,event.value())}
                    }
                    div {class:"self-status-hint-row",
                        p {id:"self-status-hint","Your status appears next to your name."}
                        if !view.emoji.is_empty() {button {id:"self-status-remove-emoji",r#type:"button",disabled:locked,
                            onclick:move |_|crate::self_status::change_emoji(state,String::new()),"Remove emoji"}}
                    }
                    label {r#for:"self-status-expiry","Clear after"}
                    select {id:"self-status-expiry",value:"{expiry}",disabled:locked,
                        onchange:move |event|if let Some(expiration)=Expiration::from_token(&event.value()) {crate::self_status::change_expiration(state,expiration);},
                        for expiration in Expiration::ALL.iter().copied() {
                            option {value:expiration.token(),selected:expiration==view.expiration,"{expiration.label()}"}
                        }
                    }
                    if view.expiration==Expiration::Custom {
                        StatusDate {state,value:view.custom_datetime.clone(),disabled:locked}
                    }
                    if let Some(error)=view.error.as_deref() {p {id:"self-status-error",class:"self-status-error",role:"alert","{error}"}}
                    if view.uncertain {p {id:"self-status-uncertain",class:"self-status-notice",role:"status","The save has not been confirmed. Check your current status before trying again."}}
                }
                footer {
                    button {id:"self-status-cancel",r#type:"button",disabled:view.busy,onclick:move |_|crate::self_status::close(state),
                        if view.uncertain {"Close"}else{"Cancel"}}
                    if view.uncertain {
                        button {id:"self-status-check",r#type:"button",class:"self-status-save",disabled:view.busy,
                            onclick:move |_|crate::self_status::check_status(state),if view.busy {"Checking…"}else{"Check status"}}
                    } else {
                        button {id:"self-status-save",r#type:"submit",class:"self-status-save",disabled:!view.can_save||view.busy,
                            "aria-busy":view.busy.to_string(),if view.busy {"Saving…"}else{"Save"}}
                    }
                }
            }
        }
    }
}

#[component]
fn StatusDate(state: Signal<ShellState>, value: String, disabled: bool) -> Element {
    let initial = use_hook(|| value);
    rsx! {div {class:"self-status-custom",
        label {r#for:"self-status-custom-date","Date and time"}
        input {id:"self-status-custom-date",r#type:"datetime-local",initial_value:"{initial}",required:true,
            disabled,"aria-describedby":"self-status-timezone self-status-error",
            oninput:move |event|crate::self_status::change_custom_datetime(state,event.value())}
        p {id:"self-status-timezone","In your local time zone."}
    }}
}

fn emoji_preview(state: Signal<ShellState>, emoji: &str) -> Element {
    if emoji.is_empty() {
        return icon(Icon::AddReaction, "icon");
    }
    let shell = state.read();
    let name = emoji.trim_matches(':');
    let custom = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
        .and_then(|workspace| {
            super_platinum_core::state::custom_emoji_source(&workspace.custom_emoji, name)
        });
    match custom {
        Some(super_platinum_core::state::EmojiSource::Url(url)) => {
            let id = shell.media.register_emoji(name, url);
            let media_epoch = shell.media_epoch;
            drop(shell);
            rsx! {super::media_image::MediaImage {state,id,media_epoch,compact:true,class:"self-status-emoji-image".to_owned(),alt:emoji.to_owned(),fallback:rsx!{span {"{emoji}"}}}}
        }
        Some(super_platinum_core::state::EmojiSource::Unicode(glyph)) => rsx! {span {"{glyph}"}},
        None => {
            let glyph = if super_platinum_core::state::is_standard_emoji(name) {
                super_platinum_core::state::emoji_glyph(name)
            } else {
                emoji.to_owned()
            };
            rsx! {span {"{glyph}"}}
        }
    }
}
