use crate::media::MediaUri;
use crate::state::ShellState;
use dioxus::prelude::*;
mod author;
mod copy_link;
mod edited;
mod replies;
mod timestamp;
pub(crate) use replies::eligible as eligible_replies;
pub(crate) use replies::selected as selected_replies;

pub(crate) fn thread_list(state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let Some(feed) = crate::thread_feed::visible_state(snapshot) else {
        return rsx! {p {class:"secondary-empty", "Loading threads…"}};
    };
    let read = crate::thread_feed::Source::capture(snapshot);
    let mark_all = crate::thread_feed::MarkAll::capture(snapshot);
    let mark_status = crate::thread_feed::mark_all_status(snapshot);
    let retry = feed.failed_read();
    let failed_older = retry.is_some_and(|r| r.cursor.is_some());
    let initial_failure = retry.is_some() && !feed.loaded && !failed_older;
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    rsx! {
        div { class: "secondary-list", "aria-label":"Followed threads",
            div {class:"secondary-toolbar",
                button {id:"thread-feed-refresh",disabled:feed.loading,onclick:{let source=read.clone();move |_|{if let Some(source)=source.clone(){source.start(state,crate::thread_feed::Mode::Refresh);}}},
                    if feed.dirty {"New activity — refresh"} else {"Refresh"}
                }
                if crate::thread_feed::has_unread_threads(snapshot) || mark_status.is_some() {
                    button {id:"thread-feed-mark-all-read",r#type:"button",disabled:mark_all.is_none(),
                        "aria-busy":mark_status==Some("Marking threads read…"),
                        onclick:{let action=mark_all.clone();move |_|if let Some(action)=action.clone(){action.start(state);}},
                        "Mark all threads read"
                    }
                }
            }
            if let Some(status)=mark_status {p {class:"secondary-empty",role:"status","{status}"}}
            if feed.loading && feed.items.is_empty() {p {class:"secondary-empty", role:"status", "Loading threads…"}}
            if feed.failed {p {class:"secondary-empty",role:"status",
                if initial_failure {"Could not load threads. Try again when you reconnect."}
                else if failed_older {"Could not load older threads. Your loaded conversations are still available."}
                else if retry.is_some() {"Could not refresh threads. Your loaded conversations are still available."}
                else {"Threads could not finish loading. Refresh to try again."}
            }}
            if retry.is_some() {div {class:"secondary-toolbar",
                button {id:"thread-feed-retry",disabled:feed.loading,onclick:{let source=read.clone();move |_|{if let Some(source)=source.clone(){source.start(state,crate::thread_feed::Mode::Retry);}}},"Retry"}
            }}
            if feed.loaded && !feed.loading && !feed.failed && feed.items.is_empty() {p {class:"secondary-empty", "No followed threads yet."}}
                for item in &feed.items {
                    {
                        let document_author = self::author::document_label(snapshot, read.as_ref(), item, &item.root_msg);
                        let author = document_author.clone().unwrap_or_else(|| workspace
                            .map(|workspace| super_platinum_core::state::message_author_name(workspace, &item.root_msg))
                            .unwrap_or_else(|| "Slack".into()));
                        let preview = workspace
                            .map(|workspace| crate::message_vm::preview::body(workspace, &item.root_msg))
                            .unwrap_or_default();
                        let time = item
                            .root_msg
                            .ts
                            .as_deref()
                            .map(super_platinum_core::state::format_relative_ts)
                            .unwrap_or_default();
                        let initials = author
                            .chars()
                            .next()
                            .map(|c| c.to_uppercase().to_string())
                            .unwrap_or_else(|| "S".into());
                        let replies = item.root_msg.reply_count.unwrap_or(item.replies().len() as u32);
                        let open = read.as_ref().and_then(|source| source.row(snapshot, item));
                        let channel_name = item.channel().and_then(|id| workspace.and_then(|w|w.channels.get(id).map(|c| super_platinum_core::state::channel_display_name(w,c)))).unwrap_or_else(|| "Conversation".into());
                        // Source's document avatar policy is separate. Never
                        // paint the posting user's face for an edited document.
                        let avatar = if document_author.is_some() { None } else { workspace.and_then(|workspace| {
                            let user = item.root_msg.user.as_deref()?;
                            let url = workspace.avatar_url(user)?;
                            let asset = snapshot.media.register_avatar(user, &url);
                            snapshot
                                .media
                                .is_ready(&asset)
                                .then(|| asset.webview_uri_at(snapshot.media_epoch))
                        }) };
                        rsx! {
                            article { class: if item.unread_replies.is_empty() {"secondary-row thread-row"} else {"secondary-row thread-row unread"}, key: "thread-feed-{item.channel().cloned().unwrap_or_default()}-{item.root_ts().cloned().unwrap_or_default()}",
                                span { class: "secondary-avatar",
                                    if let Some(uri) = avatar.as_ref() {
                                        img { src: "{uri}", alt: "{author}" }
                                    } else {
                                        "{initials}"
                                    }
                                }
                                div { class: "secondary-copy",
                                    {self::author::render(state,snapshot,read.as_ref().and_then(|r|r.profile(snapshot,item,&item.root_msg)),&author)}
                                    div {class:"message-text thread-feed-root-body",
                                        {self::edited::render(state, snapshot, item, &item.root_msg, &preview)}
                                    }
                                    {self::replies::root_counts(snapshot, item)}
                                    span { "{channel_name} · {replies} replies · "
                                        {self::timestamp::render(state,snapshot,item,read.as_ref().and_then(|r|r.timestamp(snapshot,item)),&time)}
                                        if !item.unread_replies.is_empty() {" · {item.unread_replies.len()} unread"}
                                    }
                                    {self::replies::render(state, snapshot, item, read.as_ref())}
                                    div {class:"thread-feed-root-actions",
                                        {self::copy_link::render(state,snapshot,read.as_ref().and_then(|r|r.copy_link(snapshot,item)))}
                                        button {r#type:"button",class:"thread-feed-open",id:"thread-feed-open-{item.channel().cloned().unwrap_or_default()}-{item.root_ts().cloned().unwrap_or_default()}",disabled:open.is_none(),
                                            onclick: { let open = open.clone(); move |_| if let Some(open)=open.as_ref(){open.start(state);} },
                                            "Open thread"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

            if feed.has_more {
                div {class:"secondary-toolbar",
                    button {id:"thread-feed-older",disabled:feed.loading,onclick:{let source=read.clone();move |_|{if let Some(source)=source.clone(){source.start(state,crate::thread_feed::Mode::Older);}}},"Load more threads"}
                }
            }
        }
    }
}

#[cfg(all(test, feature = "protected-native-fixture"))]
mod recovery;

#[cfg(all(test, feature = "protected-native-fixture"))]
mod preview;
