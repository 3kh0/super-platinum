use crate::{
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;

#[component]
pub(crate) fn PinsPanel(state: Signal<ShellState>, channel: String) -> Element {
    let mut visible = use_signal(|| 50usize);
    let scope = use_signal(|| channel);
    let snapshot = state.read();
    let pins = crate::pins::visible_state(&snapshot)
        .filter(|p| p.channel.as_deref() == Some(scope.read().as_str()));
    let loading = pins.is_some_and(|p| p.loading);
    let items = pins.map(|p| p.ordered_items()).unwrap_or_default();
    let total = items.len();
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    rsx! {
        section {class:"pins-panel","aria-label":"Pinned messages",
            header {class:"files-header",h2 {"Pinned messages"}span {class:"spacer"}
                button {class:"later-refresh",disabled:loading,onclick:move |_|{dioxus::core::spawn_forever(crate::pins::load(state,scope()));},"Refresh"}
            }
            div {class:"pins-results","aria-busy":loading,
                if let Some(error)=pins.and_then(|p|p.error.as_ref()) {
                    div {class:"later-error",role:"alert",p {"{error}"}button {onclick:move |_|{dioxus::core::spawn_forever(crate::pins::load(state,scope()));},"Retry"}}
                }
                if loading {p {class:"files-status",role:"status","Loading pins…"}}
                if pins.is_some_and(|p|p.loaded) && items.is_empty() && !loading {
                    div {class:"files-empty",h3 {"No pins yet"}p {"Pin important messages so everyone in this conversation can find them."}}
                }
                div {role:"list","aria-label":"Pins",
                    for item in items.into_iter().take(visible()) {
                        {
                            let key=item.key().unwrap_or_default();
                            let author=item.created_by.as_deref().map(|id|super_platinum_core::state::display_name(workspace.and_then(|w|w.users.get(id)),id)).unwrap_or_else(||"Someone".into());
                            let date=item.created.and_then(|t|super_platinum_core::state::date_key_for_ts(&t.to_string())).unwrap_or_default();
                            let ts=item.message.as_ref().and_then(|m|m.ts.clone());
                            let busy=ts.as_deref().is_some_and(|ts|pins.is_some_and(|p|p.is_busy(&scope.read(),ts)));
                            rsx! {
                                article {class:"pin-card",role:"listitem",key:"{key}",
                                    div {class:"pin-meta",
                                        {icon(Icon::Pin,"icon sm")}
                                        span {"Pinned by {author}" if !date.is_empty() {" · {date}"}}
                                        span {class:"spacer"}
                                        if let Some(ts)=ts {
                                            button {disabled:busy,onclick:move |_|crate::pins::change(state,scope(),ts.clone(),false),"Unpin"}
                                        }
                                    }
                                    if let Some(message)=&item.message {
                                        if let Some(workspace)=workspace {
                                            {
                                                let vm=crate::message_vm::message_vm(workspace,message,&snapshot.media);
                                                let source=message.clone();
                                                let surface=if message.thread_ts.as_ref().is_some_and(|t|Some(t)!=message.ts.as_ref()) {super::message::RowSurface::Thread}else{super::message::RowSurface::Timeline};
                                                rsx! {
                                                    {super::message::message_row(state,&snapshot,&vm,&scope.read(),None,surface)}
                                                    button {class:"pin-open",onclick:move |_|crate::pins::open(state,scope(),source.clone()),"Show in conversation"}
                                                }
                                            }
                                        }
                                    } else if let Some(file)=&item.file {
                                        {
                                            let file=file.clone();let title=super_platinum_core::state::file_title(&file);
                                            rsx! {button {class:"pin-open",onclick:move |_|crate::files::open(state,file.clone()),"{title}"}}
                                        }
                                        if let Some(comment)=item.comment.as_ref().and_then(|c|c.get("comment")).and_then(|v|v.as_str()) {p {class:"pin-legacy-comment","{comment}"}}
                                    }
                                }
                            }
                        }
                    }
                }
                if total>visible() {
                    button {class:"pin-more",onclick:move |_|visible.set(visible().saturating_add(50)),"Show more pins"}
                }
            }
        }
    }
}
