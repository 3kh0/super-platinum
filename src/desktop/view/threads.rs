use super::secondary::message_preview;
use crate::media::MediaUri;
use crate::state::ShellState;
use dioxus::prelude::*;

pub(crate) fn thread_list(state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let Some(feed) = crate::thread_feed::visible_state(snapshot) else {
        return rsx! {p {class:"secondary-empty", "Loading threads…"}};
    };
    let read = crate::thread_feed::Source::capture(snapshot);
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
            }
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
                        let author = workspace
                            .map(|workspace| super_platinum_core::state::message_author_name(workspace, &item.root_msg))
                            .unwrap_or_else(|| "Slack".into());
                        let preview = workspace
                            .map(|workspace| message_preview(snapshot, workspace, &item.root_msg))
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
                        let channel_name = item.channel().and_then(|id| workspace.and_then(|w|w.channels.get(id).map(|c| super_platinum_core::state::channel_display_name(w,c)))).unwrap_or_else(|| "Conversation".into());
                        let avatar = workspace.and_then(|workspace| {
                            let user = item.root_msg.user.as_deref()?;
                            let url = workspace.avatar_url(user)?;
                            let asset = snapshot.media.register_avatar(user, &url);
                            snapshot
                                .media
                                .is_ready(&asset)
                                .then(|| asset.webview_uri_at(snapshot.media_epoch))
                        });
                        rsx! {
                            button { class: if item.unread_replies.is_empty() {"secondary-row thread-row"} else {"secondary-row thread-row unread"}, key: "thread-feed-{item.channel().cloned().unwrap_or_default()}-{item.root_ts().cloned().unwrap_or_default()}",
                                onclick: { let channel = item.channel().cloned(); let root = item.root_ts().cloned(); move |_| if let (Some(channel), Some(root)) = (channel.clone(), root.clone()) { spawn(crate::bootstrap::open_thread(state, channel, root)); } },
                                span { class: "secondary-avatar",
                                    if let Some(uri) = avatar.as_ref() {
                                        img { src: "{uri}", alt: "{author}" }
                                    } else {
                                        "{initials}"
                                    }
                                }
                                div { class: "secondary-copy",
                                    strong { "{author}" }
                                    p { "{preview}" }
                                    span { "{channel_name} · {replies} replies · {time}"
                                        if !item.unread_replies.is_empty() {" · {item.unread_replies.len()} unread"}
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
