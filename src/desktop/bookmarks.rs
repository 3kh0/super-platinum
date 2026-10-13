//! Native channel bookmarks. Mutations survive row unmounts and recheck stale edits.
use crate::{
    icons::{Icon, icon},
    scheduled::Context,
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::{
    bookmarks::{
        Bookmark, BookmarksState, Draft, Movement, descendants, placement, removal_matches, rows,
        valid_link,
    },
    slack::{Error, api},
};

pub fn visible<'a>(shell: &'a ShellState, channel: &str) -> Option<&'a BookmarksState> {
    shell
        .core
        .bookmarks
        .matches_scope(
            shell.core.account_epoch,
            shell.core.active_team.as_deref()?,
            channel,
        )
        .then_some(&shell.core.bookmarks)
}
fn current(ctx: &Context, shell: &ShellState, channel: &str, generation: u64) -> bool {
    ctx.current(shell)
        && shell
            .core
            .bookmarks
            .matches_scope(ctx.epoch, &ctx.session.team_id, channel)
        && shell.core.bookmarks.generation == generation
}
pub async fn load(mut state: Signal<ShellState>, channel: String) {
    let Some(ctx) = Context::capture(&state.peek()) else {
        return;
    };
    let generation = {
        let mut shell = state.write();
        shell
            .core
            .bookmarks
            .ensure_scope(ctx.epoch, &ctx.session.team_id, &channel);
        let Some(g) = shell.core.bookmarks.begin() else {
            return;
        };
        g
    };
    let response = api::fetch_bookmarks(&ctx.transport, &ctx.client, &ctx.session, &channel).await;
    let mut shell = state.write();
    if !current(&ctx, &shell, &channel, generation) {
        return;
    }
    let store = &mut shell.core.bookmarks;
    match response {
        Ok(items) => {
            store.apply(generation, items);
        }
        Err(_) => {
            store.loading = false;
            store.error = Some("Bookmarks could not refresh. Try again when connected.".into());
        }
    }
}
fn edit(mut state: Signal<ShellState>, channel: &str, base: Option<Bookmark>, remove: bool) {
    let mut shell = state.write();
    let Some(team) = shell.core.active_team.clone() else {
        return;
    };
    let epoch = shell.core.account_epoch;
    shell.core.bookmarks.ensure_scope(epoch, &team, channel);
    let store = &mut shell.core.bookmarks;
    if store.busy || store.loading || store.draft.is_some() {
        return;
    }
    let contents = base
        .as_ref()
        .filter(|b| b.folder() && remove)
        .map(|b| {
            descendants(&store.items, &b.id)
                .into_iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut draft = Draft::new(base, remove);
    draft.contents = contents;
    store.draft = Some(draft);
}
fn change(mut state: Signal<ShellState>, f: impl FnOnce(&mut Draft)) {
    let mut shell = state.write();
    let store = &mut shell.core.bookmarks;
    if store.busy {
        return;
    }
    if let Some(d) = store.draft.as_mut().filter(|d| !d.uncertain) {
        d.error = None;
        f(d);
    }
}
async fn save(mut state: Signal<ShellState>, channel: String) {
    let Some(edit) = visible(&state.peek(), &channel)
        .filter(|b| !b.busy)
        .and_then(|b| b.draft.clone())
        .filter(|d| d.valid() && !d.uncertain)
    else {
        return;
    };
    let Some(ctx) = Context::capture(&state.peek()) else {
        change(state, |d| {
            d.error = Some("Reconnect to save this bookmark. Your draft is still here.".into())
        });
        return;
    };
    let generation = {
        let mut shell = state.write();
        let store = &mut shell.core.bookmarks;
        store.generation = store.generation.wrapping_add(1);
        store.busy = true;
        store.loading = false;
        store.generation
    };
    let mut submitted = false;
    let preflight = if let Some(base) = &edit.base {
        match api::fetch_bookmarks(&ctx.transport, &ctx.client, &ctx.session, &channel).await {
            Ok(items) => {
                if !items.iter().any(|b| b.same_content(base)) {
                    Err(Error::Api("bookmark_changed".into()))
                } else if edit.remove && !removal_matches(&items, base, &edit.contents) {
                    Err(Error::Api("bookmark_changed".into()))
                } else if let Some(movement) = &edit.movement {
                    placement(&items, base, movement)
                        .map(Some)
                        .map_err(|e| Error::Api(e.into()))
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(e),
        }
    } else {
        Ok(None)
    };
    if !current(&ctx, &state.peek(), &channel, generation) {
        abandon(state, &ctx, &channel, generation, false);
        return;
    }
    let result = match preflight {
        Err(e) => Err(e),
        Ok(target) => {
            let request = if let Some(target) = target {
                api::bookmark_reorder(
                    &ctx.client,
                    &ctx.session,
                    &channel,
                    edit.base.as_ref().unwrap(),
                    &target,
                )
            } else if edit.remove {
                edit.base
                    .as_ref()
                    .map(|b| {
                        if b.folder() {
                            api::bookmark_folder_remove(&ctx.client, &ctx.session, &channel, &b.id)
                        } else {
                            api::bookmark_remove(&ctx.client, &ctx.session, &channel, &b.id)
                        }
                    })
                    .ok_or_else(|| Error::Api("invalid_bookmark".into()))
            } else if edit.folder {
                api::bookmark_folder_save(
                    &ctx.client,
                    &ctx.session,
                    &channel,
                    edit.base.as_ref().map(|b| b.id.as_str()),
                    &edit.title,
                )
            } else {
                api::bookmark_save(
                    &ctx.client,
                    &ctx.session,
                    &channel,
                    edit.base.as_ref().map(|b| b.id.as_str()),
                    &edit.title,
                    &edit.link,
                )
            };
            match request {
                Ok(r) => {
                    submitted = true;
                    ctx.transport.execute(r).await.map(|_| ())
                }
                Err(e) => Err(e),
            }
        }
    };
    let success = {
        let mut shell = state.write();
        if !current(&ctx, &shell, &channel, generation) {
            drop(shell);
            abandon(state, &ctx, &channel, generation, submitted);
            return;
        }
        let store = &mut shell.core.bookmarks;
        store.busy = false;
        match result {
            Ok(()) => {
                store.draft = None;
                shell.show_toast(if edit.remove {
                    "Bookmark removed"
                } else {
                    "Bookmark saved"
                });
                true
            }
            Err(e) => {
                if let Some(d) = store.draft.as_mut() {
                    let changed = matches!(&e,Error::Api(code) if matches!(code.as_str(),"bookmark_changed"|"invalid_destination"));
                    d.uncertain = submitted || changed;
                    d.error=Some(if changed{"This bookmark changed or was removed elsewhere. Cancel and refresh before editing it again."}else if submitted{"Could not confirm the change. Your draft is still here. Refresh to check the saved bookmarks before trying again."}else{"Could not check the current bookmark. Reconnect and try again; your draft is still here."}.into());
                }
                false
            }
        }
    };
    if success || submitted {
        load(state, channel).await;
    }
}

#[component]
pub fn BookmarksPanel(mut state: Signal<ShellState>, channel: String) -> Element {
    let scope = use_signal(|| channel);
    let snapshot = state.read();
    let store = visible(&snapshot, &scope.read());
    let loading = store.is_some_and(|b| b.loading);
    let busy = store.is_some_and(|b| b.busy);
    let draft = store.and_then(|b| b.draft.as_ref());
    let ordered = store
        .map(|b| rows(&b.items, &b.collapsed))
        .unwrap_or_default();
    rsx! {
        section {class:"bookmarks-panel","aria-label":"Channel bookmarks",
            header {class:"files-header",h2 {"Bookmarks"}span {class:"spacer"}
                button {class:"later-refresh",disabled:busy||loading||draft.is_some(),onclick:move |_|edit(state,&scope(),None,false),"Add bookmark"}
                button {class:"later-refresh",disabled:busy||loading||draft.is_some(),onclick:move |_|new_folder(state,&scope()),"New folder"}
                button {class:"later-refresh",disabled:busy||loading,onclick:move |_|{dioxus::core::spawn_forever(load(state,scope()));},"Refresh"}
            }
            div {class:"bookmarks-content","aria-busy":loading||busy,
                if let Some(d)=draft {
                    form {class:"bookmark-editor",onsubmit:move |event|{event.prevent_default();dioxus::core::spawn_forever(save(state,scope()));},
                        h3 {if d.movement.is_some() {"Move bookmark"} else if d.remove {if d.folder {"Remove folder?"} else {"Remove bookmark?"}} else if d.folder {if d.base.is_some(){"Rename folder"}else{"New folder"}} else if d.base.is_some() {"Edit bookmark"} else {"Add bookmark"}}
                        if let Some(movement)=&d.movement {
                            p {"Choose where “{d.title}” belongs."}
                            label {r#for:"bookmark-destination","Folder"}
                            select {id:"bookmark-destination",value:"{movement.parent}",disabled:busy||d.uncertain,
                                onchange:move |e|change(state,|d|if let Some(m)=d.movement.as_mut(){m.parent=e.value();m.before.clear();}),
                                option {value:"","Top level"}
                                if !d.folder {for folder in store.into_iter().flat_map(|s|&s.items).filter(|b|b.folder()) {option {value:"{folder.id}","{folder.title()}"}}}
                            }
                            label {r#for:"bookmark-position","Position"}
                            select {id:"bookmark-position",value:"{movement.before}",disabled:busy||d.uncertain,onchange:move |e|change(state,|d|if let Some(m)=d.movement.as_mut(){m.before=e.value();}),
                                option {value:"","At the end"}
                                for sibling in store.into_iter().flat_map(|s|&s.items).filter(|b|b.parent_id.as_deref().unwrap_or("")==movement.parent&&d.base.as_ref().is_none_or(|base|base.id!=b.id)) {
                                    option {value:"{sibling.id}","Before {sibling.title()}"}
                                }
                            }
                        } else
                        if d.remove {
                            if d.folder {p {"Remove “{d.title}” and its {d.contents.len()} contained bookmarks from this channel? Linked documents and websites will remain available."}} else {p {"Remove “{d.title}” from this channel? The linked content will remain available."}}
                        } else {
                            label {r#for:"bookmark-title","Name"}
                            input {id:"bookmark-title",initial_value:"{d.title}",disabled:busy||d.uncertain,oninput:move |e|change(state,|d|d.title=e.value())}
                            if !d.folder {label {r#for:"bookmark-link","Link"}
                            input {id:"bookmark-link",r#type:"url",initial_value:"{d.link}",placeholder:"https://example.com",disabled:busy||d.uncertain,oninput:move |e|change(state,|d|d.link=e.value())}}
                        }
                        if let Some(error)=&d.error {p {role:"alert","{error}"}}
                        div {class:"bookmark-editor-actions",
                            button {r#type:"button",disabled:busy,onclick:move |_|state.write().core.bookmarks.draft=None,"Cancel"}
                            button {r#type:"submit",disabled:busy||d.uncertain||!d.valid(),if busy {"Saving…"} else if d.movement.is_some(){"Move bookmark"} else if d.remove {if d.folder{"Remove folder"}else{"Remove bookmark"}} else if d.folder {"Save folder"} else {"Save bookmark"}}
                        }
                    }
                }
                if let Some(error)=store.and_then(|b|b.error.as_ref()){p {class:"files-status",role:"alert","{error}"}}
                if loading {p {class:"files-status",role:"status","Loading bookmarks…"}}
                if store.is_some_and(|b|b.loaded&&b.items.is_empty())&&!loading {
                    div {class:"files-empty",h3 {"Keep useful links close"}p {"Add the documents, dashboards and references this channel comes back to."}}
                }
                div {role:"list","aria-label":"Bookmarks",
                    for (bookmark,depth) in ordered {
                        {
                            let title=bookmark.title().to_owned();let url=bookmark.link.clone().unwrap_or_default();
                            let edit_item=bookmark.clone();let remove_item=bookmark.clone();let move_item=bookmark.clone();let folder_id=bookmark.id.clone();let folder=bookmark.folder();let expanded=store.is_some_and(|s|!s.collapsed.contains(&bookmark.id));
                            let parent=bookmark.parent_id.as_deref().and_then(|id|store.and_then(|s|s.items.iter().find(|b|b.id==id))).map(|b|b.title().to_owned());
                            rsx! {
                                div {class:"bookmark-row",role:"listitem",key:"{bookmark.id}",style:"margin-left:{depth.min(4)*16}px",
                                    button {class:"bookmark-open",disabled:!folder&&!valid_link(&url),title:"{url}","aria-expanded":if folder{Some(expanded.to_string())}else{None},onclick:move |_|{if folder{let mut shell=state.write();let set=&mut shell.core.bookmarks.collapsed;if !set.remove(&folder_id){set.insert(folder_id.clone());}}else if crate::media::open_external(&url).is_err(){state.write().show_toast("This bookmark could not be opened.");}},
                                        {icon(if folder{Icon::ChevronRight}else{Icon::Bookmark},"icon")}
                                        span {class:"files-row-body",span {class:"files-title","{title}"}
                                            span {class:"files-meta",if let Some(parent)=parent {"{parent} · "}if matches!(bookmark.kind.as_deref(),Some("folder"|"local_folder")) {"Folder"} else {"{bookmark.link.as_deref().unwrap_or(\"Open this item in Slack\")}"}}
                                        }
                                    }
                                    if bookmark.editable() {
                                        button {class:"bookmark-action",disabled:busy||loading||draft.is_some(),"aria-label":"Edit bookmark {title}",onclick:move |_|edit(state,&scope(),Some(edit_item.clone()),false),"Edit"}
                                        button {class:"bookmark-action",disabled:busy||loading||draft.is_some(),"aria-label":"Move bookmark {title}",onclick:move |_|organize(state,&scope(),move_item.clone()),"Move"}
                                        button {class:"bookmark-action",disabled:busy||loading||draft.is_some(),"aria-label":"Remove bookmark {title}",onclick:move |_|edit(state,&scope(),Some(remove_item.clone()),true),{icon(Icon::Close,"icon sm")}}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn abandon(
    mut state: Signal<ShellState>,
    ctx: &Context,
    channel: &str,
    generation: u64,
    submitted: bool,
) {
    let mut shell = state.write();
    if shell.core.account_epoch != ctx.epoch
        || shell.core.active_team.as_deref() != Some(&ctx.session.team_id)
    {
        return;
    }
    let store = &mut shell.core.bookmarks;
    if !store.matches_scope(ctx.epoch, &ctx.session.team_id, channel)
        || store.generation != generation
    {
        return;
    }
    store.busy = false;
    if let Some(d) = store.draft.as_mut() {
        d.uncertain = submitted;
        d.error=Some("The connection changed. Refresh bookmarks before continuing; your draft is still here.".into());
    }
}

pub fn event_channel(
    shell: &ShellState,
    team: &str,
    generation: u64,
    event: &super_platinum_core::slack::events::RtEvent,
) -> Option<String> {
    use super_platinum_core::slack::events::RtEvent;
    if shell.core.active_team.as_deref() != Some(team)
        || shell.core.workspaces.get(team)?.rt_generation != generation
        || shell.channel_tab != crate::state::ChannelTab::Bookmarks
        || shell.core.bookmarks.busy
    {
        return None;
    }
    let RtEvent::Unknown(raw) = event else {
        return None;
    };
    if !matches!(
        raw.kind.as_str(),
        "bookmark_added" | "bookmark_updated" | "bookmark_removed"
    ) {
        return None;
    }
    let channel = raw.rest.get("channel")?.as_str()?;
    (shell.core.active_channel.as_deref() == Some(channel)).then(|| channel.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn realtime_bookmark_refresh_rejects_old_connections_and_other_channels() {
        use super_platinum_core::slack::events::{RawEvent, RtEvent};
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.channel_tab = crate::state::ChannelTab::Bookmarks;
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
        let event = RtEvent::Unknown(
            serde_json::from_value::<RawEvent>(
                serde_json::json!({"type":"bookmark_updated","channel":"C2","bookmark":"Bk1"}),
            )
            .unwrap(),
        );
        assert_eq!(
            event_channel(&shell, "T1", 7, &event).as_deref(),
            Some("C2")
        );
        assert!(event_channel(&shell, "T1", 6, &event).is_none());
        shell.core.active_channel = Some("C1".into());
        assert!(event_channel(&shell, "T1", 7, &event).is_none());
        shell.core.active_channel = Some("C2".into());
        shell.core.bookmarks.busy = true;
        assert!(event_channel(&shell, "T1", 7, &event).is_none());
    }
}

fn new_folder(mut state: Signal<ShellState>, channel: &str) {
    if visible(&state.peek(), channel).is_some_and(|s| s.busy || s.loading || s.draft.is_some()) {
        return;
    }
    edit(state, channel, None, false);
    if let Some(d) = state.write().core.bookmarks.draft.as_mut() {
        d.folder = true;
    }
}
fn organize(mut state: Signal<ShellState>, channel: &str, bookmark: Bookmark) {
    if visible(&state.peek(), channel).is_some_and(|s| s.busy || s.loading || s.draft.is_some()) {
        return;
    }
    let parent = bookmark.parent_id.clone().unwrap_or_default();
    edit(state, channel, Some(bookmark), false);
    if let Some(d) = state.write().core.bookmarks.draft.as_mut() {
        d.movement = Some(Movement {
            parent,
            before: String::new(),
        });
    }
}
