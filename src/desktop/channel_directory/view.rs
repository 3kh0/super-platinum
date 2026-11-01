use super::*;
use crate::icons::{Icon, icon};

fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{e000}' | '\u{e001}'))
        .collect()
}
fn members(count: u64) -> String {
    format!("{count} {}", if count == 1 { "member" } else { "members" })
}

pub(crate) fn dialog(mut state: Signal<ShellState>) -> Element {
    let s = state.read();
    let d = &s.channel_directory;
    let entry = d.entry.clone().filter(|e| e.current(&s));
    let origin = d.origin.clone();
    let busy = d.busy();
    let loaded = d.loaded.clone().filter(|p| p.entry.current(&s));
    let presentation = std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
        && s.core.session.is_none()
        && s.core.transport.is_none()
        && s.core.active_account.is_none()
        && d.entry.is_none();
    let page = loaded
        .as_ref()
        .map(|p| &p.page)
        .or_else(|| presentation.then_some(d.presentation.as_ref()).flatten());
    let valid = loaded.as_ref().is_some_and(|p| p.current(&s));
    let next = valid && page.is_some_and(|p| !p.items.is_empty() && p.page < p.pages);
    let previous =
        valid && page.is_some_and(|p| p.page > 1 && d.cursors.contains_key(&(p.page - 1)));
    let sort = match d.sort() {
        Sort::Name => "name",
        Sort::NameReverse => "name-reverse",
        Sort::MostMembers => "members",
        Sort::FewestMembers => "fewest-members",
        Sort::CreatedNewest => "created-newest",
        Sort::CreatedOldest => "created-oldest",
        Sort::Relevant => "relevant",
    };
    let revision = d.revision;
    let kind = match d.kind {
        Kind::Active => "active",
        Kind::Public => "public",
        Kind::Private => "private",
        Kind::Archived => "archived",
        Kind::Shared => "shared",
    };
    let selected = loaded
        .as_ref()
        .filter(|p| p.current(&s))
        .and_then(|p| {
            p.page
                .items
                .iter()
                .find(|i| Some(&i.id) == d.selected.as_ref())
        })
        .or_else(|| {
            presentation
                .then(|| {
                    page?
                        .items
                        .iter()
                        .find(|i| Some(&i.id) == d.presentation_selected.as_ref())
                })
                .flatten()
        });
    let open = selected.and_then(|item| {
        loaded
            .as_ref()
            .and_then(|page| super::open::capture(&s, page.clone(), revision, &item.id))
    });
    rsx! {
        div {class:"scrim",onclick:{let origin=origin.clone();move |_|close_local(state,&origin)}}
        section {id:"your-channels-dialog",class:"modal your-channels-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"your-channels-title",
            tabindex:"-1","data-keyboard-dialog":"true","data-dialog-kind":"other","data-dialog-generation":s.shortcut_generation,"data-escape-local":"true",
            onkeydown:{let origin=origin.clone();move |event|if event.key()==Key::Escape{event.prevent_default();event.stop_propagation();close_local(state,&origin);}},
            header {h2 {id:"your-channels-title","Your channels"}
                button {id:"your-channels-close",r#type:"button",class:"icon-btn","aria-label":"Close your channels",onclick:{let origin=origin.clone();move |_|close_local(state,&origin)}, {icon(Icon::Close,"icon sm")}}
            }
            form {id:"your-channels-form",class:"your-channels-controls",onsubmit:{let entry=entry.clone();move |event|{event.prevent_default();if let Some(entry)=&entry{entry.submit_input(state);}}},
                input {id:"your-channels-query",r#type:"search",value:d.edited.as_deref().unwrap_or(&d.draft).to_owned(),placeholder:"Search your channels","aria-label":"Search your channels",disabled:entry.is_none(),
                    oninput:{let entry=entry.clone();move |event|if let Some(entry)=&entry{entry.edit_input(state,event.value());}}
                }
                select {id:"your-channels-type","aria-label":"Channel type",value:kind,disabled:entry.is_none(),
                    onchange:{let entry=entry.clone();move |event|if let Some(entry)=&entry{
                        let kind=match event.value().as_str(){"active"=>Kind::Active,"public"=>Kind::Public,"private"=>Kind::Private,"archived"=>Kind::Archived,"shared"=>Kind::Shared,_=>return};
                        entry.change_kind(state,kind);entry.start(state,read::Mode::Search);
                    }},
                    option {value:"active","Active channels"} option {value:"public","Public channels"}
                    option {value:"private","Private channels"} option {value:"archived","Archived channels"}
                    option {value:"shared","Slack Connect channels"}
                }
                select {id:"your-channels-sort","aria-label":"Sort your channels",value:sort,disabled:entry.is_none(),
                    onchange:{let entry=entry.clone();move |event|if let Some(entry)=&entry{
                        let sort=match event.value().as_str(){"name"=>Sort::Name,"name-reverse"=>Sort::NameReverse,"members"=>Sort::MostMembers,"fewest-members"=>Sort::FewestMembers,"created-newest"=>Sort::CreatedNewest,"created-oldest"=>Sort::CreatedOldest,"relevant"=>Sort::Relevant,_=>return};
                        entry.change(state,None,Some(sort));entry.start(state,read::Mode::Search);
                    }},
                    if !d.draft.trim().is_empty(){option {value:"relevant","Most relevant"}}
                    option {value:"name","Name A–Z"} option {value:"name-reverse","Name Z–A"}
                    option {value:"created-newest","Newest channel"} option {value:"created-oldest","Oldest channel"}
                    option {value:"members","Most members"} option {value:"fewest-members","Fewest members"}
                }
                button {id:"your-channels-search",r#type:"submit",disabled:entry.is_none()||busy,"Search"}
                button {id:"your-channels-refresh",r#type:"button",disabled:entry.is_none()||busy,
                    onclick:{let entry=entry.clone();move |_|if let Some(entry)=&entry{entry.start(state,read::Mode::Refresh);}},"Refresh"}
            }
            if busy {p {class:"your-channels-status",role:"status","Loading your channels…"}}
            if let Some(error)=d.error {p {class:"your-channels-status",role:"status","{error}"}
                button {id:"your-channels-retry",class:"your-channels-retry",r#type:"button",disabled:busy||entry.is_none(),onclick:{let entry=entry.clone();move |_|if let Some(entry)=&entry{entry.start(state,read::Mode::Retry);}},"Retry"}
            }
            if entry.is_none() && d.entry.is_some(){p {class:"your-channels-status",role:"status","Close this dialog and reopen it for the current workspace."}}
            div {class:"your-channels-body",
                ul {class:"your-channels-results","aria-label":"Your channel results",
                    if let Some(page)=page {
                        for item in &page.items {
                            li {key:"your-channel-{item.id}",
                                button {id:"your-channel-{item.id}",r#type:"button",class:if d.selected.as_ref()==Some(&item.id){"your-channel-row selected"}else{"your-channel-row"},disabled:!valid||busy,
                                    onclick:{let loaded=loaded.clone();let id=item.id.clone();move |_|if let Some(loaded)=&loaded{loaded.select(state,revision,&id);}},
                                    strong {{plain(item.name.as_deref().unwrap_or(&item.id))}}
                                    if let Some(purpose)=item.purpose.as_deref(){span {class:"your-channel-purpose",{plain(purpose)}}}
                                    if let Some(count)=item.member_count{span {class:"your-channel-members",{members(count)}}}
                                }
                            }
                        }
                        if page.items.is_empty() && !busy {li {class:"your-channels-status","No matching channels."}}
                    } else if !busy && d.error.is_none(){li {class:"your-channels-status","Refresh to load your channels."}}
                }
                if let Some(item)=selected {
                    aside {class:"your-channel-details","aria-label":"Channel information",
                        h3 {{plain(item.name.as_deref().unwrap_or(&item.id))}}
                        if let Some(purpose)=item.purpose.as_deref(){p {{plain(purpose)}}}
                        if let Some(count)=item.member_count{p {{members(count)}}}
                        p {class:"your-channel-members","You’re a member"}
                        button {id:"your-channels-open",class:"your-channel-open",r#type:"button",disabled:open.is_none(),
                            onclick:{let open=open.clone();move |_|if let Some(open)=&open{open.start(state);}},"Open channel"}
                    }
                }
            }
            footer {class:"your-channels-paging",
                button {id:"your-channels-reset",r#type:"button",disabled:entry.is_none()||busy,
                    onclick:{let entry=entry.clone();move |_|if let Some(entry)=&entry{entry.reset(state,revision);}},"Reset filters"}
                button {id:"your-channels-previous",r#type:"button",disabled:!previous||busy,onclick:{let entry=entry.clone();move |_|if let Some(entry)=&entry{entry.start(state,read::Mode::Previous);}},"Previous"}
                span {if let Some(page)=page {"Page {page.page}"}}
                button {id:"your-channels-next",r#type:"button",disabled:!next||busy,onclick:{let entry=entry.clone();move |_|if let Some(entry)=&entry{entry.start(state,read::Mode::Next);}},"Next"}
            }
        }
    }
}

fn close_local(mut state: Signal<ShellState>, origin: &Arc<()>) {
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if s.overlay == Some(Overlay::ChannelDirectory)
        && Arc::ptr_eq(origin, &s.channel_directory.origin)
    {
        s.channel_directory.revision = s.channel_directory.revision.wrapping_add(1);
        s.channel_directory.selected = None;
        s.dismiss_overlay();
    }
}
