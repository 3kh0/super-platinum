use crate::{
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;

/// A local datetime field uses the WebView's timezone, including DST at the
/// selected date. Do not reuse today's UTC offset for tomorrow's reminder.
pub(super) async fn local_date_value(seconds: i64) -> Option<String> {
    let mut eval = dioxus::document::eval(&format!(
        "const d=new Date({seconds}*1000);const pad=n=>String(n).padStart(2,'0');dioxus.send(`${{d.getFullYear()}}-${{pad(d.getMonth()+1)}}-${{pad(d.getDate())}}T${{pad(d.getHours())}}:${{pad(d.getMinutes())}}`);"
    ));
    eval.recv::<String>().await.ok()
}

use super_platinum_core::later::SavedKey;

fn close_editor(onclose: EventHandler) {
    dioxus::document::eval("window.superPlatinumRichEditor?.restoreFocus('reminder-description');");
    onclose.call(());
}

#[component]
pub(super) fn NewReminder(
    state: Signal<ShellState>,
    item: Option<SavedKey>,
    onclose: EventHandler,
) -> Element {
    let initial = use_hook(|| {
        item.as_ref()
            .and_then(|key| state.peek().core.later.items.get(key).cloned())
    });
    let initial_blocks=initial.as_ref().map(|item|item.description.clone()).unwrap_or_else(|| {
        if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref()==Ok("later-new-reminder") {
            vec![serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Review the release notes","style":{"bold":true}},{"type":"text","text":"\nCheck keyboard shortcuts and attachments."}]}]})]
        } else {vec![]}
    });
    let existing_due = initial.as_ref().map(|item| item.date_due).unwrap_or(0);
    let editing = item.is_some();
    let mut date = use_signal(String::new);
    let mut original_date = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let title = if editing {
        "Edit reminder"
    } else {
        "New reminder"
    };
    let mount_blocks = serde_json::to_string(&initial_blocks).unwrap();
    let mount_labels = {
        let shell = state.peek();
        let mut labels = std::collections::BTreeMap::new();
        if let Some(workspace) = shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))
        {
            fn collect(
                value: &serde_json::Value,
                workspace: &super_platinum_core::state::Workspace,
                labels: &mut std::collections::BTreeMap<String, String>,
            ) {
                if let Some(id) = value.get("user_id").and_then(|value| value.as_str()) {
                    labels.insert(id.into(), format!("@{}", workspace.display_name(id)));
                }
                if let Some(id) = value.get("channel_id").and_then(|value| value.as_str()) {
                    if let Some(channel) = workspace.channels.get(id) {
                        labels.insert(
                            id.into(),
                            format!(
                                "#{}",
                                super_platinum_core::state::channel_display_name(
                                    workspace, channel
                                )
                            ),
                        );
                    }
                }
                if let Some(children) = value.get("elements").and_then(|value| value.as_array()) {
                    for child in children {
                        collect(child, workspace, labels);
                    }
                }
            }
            for block in &initial_blocks {
                collect(block, workspace, &mut labels);
            }
        }
        serde_json::to_string(&labels).unwrap()
    };
    rsx! {
        div {class:"scrim",onclick:move |_|if !saving(){close_editor(onclose)}}
        section {class:"modal later-reminder",role:"dialog","aria-modal":"true","aria-labelledby":"new-reminder-title",
            onkeydown:move |event|if event.key()==Key::Escape {event.stop_propagation();if !saving(){close_editor(onclose);}},
            onmounted:move |_|{
                spawn(async move {
                    if existing_due>1 || !editing {
                        if let Some(value)=local_date_value(if existing_due>1 {existing_due}else{super_platinum_core::state::now_secs()+3600}).await {original_date.set(value.clone());if date.peek().is_empty(){date.set(value);}}
                    }
                });
            },
            header {
                h2 {id:"new-reminder-title","{title}"}
                button {"aria-label":"Close reminder",disabled:saving(),onclick:move |_|close_editor(onclose),{icon(Icon::Close,"icon")}}
            }
            super::rich_editor::RichEditor {id:"reminder-description",label:"Description",blocks:mount_blocks,labels:mount_labels,disabled:saving()}
            label {r#for:"new-reminder-date","Date and time"}
            input {id:"new-reminder-date",r#type:"datetime-local",value:"{date}",disabled:saving(),oninput:move |event|date.set(event.value())}
            if editing {p {class:"reminder-hint","Leave the date empty to keep this reminder without a due time."}}
            if !error.read().is_empty() {p {role:"alert",class:"later-error","{error}"}}
            footer {
                button {disabled:saving(),onclick:move |_|close_editor(onclose),"Cancel"}
                button {class:"primary",disabled:saving()||(!editing&&date.read().is_empty()),onclick:move |_|{
                    saving.set(true);error.set(String::new());
                    let value=date();let unchanged_date=editing&&value==original_date();let item=item.clone();
                    let scope={let shell=state.peek();(shell.core.account_epoch,shell.core.active_team.clone())};
                    spawn(async move {
                        let value=serde_json::to_string(&value).unwrap_or_default();
                        let mut eval=dioxus::document::eval(&format!("try{{const date={value};const n=Date.parse(date);dioxus.send({{due:date===''?0:(Number.isFinite(n)?Math.floor(n/1000):-1),description:window.superPlatinumRichEditor.read('reminder-description'),text:document.getElementById('reminder-description').textContent}});}}catch(error){{dioxus.send({{error:String(error)}});}}"));
                        let result=eval.recv::<serde_json::Value>().await.unwrap_or_default();
                        let due=if unchanged_date {existing_due}else{result.get("due").and_then(|v|v.as_i64()).unwrap_or(-1)};
                        if due<0 || ((!editing || (due!=existing_due && due!=0)) && due<=super_platinum_core::state::now_secs()) {
                            saving.set(false);error.set("Choose a time in the future.".into());return;
                        }
                        if result.get("text").and_then(|v|v.as_str()).is_none_or(|text|text.trim().is_empty()) {
                            saving.set(false);error.set("Add a description for this reminder.".into());return;
                        }
                        let description=result.get("description").and_then(|v|v.as_array()).cloned().unwrap_or_default();
                        if (state.peek().core.account_epoch,state.peek().core.active_team.clone())!=scope {return;}
                        crate::later::save_reminder(state,item,description,due,move |result|{
                            let Ok(mut busy)=saving.try_write() else {return;};*busy=false;drop(busy);
                            match result {Ok(())=>close_editor(onclose),Err(message)=>error.set(message)}
                        });
                    });
                },if saving(){"Saving…"}else if editing{"Save changes"}else{"Create reminder"}}
            }
        }
    }
}
