//! Channel-local followed discussions; original reader controls and inert rows.
use crate::{
    channel_tabs::{self, threads},
    state::{MessageVm, ShellState},
};
use dioxus::prelude::*;
struct Item {
    root: MessageVm,
    replies: Vec<MessageVm>,
    earlier: bool,
    previous: Option<threads::previous::Action>,
    show_previous: bool,
}
/// Same-visit UI wakes keep this key; departure/reentry renews the original
/// mount receipt instead of allowing a retained hook to acquire a new route.
pub(super) fn owned(state: Signal<ShellState>) -> Element {
    let scope = {
        let s = state.read();
        format!(
            "{}-{:?}-{:?}-{}-{:?}",
            s.core.account_epoch,
            s.core.active_team,
            s.core.active_channel,
            s.channel_generation,
            s.main_view
        )
    };
    rsx! {for scope in [scope] {Panel {key:"{scope}",state}}}
}

#[component]
pub(crate) fn Panel(state: Signal<ShellState>) -> Element {
    let mount = use_hook(|| {
        let s = state.peek();
        let channel_tabs::selection::Route::Threads(route) = channel_tabs::selection::current(&s)?
        else {
            return None;
        };
        Some((channel_tabs::Visit::capture(&s)?, route))
    });
    use_effect(move || {
        if let Some((visit, route)) = mount.as_ref() {
            threads::enter(state, visit, route);
        }
    });
    let s = state.read();
    let source = threads::Source::capture(&s);
    let presentation = threads::fixture::presentation(&s);
    let reader = threads::visible(&s);
    let supported = reader.is_some() || presentation.is_some();
    let busy = reader.is_some_and(|r| r.loading()) || presentation.is_some_and(|p| p.loading);
    let cooling = threads::limit_status(&s).cooling();
    let notice = reader
        .map(|r| r.notice.clone())
        .or_else(|| presentation.map(|p| p.notice.clone()))
        .unwrap_or_default();
    let loaded = reader.is_some_and(|r| r.loaded()) || presentation.is_some_and(|p| !p.loading);
    let more = reader
        .and_then(|r| r.accepted())
        .is_some_and(|a| a.has_more())
        || presentation.is_some_and(|p| p.has_more);
    let channel = s.core.active_channel.as_deref().unwrap_or_default();
    let items = if let Some(p) = presentation {
        p.items
            .iter()
            .map(|i| Item {
                root: i.root.clone(),
                replies: i.replies.clone(),
                earlier: i.earlier,
                previous: None,
                show_previous: i.previous,
            })
            .collect::<Vec<_>>()
    } else {
        s.core
            .active_team
            .as_ref()
            .and_then(|team| s.core.workspaces.get(team))
            .map(|w| {
                threads::loaded(&s)
                    .into_iter()
                    .map(|i| Item {
                        root: crate::message_vm::message_vm_with_pending(
                            w, i.root, &s.media, false,
                        ),
                        replies: i
                            .replies
                            .iter()
                            .map(|m| {
                                crate::message_vm::message_vm_with_pending(w, m, &s.media, false)
                            })
                            .collect(),
                        earlier: i.earlier,
                        show_previous: i.previous.is_some(),
                        previous: i.previous,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    rsx! {section{id:"channel-threads-panel",class:"secondary-list channel-threads-panel","aria-label":"Channel threads",
        div{class:"secondary-toolbar",h2{"Threads"}
            button{r#type:"button",id:"channel-threads-refresh",disabled:source.is_none()||busy||cooling,onclick:{let source=source.clone();move |_|if let Some(source)=source.as_ref(){source.start(state);}},"Refresh"}
        }
        if !supported{p{class:"secondary-empty",role:"status","Threads aren’t available for this channel right now."}}
        if busy{p{class:"secondary-empty",role:"status","Loading threads…"}}
        if !notice.is_empty(){p{class:"secondary-empty",role:"status","{notice}"}
            div{class:"secondary-toolbar",button{r#type:"button",id:"channel-threads-retry",disabled:source.is_none()||busy||cooling,onclick:{let source=source.clone();move |_|if let Some(source)=source.as_ref(){source.start(state);}},"Retry"}}
        }
        if supported&&loaded&&!busy&&notice.is_empty()&&items.is_empty(){div{class:"secondary-empty",h3{"No current threads are loaded."}p{"Open Threads to find more followed conversations."}}}
        for item in &items{section{class:"channel-discussion",key:"{item.root.id}","data-discussion-root":"{item.root.id}",
            header{class:"channel-discussion-heading",span{"{item.root.reply_count} replies"}}
            {super::channel_discussion_message(state,&s,&item.root,channel)}
            if item.show_previous{div{class:"secondary-toolbar",button{r#type:"button",class:"channel-discussion-previous","data-previous-thread":"{item.root.id}",disabled:item.previous.is_none()||busy,onclick:{let action=item.previous.clone();move |_|if let Some(action)=action.as_ref(){action.reveal(state);}},"Show previous replies"}}}
            else if item.earlier{p{class:"channel-discussion-context","Open the thread to read earlier replies."}}
            for reply in &item.replies{{super::channel_discussion_message(state,&s,reply,channel)}}
            footer{class:"secondary-toolbar",button{r#type:"button",class:"channel-discussion-open","data-open-thread":"{item.root.id}",disabled:source.is_none()||busy,onclick:{let source=source.clone();let root=item.root.id.clone();move |_|if let Some(source)=source.as_ref(){source.open(state,root.clone());}},"Open thread"}}
        }}
        if more&&!items.is_empty(){p{class:"channel-discussion-context","More followed threads may be available in Threads."}}
    }}
}
