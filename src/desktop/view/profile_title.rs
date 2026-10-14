//! Separately saved Header title; ordinary profile fields are never bundled.
use crate::{
    icons::{Icon, icon},
    profile_title,
    state::ShellState,
};
use dioxus::prelude::*;
pub(crate) fn profile_title(state: Signal<ShellState>) -> Element {
    let Some(visit) = profile_title::presentation(&state.read()).map(|v| v.visit) else {
        return rsx! {};
    };
    rsx! {TitleEditor{key:"profile-title-{visit}",state}}
}
#[component]
fn TitleEditor(state: Signal<ShellState>) -> Element {
    let Some(v) = profile_title::presentation(&state.read()) else {
        return rsx! {};
    };
    let visit = v.visit;
    rsx! {
        div{class:"scrim",onclick:move |_|profile_title::close(state,visit)}
        section{id:"shell-overlay-dialog",class:"modal profile-edit profile-title",role:"dialog","aria-modal":"true","aria-labelledby":"profile-title-heading",tabindex:"-1",
            "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":visit,
            onkeydown:move |e|if e.key()==Key::Escape {e.prevent_default();e.stop_propagation();profile_title::close(state,visit);},
            header{h2{id:"profile-title-heading","Edit title"}button{r#type:"button",class:"icon-btn",disabled:v.busy,"aria-label":"Close title editor",onclick:move |_|profile_title::close(state,visit),{icon(Icon::Close,"icon")}}}
            form{id:"profile-title-form",onsubmit:move |e|{e.prevent_default();profile_title::save(state,visit);},
                div{class:"profile-edit-body",
                    if v.loading {p{role:"status","Loading your title and editing permissions…"}}
                    else {TitleField{state,visit,value:v.value,disabled:v.busy||v.uncertain||!v.allowed,error:v.validation}}
                    if let Some(reason)=v.reason {p{id:"profile-title-reason",class:"profile-edit-hint","{reason}"}}
                    if let Some(message)=v.message {p{id:"profile-title-message",class:"profile-edit-notice",role:"status","{message}"}}
                    if v.uncertain {p{id:"profile-title-uncertain",class:"profile-edit-notice","The previous save is unconfirmed. Checking your title never sends it again."}}
                }
                footer{
                    if v.can_retire {button{id:"profile-title-retire",r#type:"button",onclick:move |_|profile_title::keep_current(state,visit),"Keep current title"}}
                    button{id:"profile-title-cancel",r#type:"button",disabled:v.busy,onclick:move |_|profile_title::close(state,visit),if v.uncertain{"Close"}else{"Cancel"}}
                    if v.uncertain {button{id:"profile-title-check",r#type:"button",class:"profile-edit-primary",disabled:!v.can_check,onclick:move |_|profile_title::check_status(state,visit),if v.busy{"Checking…"}else{"Check title"}}}
                    else if v.can_reload {button{id:"profile-title-reload",r#type:"button",onclick:move |_|profile_title::reload(state,visit),"Reload"}}
                    else {button{id:"profile-title-save",r#type:"submit",class:"profile-edit-primary",disabled:!v.can_save,if v.busy{"Saving…"}else{"Save title"}}}
                }
            }
        }
    }
}
#[component]
fn TitleField(
    state: Signal<ShellState>,
    visit: u64,
    value: String,
    disabled: bool,
    error: Option<String>,
) -> Element {
    let seed = use_hook(|| value);
    rsx! {div{class:"profile-edit-field",
        label{r#for:"profile-title-input","What I do"}
        input{id:"profile-title-input",r#type:"text",initial_value:"{seed}",disabled,autocomplete:"off","data-dialog-input":"true","aria-describedby":"profile-title-hint","aria-invalid":error.is_some().to_string(),oninput:move |e|profile_title::change(state,visit,e.value())}
        if let Some(ref error)=error {p{id:"profile-title-hint",class:"profile-edit-field-error",role:"alert","{error}"}}
        else {p{id:"profile-title-hint",class:"profile-edit-hint","Your role or what you work on. Optional; saved separately from your name and pronouns."}}
    }}
}
