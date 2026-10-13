use crate::{
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::later::{LaterFilter, SavedAction, SavedKey};

#[component]
pub(crate) fn LaterPanel(state: Signal<ShellState>) -> Element {
    let mut creating = use_signal(|| {
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("later-new-reminder")
    });
    let mut reminder = use_signal(|| {
        if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("later-reminder") {
            state.peek().core.later.visible.first().cloned()
        } else {
            None::<SavedKey>
        }
    });
    let snapshot = state.read();
    let later = crate::later::visible_state(&snapshot);
    let filter = later.map(|later| later.filter).unwrap_or_default();
    let loading = later.is_some_and(|later| later.loading);
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    rsx! {
        aside {class:"list-panel later-panel", "aria-label":"Later",
            onmounted:move |_|{spawn(crate::later::file_fixture::native_check(state));},
            header {class:"list-panel-header",
                h2 {"Later"}
                span {class:"spacer"}
                button {class:"later-refresh",onclick:move |_|creating.set(true),"New reminder"}
                button {class:"later-refresh",disabled:loading,onclick:move |_|{spawn(crate::later::load(state,filter,false));},"Refresh"}
            }
            div {class:"later-tabs",role:"tablist","aria-label":"Saved item status",
                for tab in LaterFilter::ALL {
                    button {role:"tab","aria-selected":tab==filter,class:if tab==filter {"active"}else{""},
                        onclick:move |_|{spawn(crate::later::load(state,tab,false));},
                        "{tab.label()}"
                        if let Some(later)=later {span {"{later.counts.get(tab)}"}}
                    }
                }
            }
            div {class:"list-body later-body",
                crate::file_actions::later::RecoveryInventory {state}
                if let Some(error)=later.and_then(|later|later.error.as_ref()) {
                    div {class:"later-error",role:"alert",
                        p {"{error}"}
                        button {onclick:move |_|{spawn(crate::later::load(state,filter,false));},"Retry"}
                    }
                }
                if loading {p {class:"later-status",role:"status","Loading saved items…"}}
                if let Some(later)=later {
                    for key in &later.visible {
                        if let Some(item)=later.items.get(key) {
                            {
                                let description=item.description_message();
                                let message=if key.item_type=="reminder" {Some(&description)}else{later.hydrated.get(key)};
                                let mut title=if key.item_type=="reminder" {"Reminder".into()}else{workspace.and_then(|ws|ws.channels.get(&key.item_id).map(|channel|super_platinum_core::state::channel_display_name(ws,channel))).unwrap_or_else(||key.item_id.clone())};
                                let mut author=if key.item_type=="reminder" {String::new()}else{workspace.zip(message).map(|(ws,message)|super_platinum_core::state::message_author_name(ws,message)).unwrap_or_default()};
                                let mut preview=workspace.zip(message).map(|(ws,message)|super::rich::message_plain_text(&crate::message_vm::message_vm(ws,message,&snapshot.media))).unwrap_or_else(||{
                                    item.extra.get("text").and_then(|value|value.as_str()).map(str::to_owned).unwrap_or_else(||if key.item_type=="message" {"Message preview unavailable".into()}else{format!("Saved {}",key.item_type)})
                                });
                                let mut deleted=false;
                                if key.item_type=="file" {
                                    let file=later.files.get(&key.item_id);
                                    deleted=workspace.is_some_and(|ws|ws.deleted_files.contains(&key.item_id))
                                        || matches!(later.file_errors.get(&key.item_id),Some(super_platinum_core::later::FileHydrationFailure::Deleted));
                                    title=file.map(super_platinum_core::state::file_title).unwrap_or_else(||"Saved file".into());
                                    author=file.and_then(super_platinum_core::state::file_uploader_id).map(|id|super_platinum_core::state::display_name(workspace.and_then(|ws|ws.users.get(id)),id)).unwrap_or_default();
                                    preview=if deleted {"This file was deleted. You can remove it from Later.".into()}
                                        else if matches!(later.file_errors.get(&key.item_id),Some(super_platinum_core::later::FileHydrationFailure::Unavailable)) {"File unavailable. Open to try again, or remove it from Later.".into()}
                                        else {file.map(super_platinum_core::state::file_summary).unwrap_or_else(||"Open to load file details.".into())};
                                }
                                let busy=later.busy.contains(key);
                                let open_key=key.clone();
                                let done_key=key.clone();
                                let archive_key=key.clone();
                                let remove_key=key.clone();
                                let remind_key=key.clone();
                                let archived=item.filter()==LaterFilter::Archived;
                                let complete=item.filter()==LaterFilter::Completed;
                                let due=if item.date_due>1 && !archived && !complete {Some(item.date_due)}else{None};
                                rsx! {
                                    article {class:"later-item",key:"{key.item_type}-{key.item_id}-{key.ts}","aria-busy":busy,"data-saved-file":if key.item_type=="file"{key.item_id.clone()}else{String::new()},
                                        button {class:"later-entry",disabled:deleted,onclick:move |_|if open_key.item_type=="reminder" {reminder.set(Some(open_key.clone()))}else{crate::later::open(state,open_key.clone())},
                                            span {class:"later-channel","{title}"}
                                            if let Some(due)=due {
                                                span {class:if due<super_platinum_core::state::now_secs(){"later-due overdue"}else{"later-due"},
                                                    {icon(Icon::Schedule,"icon sm")}
                                                    "Due {super_platinum_core::state::format_ts_date_label(&due.to_string())} at {super_platinum_core::state::format_ts_hm(&due.to_string())}"
                                                }
                                            }
                                            if !author.is_empty() {strong {"{author}"}}
                                            p {"{preview}"}
                                        }
                                        div {class:"later-actions",
                                            button {disabled:busy,title:if complete {"Move to in progress"}else{"Mark complete"},
                                                onclick:move |_|crate::later::change(state,done_key.clone(),if complete {SavedAction::Restore}else{SavedAction::Complete}),
                                                {icon(Icon::Check,"icon sm")}
                                                if complete {"Restore"}else{"Complete"}
                                            }
                                            if !complete && !archived {
                                                button {disabled:busy,title:"Set reminder",onclick:move |_|reminder.set(Some(remind_key.clone())),{icon(Icon::Schedule,"icon sm")},"Remind"}
                                            }
                                            button {disabled:busy,onclick:move |_|crate::later::change(state,archive_key.clone(),if archived{SavedAction::Unarchive}else{SavedAction::Archive}),
                                                if archived {"Unarchive"}else{"Archive"}
                                            }
                                            button {disabled:busy,title:"Remove from Later","aria-label":"Remove from Later",onclick:move |_|crate::later::change(state,remove_key.clone(),SavedAction::Remove),{icon(Icon::Close,"icon sm")}}
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if later.loaded && later.visible.is_empty() && !loading && later.error.is_none() {
                        p {class:"list-empty",
                            if filter==LaterFilter::Saved {"Save messages and files to keep them here, then add a reminder or mark them complete."}
                            else if filter==LaterFilter::Completed {"Completed items will appear here."}
                            else {"Archived items will appear here."}
                        }
                    }
                    if later.next_cursor.is_some() {
                        button {class:"later-more",disabled:loading,onclick:move |_|{spawn(crate::later::load(state,filter,true));},"Load more"}
                    }
                }
            }
        }
        if let Some(key)=reminder() {
            if key.item_type=="reminder" {super::new_reminder::NewReminder {state,item:key,onclose:move |_|reminder.set(None)}}
            else {ReminderEditor {state,item:key,onclose:move |_|reminder.set(None)}}
        }
        if creating() {
            super::new_reminder::NewReminder {state,onclose:move |_|creating.set(false)}
        }
    }
}

#[component]
fn ReminderEditor(state: Signal<ShellState>, item: SavedKey, onclose: EventHandler) -> Element {
    let mut draft = use_signal(String::new);
    let mut error = use_signal(String::new);
    let hour = item.clone();
    let tomorrow = item.clone();
    let clear = item.clone();
    let current_due = state
        .peek()
        .core
        .later
        .items
        .get(&item)
        .map(|item| item.date_due)
        .filter(|due| *due > 1)
        .unwrap_or_else(|| super_platinum_core::state::now_secs() + 3600);
    rsx! {
        div {class:"scrim",onclick:move |_|onclose.call(())}
        section {class:"modal later-reminder",role:"dialog","aria-modal":"true","aria-labelledby":"reminder-title",
            onmounted:move |_|{spawn(async move {if let Some(value)=super::new_reminder::local_date_value(current_due).await {draft.set(value);}});},
            onkeydown:move |event| {if event.key()==Key::Escape {event.stop_propagation();onclose.call(());}},
            header {h2 {id:"reminder-title","Remind me"}
                button {"aria-label":"Close reminder",onclick:move |_|onclose.call(()),{icon(Icon::Close,"icon")}}
            }
            div {class:"reminder-presets",
                button {onclick:move |_|{
                    crate::later::change(state,hour.clone(),SavedAction::Reminder(super_platinum_core::state::now_secs()+3600));onclose.call(());
                },"In 1 hour"}
                button {onclick:move |_|{
                    let key=tomorrow.clone();
                    spawn(async move {
                        let mut result=dioxus::document::eval("const d=new Date(); d.setDate(d.getDate()+1); d.setHours(9,0,0,0); dioxus.send(Math.floor(d.getTime()/1000));");
                        if let Ok(due)=result.recv::<i64>().await {crate::later::change(state,key,SavedAction::Reminder(due));onclose.call(());}
                    });
                },"Tomorrow at 9 AM"}
            }
            label {r#for:"reminder-date","Choose a date and time"}
            input {id:"reminder-date",r#type:"datetime-local",value:"{draft}",oninput:move |event|draft.set(event.value()),onmounted:move |_|super::composer::focus_field_when_mounted("reminder-date")}
            if !error.read().is_empty() {p {role:"alert",class:"later-error","{error}"}}
            footer {
                button {onclick:move |_|{crate::later::change(state,clear.clone(),SavedAction::Reminder(0));onclose.call(());},"Clear reminder"}
                button {class:"primary",disabled:draft.read().is_empty(),onclick:move |_|{
                    let key=item.clone(); let value=draft();
                    spawn(async move {
                        let value=serde_json::to_string(&value).unwrap_or_default();
                        let mut result=dioxus::document::eval(&format!("const n=Date.parse({value});dioxus.send(Number.isFinite(n)?Math.floor(n/1000):0);"));
                        let due=result.recv::<i64>().await.unwrap_or(0);
                        if due<=super_platinum_core::state::now_secs() {error.set("Choose a time in the future.".into());return;}
                        crate::later::change(state,key,SavedAction::Reminder(due));onclose.call(());
                    });
                },"Set reminder"}
            }
        }
    }
}
