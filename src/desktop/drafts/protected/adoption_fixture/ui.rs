//! Enrollment consent is shared; adoption changes only the local input route.
use super::*;
pub(crate) fn view(state: Signal<ShellState>) -> Element {
    if !enabled() {
        return rsx! {};
    }
    rsx! {AdoptionPanel{state}}
}
#[component]
fn AdoptionPanel(state: Signal<ShellState>) -> Element {
    let ui = use_signal(controller::Panel::default);
    #[cfg(test)]
    tests::observed(ui);
    let panel = ui.read();
    let busy = panel.busy;
    let dialog = panel.dialog.clone();
    let message = panel.message.clone();
    // Subscribe even on the first render, before async fixture installation.
    let shell = state.read();
    let (pending, enrolled, installed, failed, disconnected) = BINDING
        .get()
        .map(|b| {
            let host = b.host.lock().unwrap();
            (
                host.pending_enrollment().is_some() || b.adoption.lock().unwrap().is_some(),
                host.discovery()
                    .entry(&shell, &key(b.target()))
                    .is_some_and(|v| v.inventory().row().is_some()),
                host.installation(&b.account, &key(b.target())).is_some(),
                host.installation(&b.account, &key(b.target()))
                    .is_some_and(|r| r.editor().snapshot().error.is_some()),
                host.route_snapshot(&shell, &key(b.target()))
                    .is_ok_and(|r| r.kind == "managed" && !r.registry_current),
            )
        })
        .unwrap_or((false, false, false, false, false));
    rsx! {
        style{{include_str!("ui.css")}}
        if let Some(dialog)=dialog{
            {super::super::enrollment_consent::dialog(&dialog.id,dialog.target,&dialog.views,&dialog.pending,busy,std::rc::Rc::new(move||controller::submit(state,ui)),std::rc::Rc::new(move||controller::cancel(ui)))}
        }else{
            section{id:"protected-adoption-status",class:"toast protected-adoption-status",role:"status",
                onmounted:move |_|{let script=include_str!("../../../view/toast_position.js").replace("'status-toast'","'protected-adoption-status'").replace(".composer-wrap,","#status-toast, .composer-wrap,").replace(".toast-dismiss",".protected-adoption-actions");document::eval(&script);},
                div{class:"toast-text",p{if let Some(message)=message{"{message}"}else{"Review saved drafts, then continue editing with recovery."}}}
                div{class:"protected-adoption-actions",
                    if pending || installed{button{id:"protected-adoption-check",r#type:"button",disabled:busy,onclick:move |_|controller::check(state,ui),"Check status"}}
                    button{id:"protected-adoption-reload",r#type:"button",disabled:busy,onclick:move |_|controller::reload(state,ui),"Reload saved drafts"}
                    if !enrolled && !installed{button{id:"protected-adoption-open",r#type:"button",disabled:busy,onclick:move |_|controller::open(state,ui),"Review draft"}}
                    if enrolled && !installed{button{id:"protected-adoption-adopt",r#type:"button",disabled:busy,onclick:move |_|adoption::start(state,ui),"Continue editing"}}
                    if failed{button{id:"protected-adoption-retry",r#type:"button",disabled:busy,onclick:move |_|resume::retry(state,ui),"Retry saving"}}
                    if installed && disconnected{button{id:"protected-adoption-resume",r#type:"button",disabled:busy || failed,onclick:move |_|resume::start(state,ui),"Resume editing"}}
                    if busy{button{id:"protected-adoption-cancel",r#type:"button",onclick:move |_|controller::cancel(ui),"Cancel waiting"}}
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
