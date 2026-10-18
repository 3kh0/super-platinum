//! A reminder is a local wall-clock input, separate from the item's due date.
use super::*;
use super_platinum_core::slack_lists::ListReminderChange;

#[component]
pub(super) fn Reminder(
    state: Signal<ShellState>,
    view: crate::slack_lists::reminder::Presentation,
) -> Element {
    let serial = view.generation;
    let mut reading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let editable = view.editable && !reading();
    let due = view.due;
    rsx! {
        div {class:"slack-list-editor-shade"}
        section {id:"slack-list-reminder-editor",class:"slack-list-cell-editor",role:"dialog",
            "aria-modal":"true","aria-labelledby":"slack-list-reminder-title","aria-busy":view.pending||view.loading||reading(),
            "data-keyboard-dialog":"true","data-escape-local":"true","data-reminder-generation":serial,tabindex:"-1",
            onmounted:move |_| {dioxus::document::eval(&format!("{SCRIPT}\nmountListReminder({serial});"));},
            onkeydown:move |event|if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();crate::slack_lists::reminder::close(state,serial);},
            h3 {id:"slack-list-reminder-title","Remind me"}
            p {class:"slack-list-editor-item","{view.title}"}
            div {class:"slack-list-editor-body",
                if view.ready {
                    label {r#for:"slack-list-reminder-time","Date and time"}
                    input {id:"slack-list-reminder-time",r#type:"datetime-local",disabled:!editable,
                        "aria-describedby":"slack-list-reminder-help",
                        onmounted:move |_| {dioxus::document::eval(&format!("{SCRIPT}\ninitializeListReminder({serial},{due});"));},
                        oninput:move |_|error.set(None)}
                    p {id:"slack-list-reminder-help",class:"slack-list-editor-hint","Uses your local time. This does not change the item's due date."}
                }
                if view.loading {p {role:"status","Loading the current reminder…"}}
                if let Some(message)=view.message {p {class:"slack-list-editor-error",role:"status","{message}"}}
                if let Some(message)=error() {p {class:"slack-list-editor-error",role:"alert","{message}"}}
            }
            div {class:"slack-list-editor-actions",
                button {id:"slack-list-reminder-close",r#type:"button",onclick:move |_|crate::slack_lists::reminder::close(state,serial),if view.pending {"Close"} else {"Cancel"}}
                if view.can_check {button {id:"slack-list-reminder-check",r#type:"button",onclick:move |_|crate::slack_lists::reminder::check(state,serial),"Check result"}}
                if view.ready {
                    button {id:"slack-list-reminder-clear",r#type:"button",disabled:!view.can_clear||reading(),onclick:move |_|crate::slack_lists::reminder::save(state,serial,ListReminderChange::Clear),"Clear reminder"}
                    button {id:"slack-list-reminder-save",r#type:"button",class:"primary",disabled:!editable,
                        onclick:move |_| {
                            if reading() {return;}
                            reading.set(true);error.set(None);
                            spawn(async move {
                                // Keep the Eval alive through its reply. The host serial,
                                // detail, raw row and account guards are checked again by save.
                                let mut eval=dioxus::document::eval(&format!("{SCRIPT}\ndioxus.send(captureListReminder({serial}));"));
                                let result=tokio::time::timeout(std::time::Duration::from_secs(3),eval.recv::<Option<i64>>()).await;
                                if let Ok(mut pending)=reading.try_write(){*pending=false;}else{return;}
                                match result {
                                    Ok(Ok(Some(due)))=>crate::slack_lists::reminder::save(state,serial,ListReminderChange::Set(due)),
                                    _=>if let Ok(mut message)=error.try_write(){*message=Some("Choose a complete, valid local date and time.".into());},
                                }
                            });
                        },"Set reminder"}
                }
            }
        }
    }
}
const SCRIPT: &str = include_str!("reminder.js");
