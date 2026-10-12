use crate::{
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::{
    files::{FileKind, FileOwner, FileSort, FilesQuery},
    state::{file_summary, file_title, file_uploader_id},
};

fn reload(state: Signal<ShellState>, channel: Option<&str>, edit: impl FnOnce(&mut FilesQuery)) {
    let mut query = crate::files::visible_for(&state.peek(), channel)
        .map(|f| f.query.clone())
        .unwrap_or_default();
    query.channel = channel.map(str::to_owned);
    edit(&mut query);
    dioxus::core::spawn_forever(crate::files::load(state, query, 1));
}

#[component]
pub(crate) fn FilesPanel(
    state: Signal<ShellState>,
    #[props(default)] channel: Option<String>,
) -> Element {
    let scope = use_signal(|| channel);
    let channel = scope.read();
    let channel = channel.as_deref();
    let mut text = use_signal(|| {
        crate::files::visible_for(&state.peek(), channel)
            .map(|f| f.query.text.clone())
            .unwrap_or_default()
    });
    let snapshot = state.read();
    let files = crate::files::visible_for(&snapshot, channel);
    let query = files
        .map(|f| f.query.clone())
        .unwrap_or_else(|| FilesQuery {
            channel: channel.map(str::to_owned),
            sort: if channel.is_some() {
                FileSort::Newest
            } else {
                FileSort::Viewed
            },
            ..Default::default()
        });
    let loading = files.is_some_and(|f| f.loading);
    let page = files.map(|f| f.page).unwrap_or(1).max(1);
    let page_count = files.map(|f| f.page_count).unwrap_or(1).max(1);
    let refresh = query.clone();
    let previous = query.clone();
    let next = query.clone();
    let retry = query.clone();
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    rsx! {
        section {class:"files-panel","aria-label":"Files",
            if channel.is_none() {
            header {class:"files-header",
                h2 {if channel.is_some() {"Shared files"} else {"Files"}}
                span {class:"spacer"}
                button {class:"later-refresh",disabled:loading,onclick:move |_|{dioxus::core::spawn_forever(crate::files::load(state,refresh.clone(),page));},"Refresh"}
            }
            }
            form {class:"files-search",onsubmit:move |event|{event.prevent_default();let value=text();reload(state,scope.peek().as_deref(),|q|q.text=value);},
                {icon(Icon::Search,"icon")}
                input {type:"search","aria-label":"Search files",placeholder:if channel.is_some() {"Search files in this conversation"} else {"Search files, or use in:channel and from:person"},value:"{text}",oninput:move |event|text.set(event.value())}
                button {type:"submit","Search"}
            }
            div {class:"files-controls",
                if channel.is_none() {
                div {class:"files-owner",role:"group","aria-label":"File ownership",
                    for owner in FileOwner::ALL {
                        button {class:if query.owner==owner {"active"}else{""},"aria-pressed":query.owner==owner,
                            onclick:move |_|reload(state,scope.peek().as_deref(),|q|q.owner=owner),"{owner.label()}"}
                    }
                }
                }
                div {class:"files-selects",
                    if channel.is_some() {
                        select {"aria-label":"File ownership",value:"{query.owner.label()}",onchange:move |event|{let value=event.value();if let Some(owner)=FileOwner::ALL.into_iter().find(|o|o.label()==value){reload(state,scope.peek().as_deref(),|q|q.owner=owner);}},
                            for owner in FileOwner::ALL {option {value:owner.label(),"{owner.label()}"}}
                        }
                    }
                    select {"aria-label":"File type",value:"{query.kind.label()}",onchange:move |event|{let value=event.value();if let Some(kind)=FileKind::ALL.into_iter().find(|k|k.label()==value){reload(state,scope.peek().as_deref(),|q|q.kind=kind);}},
                        for kind in FileKind::ALL {option {value:kind.label(),"{kind.label()}"}}
                    }
                    select {"aria-label":"Sort files",value:"{query.sort.label()}",onchange:move |event|{let value=event.value();if let Some(sort)=FileSort::ALL.into_iter().find(|s|s.label()==value){reload(state,scope.peek().as_deref(),|q|q.sort=sort);}},
                        for sort in FileSort::ALL {option {value:sort.label(),disabled:sort==FileSort::Viewed && (!query.text.trim().is_empty() || query.owner!=FileOwner::All || channel.is_some()),"{sort.label()}"}}
                    }
                }
            }
            div {class:"files-results","aria-busy":loading,
                if let Some(error)=files.and_then(|f|f.error.as_ref()) {
                    div {class:"later-error",role:"alert",p {"{error}"}button {onclick:move |_|{dioxus::core::spawn_forever(crate::files::load(state,retry.clone(),page));},"Retry"}}
                }
                if loading {p {class:"files-status",role:"status","Loading files…"}}
                if let Some(files)=files {
                    if files.loaded && files.items.is_empty() && !loading {
                        div {class:"files-empty",h3 {"No files found"}p {"Try a different search or file type."}}
                    }
                    div {role:"list","aria-label":"File results",
                        for file in &files.items {
                            {
                                let title=file_title(file);let summary=file_summary(file);let id=file.id.as_deref().unwrap_or_default();
                                let uploader=file_uploader_id(file);
                                let author=uploader.map(|id|super_platinum_core::state::display_name(workspace.and_then(|ws|ws.users.get(id)),id)).unwrap_or_default();
                                let created=file.extra.get("created").and_then(|v|v.as_i64()).and_then(|t|super_platinum_core::state::date_key_for_ts(&t.to_string())).unwrap_or_default();
                                let thumbnail=file.preview().map(|p|snapshot.media.register_image(super_platinum_core::MediaAssetKind::Attachment,&p.url,p.mime)).filter(|id|snapshot.media.is_ready(id));
                                let external=matches!(file.filetype.as_deref(),Some("quip"|"canvas"|"list"|"lists"));
                                let open=file.clone();
                                rsx! {
                                    div {role:"listitem",key:"{id}",
                                        button {class:"files-row",onclick:move |_|crate::files::open(state,open.clone()),
                                            span {class:"files-thumbnail",
                                                if let Some(id)=thumbnail {super::media_image::MediaImage {state,id,media_epoch:snapshot.media_epoch,compact:true,loading:"lazy",fallback:icon(Icon::AttachFile,"icon")}}else{{icon(Icon::AttachFile,"icon")}}
                                            }
                                            span {class:"files-row-body",
                                                span {class:"files-title","{title}"}
                                                span {class:"files-meta", "{summary}", if !author.is_empty(){" · {author}"}}
                                            }
                                            span {class:"files-date","{created}"}
                                            if external {span {class:"files-open-hint","Open in Slack"}}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            footer {class:"files-pagination",
                span {role:"status",if let Some(files)=files {if files.loaded {"{files.total} " if files.total==1 {"file"} else {"files"} " · Page {page} of {page_count}"}}}
                span {class:"spacer"}
                button {disabled:loading || page<=1,onclick:move |_|{dioxus::core::spawn_forever(crate::files::load(state,previous.clone(),page.saturating_sub(1)));},"Previous"}
                button {disabled:loading || page>=page_count,onclick:move |_|{dioxus::core::spawn_forever(crate::files::load(state,next.clone(),page+1));},"Next"}
            }
        }
    }
}
