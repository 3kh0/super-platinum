//! Typed fixture-only consent surface with the shared modal keyboard manager.
use super::*;
pub(crate) fn view(state: Signal<ShellState>) -> Element {
    if !enabled() {
        return rsx! {};
    }
    rsx! {EnrollmentPanel {state}}
}
#[component]
fn EnrollmentPanel(state: Signal<ShellState>) -> Element {
    let ui = use_signal(controller::Panel::default);
    let panel = ui.read();
    let busy = panel.busy;
    let dialog = panel.dialog.clone();
    let pending = BINDING
        .get()
        .is_some_and(|b| b.discovery.lock().unwrap().pending_enrollment().is_some());
    let message = panel.message.clone();
    rsx! {
        style {{include_str!("ui.css")}}
        if let Some(dialog)=dialog {
            {super::super::enrollment_consent::dialog(
                &dialog.id, dialog.target, &dialog.views, &dialog.pending, busy,
                std::rc::Rc::new(move || controller::submit(state, ui)),
                std::rc::Rc::new(move || controller::cancel(ui)),
            )}
        } else {
            section {id:"protected-enrollment-status",class:"toast protected-enrollment-status",role:"status",
                onmounted:move |_|{let script=include_str!("../../../view/toast_position.js").replace("'status-toast'","'protected-enrollment-status'").replace(".composer-wrap,","#status-toast, .composer-wrap,").replace(".toast-dismiss",".protected-enrollment-actions");document::eval(&script);},
                div {class:"toast-text",p {if let Some(message)=message{"{message}"}else{"Review saved drafts before enabling recovery."}}}
                div {class:"protected-enrollment-actions",
                    if pending {button {id:"protected-enrollment-check",r#type:"button",disabled:busy,onclick:move |_|controller::check(state,ui),"Check status"}}
                    button {id:"protected-enrollment-reload",r#type:"button",disabled:busy,onclick:move |_|controller::reload(state,ui),"Reload saved drafts"}
                    button {id:"protected-enrollment-open",r#type:"button",disabled:busy||pending,onclick:move |_|controller::open(state,ui),"Review draft"}
                }
            }
        }
    }
}
