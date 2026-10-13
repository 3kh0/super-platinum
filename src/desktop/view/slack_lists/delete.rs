//! Explicit item-and-subtasks confirmation; no mutation is started by mounting.
use super::*;
use crate::slack_lists::delete;

pub(super) fn status(state: Signal<ShellState>) -> Element {
    let view = delete::presentation(&state.read());
    if let Some(operation) = view.operation {
        let serial = operation.serial;
        let visit = crate::slack_lists::generation(&state.read()).unwrap_or_default();
        let stage = format!("{:?}", operation.stage);
        rsx! {div{id:"slack-list-delete-status",class:"slack-list-delete-status",role:"status","data-stage":stage,"data-delete-serial":serial,
            onmounted:move |_|{dioxus::document::eval(&format!("{MOUNT}\nmountListDeleteStatus({visit},{serial});"));},"aria-busy":operation.pending||operation.checking,
            p{"{operation.message}"}
            if operation.can_cancel {button{id:"slack-list-delete-cancel-pending",r#type:"button",onclick:move |_|delete::cancel_preparing(state,serial),"Cancel"}}
            if matches!(operation.stage,delete::Stage::AwaitingRead|delete::Stage::Uncertain) {button{id:"slack-list-delete-check",r#type:"button",disabled:!operation.can_check,onclick:move |_|delete::check_result(state,serial),if operation.checking{"Checking…"}else{"Check result"}}}
        }}
    } else if let Some(serial) = view.undo {
        let visit = crate::slack_lists::generation(&state.read()).unwrap_or_default();
        let label = if view.redo { "Redo" } else { "Undo" };
        let notice = if view.redo {
            "Item restored."
        } else {
            "Item deleted."
        };
        rsx! {div{id:"slack-list-delete-status",class:"slack-list-delete-status",role:"status",
            p{"{notice}"}
            button{id:"slack-list-delete-undo",r#type:"button",onclick:move |_|delete::undo_delete(state,serial,visit),"{label}"}
            button{id:"slack-list-delete-dismiss",r#type:"button",onclick:move |_|delete::dismiss_undo(state,serial,visit),"Dismiss"}
        }}
    } else if let Some(notice) = view.notice {
        rsx! {p{id:"slack-list-delete-status",role:"status","{notice}"}}
    } else {
        rsx! {}
    }
}
pub(super) fn confirmation(state: Signal<ShellState>, visit: u64) -> Element {
    let view = delete::presentation(&state.read());
    let Some(confirm) = view.confirmation else {
        return rsx! {};
    };
    let generation = confirm.generation;
    let title = if confirm.title.trim().is_empty() {
        "Untitled item".into()
    } else {
        confirm.title
    };
    rsx! {DeleteConfirmation{key:"delete-{generation}",state,visit,generation,title,can_confirm:confirm.can_confirm,reason:confirm.reason}}
}
#[component]
fn DeleteConfirmation(
    state: Signal<ShellState>,
    visit: u64,
    generation: u64,
    title: String,
    can_confirm: bool,
    reason: Option<String>,
) -> Element {
    rsx! {div{class:"slack-list-editor-shade",
        section{id:"slack-list-delete-confirm",class:"slack-list-cell-editor slack-list-delete-confirm",role:"alertdialog","aria-modal":"true","aria-labelledby":"slack-list-delete-title","aria-describedby":"slack-list-delete-description","data-keyboard-dialog":"true","data-escape-local":"true","data-delete-generation":generation,tabindex:"-1",
            onmounted:move |_|{dioxus::document::eval(&format!("{MOUNT}\nmountListDelete({visit},{generation});"));},
            onkeydown:move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();delete::cancel_confirmation(state,generation);},
            h3{id:"slack-list-delete-title","Delete item?"}
            p{id:"slack-list-delete-description","Delete “{title}” and any associated subtasks?"}
            if let Some(reason)=reason {p{class:"slack-list-error",role:"alert","{reason}"}}
            footer{button{id:"slack-list-delete-cancel",r#type:"button",onclick:move |_|delete::cancel_confirmation(state,generation),"Cancel"}
                button{id:"slack-list-delete-confirm-button",r#type:"button",class:"danger",disabled:!can_confirm,onclick:move |_|delete::confirm_delete(state,generation),"Delete item"}}
        }
    }}
}
const MOUNT: &str = include_str!("delete_focus.js");
