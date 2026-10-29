use crate::{
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::{
    files::{FileKind, FileOwner, FileSort, FilesQuery},
    state::{file_summary, file_title, file_uploader_id},
};

pub(crate) mod check;

fn reload(
    state: Signal<ShellState>,
    source: Option<crate::files::Source>,
    edit: impl FnOnce(&mut FilesQuery),
) {
    let Some(source) = source else {
        return;
    };
    if !source.is_current(state) {
        return;
    }
    let mut query = source.query.clone();
    edit(&mut query);
    source.submit(state, query);
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
    let source = crate::files::Source::capture(&snapshot, channel);
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    rsx! {
        section {class:"files-panel","aria-label":"Files",
            if channel.is_none() {
            header {class:"files-header",
                h2 {"Files"}
                span {class:"spacer"}
                button {class:"later-refresh",onclick:move |_|crate::canvas_create::open(state),"New canvas"}
                button {id:"files-refresh",class:"later-refresh",disabled:loading,onclick:{let source=source.clone();move |_|{if let Some(source)=source.clone(){source.refresh(state);}}},"Refresh"}
            }
            }
            if channel.is_none() {crate::file_actions::recovery_list::RecoveryList {state}super::file_share::SavedFileShares {state}}
            form {class:"files-search",onsubmit:{let source=source.clone();move |event|{
                event.prevent_default();let Some(source)=source.clone() else{return;};if !source.is_current(state){return;}
                let Ok(value)=text.try_peek().map(|value|(*value).clone()) else{return;};reload(state,Some(source),|q|q.text=value);
            }},
                {icon(Icon::Search,"icon")}
                input {type:"search","aria-label":"Search files",placeholder:if channel.is_some() {"Search files in this conversation"} else {"Search files, or use in:channel and from:person"},initial_value:"{text}",oninput:move |event|text.set(event.value())}
                button {id:"files-search-submit",type:"submit","Search"}
            }
            div {class:"files-controls",
                if channel.is_none() {
                div {class:"files-owner",role:"group","aria-label":"File ownership",
                    for owner in FileOwner::ALL {
                        button {class:if query.owner==owner {"active"}else{""},"aria-pressed":query.owner==owner,
                            onclick:{let source=source.clone();move |_|reload(state,source.clone(),|q|q.owner=owner)},"{owner.label()}"}
                    }
                }
                }
                div {class:"files-selects",
                    if channel.is_some() {
                        select {"aria-label":"File ownership",value:"{query.owner.label()}",onchange:{let source=source.clone();move |event|{let value=event.value();if let Some(owner)=FileOwner::ALL.into_iter().find(|o|o.label()==value){reload(state,source.clone(),|q|q.owner=owner);}}},
                            for owner in FileOwner::ALL {option {value:owner.label(),"{owner.label()}"}}
                        }
                    }
                    select {"aria-label":"File type",value:"{query.kind.label()}",onchange:{let source=source.clone();move |event|{let value=event.value();if let Some(kind)=FileKind::ALL.into_iter().find(|k|k.label()==value){reload(state,source.clone(),|q|q.kind=kind);}}},
                        for kind in FileKind::ALL {option {value:kind.label(),"{kind.label()}"}}
                    }
                    select {"aria-label":"Sort files",value:"{query.sort.label()}",onchange:{let source=source.clone();move |event|{let value=event.value();if let Some(sort)=FileSort::ALL.into_iter().find(|s|s.label()==value){reload(state,source.clone(),|q|q.sort=sort);}}},
                        for sort in FileSort::ALL {option {value:sort.label(),disabled:sort==FileSort::Viewed && (!query.text.trim().is_empty() || query.owner!=FileOwner::All || channel.is_some()),"{sort.label()}"}}
                    }
                }
            }
            div {class:"files-results","aria-busy":loading,
                if let Some(error)=files.and_then(|f|f.error.as_ref()) {
                    div {class:"later-error",role:"alert",p {"{error}"}if files.is_some_and(|files|files.loaded){p {"Showing the last loaded results."}}button {id:"files-retry",disabled:loading,onclick:{let source=source.clone();move |_|{if let Some(source)=source.clone(){source.retry(state);}}},"Retry"}}
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
                                let external=matches!(file.filetype.as_deref(),Some("list"|"lists"))
                                    && !super_platinum_core::slack_lists::is_list(file);
                                let open=file.clone();
                                let target=crate::file_actions::Target::new(&snapshot,file);
                                let epoch=target.epoch;let team=target.team.clone();
                                rsx! {
                                    div {class:"file-result",role:"listitem",key:"{epoch}-{team}-{id}","data-file-id":"{id}",
                                        button {class:"files-row",onclick:move |_|crate::files::open(state,open.clone()),
                                            span {class:"files-thumbnail",
                                                if let Some(id)=thumbnail {super::media_image::MediaImage {state,id,media_epoch:snapshot.media_epoch,compact:true,loading:"lazy",fallback:icon(Icon::AttachFile,"icon")}}else{{icon(Icon::AttachFile,"icon")}}
                                            }
                                            span {class:"files-row-body",
                                                span {class:"files-title",title:"{title}","{title}"}
                                                span {class:"files-meta",title:if author.is_empty() {summary.clone()} else {format!("{summary} · {author}")}, "{summary}", if !author.is_empty(){" · {author}"}}
                                            }
                                            span {class:"files-date","{created}"}
                                            if external {span {class:"files-open-hint","Open in Slack"}}
                                        }
                                        crate::file_actions::Actions {state,target}
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
                button {id:"files-previous",disabled:loading || !files.is_some_and(|files|files.loaded) || page<=1,onclick:{let source=source.clone();move |_|{if let Some(source)=source.clone(){source.page(state,page.saturating_sub(1));}}},"Previous"}
                button {id:"files-next",disabled:loading || !files.is_some_and(|files|files.loaded) || page>=page_count,onclick:{let source=source.clone();move |_|{if let Some(source)=source.clone(){source.page(state,page+1);}}},"Next"}
            }
        }
    }
}

#[cfg(test)]
mod resource_page_recovery;
