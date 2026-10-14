use super::*;
use crate::icons::{Icon, icon};
pub(crate) fn view(state: Signal<ShellState>, s: &ShellState) -> Element {
    let dialog = s
        .bulk_read
        .dialog
        .as_ref()
        .filter(|d| d.context.visit_current(s))
        .map(|d| d.id);
    let owner = Owner::capture(s);
    let entry = owner
        .as_ref()
        .and_then(|o| s.bulk_read.entries.get(o))
        .filter(|e| !e.hidden);
    let notice = entry.and_then(|e| e.notice.as_ref());
    let sending = entry.is_some_and(|e| {
        e.operation
            .as_ref()
            .is_some_and(|o| o.phase == Phase::Sending)
    });
    let uncertain = entry.is_some_and(|e| {
        e.operation
            .as_ref()
            .is_some_and(|o| o.phase == Phase::Unknown)
    });
    let refreshing = entry.is_some_and(|e| e.reconciling.is_some());
    let undo = notice.is_some_and(|n| {
        n.receipt.is_some() && n.context.current(s) && n.started.elapsed() < n.duration
    });
    let message = entry.and_then(|e| e.message.clone());
    let pref = entry
        .and_then(|e| e.preference.as_ref())
        .and_then(|p| p.message.clone())
        .or_else(|| entry.and_then(|e| e.preference_message.clone()));
    let show = entry.is_some_and(|e| {
        e.operation.is_some()
            || e.notice.is_some()
            || e.reconciling.is_some()
            || e.refresh_failed
            || e.preference.is_some()
            || e.preference_message.is_some()
            || e.message.is_some()
    });
    rsx! {
        style { {include_str!("ui.css")} }
        if let Some(id)=dialog { BulkDialog {key:"bulk-dialog-{id}",state,id} }
        if show && dialog.is_none() {
            div {id:"bulk-read-status",class:"toast bulk-read-status",role:"status",
                onmounted:move |_|{
                    let script=include_str!("../../view/toast_position.js")
                        .replace("'status-toast'","'bulk-read-status'")
                        .replace(".composer-wrap,", "#status-toast, .composer-wrap,");
                    dioxus::document::eval(&script);
                },
                div {class:"toast-text",
                    if let Some(message)=message {p {"{message}"}}
                    if let Some(pref)=pref {p {class:"scheduled-hint","{pref}"}}
                    if refreshing {p {class:"scheduled-hint","Refreshing current read state…"}}
                }
                div {class:"bulk-read-actions",
                    if undo {button {id:"bulk-read-undo",r#type:"button",onclick:move |_|super::undo(state),"Undo"}}
                    if uncertain || entry.is_some_and(|e|e.refresh_failed) {
                        button {id:"bulk-read-refresh",r#type:"button",disabled:refreshing||sending,onclick:move |_|super::refresh(state),"Refresh"}
                    }
                    button {id:"bulk-read-dismiss",r#type:"button",class:"toast-dismiss","aria-label":"Dismiss read status",onclick:move |_|dismiss(state),{icon(Icon::Close,"icon sm")}}
                }
            }
        }
    }
}
#[component]
fn BulkDialog(mut state: Signal<ShellState>, id: u64) -> Element {
    let initial_focus = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(true)));
    let phase = use_memo(move || {
        state
            .read()
            .bulk_read
            .dialog
            .as_ref()
            .filter(|d| d.id == id)
            .map(|d| d.phase)
    });
    use_effect(move || {
        let initial = initial_focus.replace(false);
        let target = if phase() == Some(DialogPhase::Confirm) {
            "bulk-read-confirm-submit"
        } else {
            "bulk-read-cancel"
        };
        dioxus::document::eval(&format!(
            r#"requestAnimationFrame(()=>{{const d=document.getElementById('bulk-read-confirm');if(d?.dataset.dialogGeneration!=='{id}')return;const top=[...document.querySelectorAll('[data-keyboard-dialog]')].filter(x=>x.getClientRects().length&&getComputedStyle(x).visibility!=='hidden').at(-1);if(top!==d)return;const e=document.getElementById('{target}'),a=document.activeElement;if(e&&!e.matches(':disabled')&&({initial}||a===document.body||a===d))e.focus({{preventScroll:true}});}});"#
        ));
    });
    let shell = state.read();
    let Some(d) = shell
        .bulk_read
        .dialog
        .as_ref()
        .filter(|d| d.id == id && d.context.visit_current(&shell))
    else {
        return rsx! {};
    };
    let confirm = d.phase == DialogPhase::Confirm;
    let loading = d.phase == DialogPhase::Loading;
    let disabled = preference_pending(&shell, &d.context.owner) || preference::cooling(&shell);
    let checked = d.dont_ask;
    let error = d.error.clone();
    rsx! {
        div {class:"scrim",onclick:move |_|cancel(state,id)}
        section {id:"bulk-read-confirm",class:"modal button-confirmation bulk-read-confirm",role:"dialog",
            "aria-modal":"true","aria-labelledby":"bulk-read-title","aria-describedby":"bulk-read-body",tabindex:"-1",
            "data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":id,"data-escape-local":"true",
            onkeydown:move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();cancel(state,id);},
            header {h2 {id:"bulk-read-title","Clear all unreads?"}
                button {id:"bulk-read-close",r#type:"button",class:"icon-btn","aria-label":"Close confirmation",onclick:move |_|cancel(state,id),{icon(Icon::Close,"icon")}}
            }
            div {id:"bulk-read-body",class:"button-confirmation-body",
                if confirm {
                    p {"This will mark all unread messages and notifications in this workspace as read."}
                    label {class:"bulk-read-checkbox",
                        input {id:"bulk-read-dont-ask",r#type:"checkbox",checked,disabled,
                            onchange:move |event|{if let Some(d)=state.write().bulk_read.dialog.as_mut().filter(|d|d.id==id&&d.phase==DialogPhase::Confirm){d.dont_ask=event.checked();}}
                        }
                        span {"Don’t ask me again, please"}
                    }
                    if disabled {p {class:"scheduled-hint","The confirmation preference is pending or temporarily unavailable. You can still clear this workspace."}}
                } else if loading {p {role:"status","Loading the confirmation preference…"}}
                else if let Some(error)=error {p {role:"status","{error}"}}
            }
            form {onsubmit:move |event|{event.prevent_default();super::confirm(state,id);},
                footer {
                    button {id:"bulk-read-cancel",r#type:"button",onclick:move |_|cancel(state,id),"Cancel"}
                    if confirm {button {id:"bulk-read-confirm-submit",r#type:"submit",class:"button-confirmation-submit","Clear all unreads"}}
                    else {button {id:"bulk-read-pref-retry",r#type:"button",disabled:loading||preference::cooling(&shell),onclick:move |_|preference::retry(state,id),"Check again"}}
                }
            }
        }
    }
}
