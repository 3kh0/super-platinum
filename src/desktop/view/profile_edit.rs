//! Compact own-profile text form. All permissions and write ownership remain in
//! the controller. Uncontrolled inputs preserve native caret/IME state.
use crate::{
    icons::{Icon, icon},
    profile_edit,
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::profile_text::Field;
mod photo;
mod sections;

pub(crate) fn profile_edit(state: Signal<ShellState>) -> Element {
    if profile_edit::sections::presentation(&state.read()).is_some() {
        return sections::view(state);
    }
    if profile_edit::photo::presentation(&state.read()).is_some() {
        return photo::view(state);
    }
    let Some(visit) = profile_edit::presentation(&state.read()).map(|v| v.visit) else {
        return rsx! {};
    };
    rsx! {ProfileEditor {key:"profile-edit-{visit}",state}}
}
#[component]
fn ProfileEditor(state: Signal<ShellState>) -> Element {
    let Some(view) = profile_edit::presentation(&state.read()) else {
        return rsx! {};
    };
    let visit = view.visit;
    let locked = view.loading || view.busy || view.uncertain;
    rsx! {
        div {class:"scrim",onclick:move |_|profile_edit::close(state,visit)}
        section {id:"shell-overlay-dialog",class:"modal profile-edit",role:"dialog","aria-modal":"true",
            "aria-labelledby":"profile-edit-title",tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":visit,
            onkeydown:move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();profile_edit::close(state,visit);},
            header {h2 {id:"profile-edit-title","Edit profile"}
                button {r#type:"button",class:"icon-btn",disabled:view.busy,"aria-label":"Close profile editor",onclick:move |_|profile_edit::close(state,visit),{icon(Icon::Close,"icon")}}
            }
            form {id:"profile-edit-form",onsubmit:move |event|{event.prevent_default();profile_edit::save(state,visit);},
                div {class:"profile-edit-body",
                    if view.loading {p {id:"profile-edit-loading",role:"status","Loading your profile and editing permissions…"}}
                    else {
                        button {id:"profile-edit-photo",r#type:"button",disabled:locked,onclick:move |_|profile_edit::photo::open(state,visit),"Change profile photo"}
                        button {id:"profile-edit-sections",r#type:"button",disabled:locked,onclick:move |_|profile_edit::sections::open(state,visit),"Edit profile fields"}
                        for field in view.fields {
                            ProfileField {key:"{visit}-{field.field.key()}",state,visit,field:field.field,value:field.value,allowed:field.allowed,
                                reason:field.reason,error:field.error,locked}
                        }
                    }
                    if view.status_conflict {p {class:"profile-edit-notice",role:"status","Finish or check your pending title or status change before saving your profile."}}
                    if let Some(error)=view.error {p {id:"profile-edit-error",class:"profile-edit-notice",role:"status","{error}"}}
                    if view.uncertain {p {id:"profile-edit-uncertain",class:"profile-edit-notice",role:"status","The previous change has not been confirmed. Checking your profile does not send it again."}}
                }
                footer {
                    if view.can_retire {button {id:"profile-edit-retire",r#type:"button",onclick:move |_|profile_edit::keep_current(state,visit),"Keep current profile"}}
                    button {id:"profile-edit-cancel",r#type:"button",disabled:view.busy,onclick:move |_|profile_edit::close(state,visit),if view.uncertain{"Close"}else{"Cancel"}}
                    if view.uncertain {
                        button {id:"profile-edit-check",r#type:"button",class:"profile-edit-primary",disabled:!view.can_check,onclick:move |_|profile_edit::check_status(state,visit),if view.busy{"Checking…"}else{"Check profile"}}
                    } else if !view.loading && view.can_reload {
                        button {id:"profile-edit-reload",r#type:"button",onclick:move |_|profile_edit::reload(state,visit),"Reload"}
                    } else {
                        button {id:"profile-edit-save",r#type:"submit",class:"profile-edit-primary",disabled:!view.can_save,"aria-busy":view.busy.to_string(),if view.busy{"Saving…"}else{"Save"}}
                    }
                }
            }
        }
    }
}
#[component]
fn ProfileField(
    state: Signal<ShellState>,
    visit: u64,
    field: Field,
    value: String,
    allowed: bool,
    reason: Option<String>,
    error: Option<String>,
    locked: bool,
) -> Element {
    let seed = use_hook(|| value);
    let key = field.key();
    let label = match field {
        Field::FullName => "Full name",
        Field::DisplayName => "Display name",
        Field::Pronouns => "Pronouns",
    };
    rsx! {div {class:"profile-edit-field",
        label {r#for:"profile-edit-{key}","{label}"}
        input {id:"profile-edit-{key}",r#type:"text",name:key,initial_value:"{seed}",disabled:locked||!allowed,autocomplete:"off",
            "aria-invalid":error.is_some().to_string(),"aria-describedby":"profile-edit-{key}-hint","data-dialog-input":"true",
            oninput:move |event|profile_edit::change(state,visit,field,event.value())}
        if let Some(reason)=reason {p {id:"profile-edit-{key}-hint",class:"profile-edit-hint","{reason}"}}
        else if let Some(ref error)=error {p {id:"profile-edit-{key}-hint",class:"profile-edit-field-error",role:"alert","{error}"}}
        else if field==Field::DisplayName {p {id:"profile-edit-{key}-hint",class:"profile-edit-hint","Leave blank to use your full name."}}
        else if field==Field::Pronouns {p {id:"profile-edit-{key}-hint",class:"profile-edit-hint","For example, she/her or they/them. Optional."}}
    }}
}
