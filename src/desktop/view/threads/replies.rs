//! Reading only from the current native feed, never canonical/history authority.
use super::*;
use super_platinum_core::slack::models::{Message, ThreadViewItem};

// Native DOM safety budget, not Slack's history/minimum-visible policy.
const MAX_REPLIES: usize = 100;

fn timestamp(ts: &str) -> Option<(u64, u64)> {
    let (seconds, fraction) = ts.split_once('.')?;
    if seconds.is_empty()
        || !seconds.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_empty()
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let seconds = seconds.parse::<u64>().ok()?;
    if seconds == 0 || seconds > i64::MAX as u64 {
        return None;
    }
    Some((
        seconds,
        fraction.parse::<u64>().ok()? * 10u64.pow(6 - fraction.len() as u32),
    ))
}

pub(crate) fn selected<'a>(snapshot: &ShellState, item: &'a ThreadViewItem) -> Vec<&'a Message> {
    let mut replies = eligible(snapshot, item);
    let limit = crate::thread_feed::reply_visible_limit(snapshot, item).unwrap_or(MAX_REPLIES);
    if replies.len() > limit {
        replies.drain(..replies.len() - limit);
    }
    replies
}

// Same original row predicates, without a visibility budget. Reading-memory
// validation uses this to avoid recursively asking for its own budget.
pub(crate) fn eligible<'a>(snapshot: &ShellState, item: &'a ThreadViewItem) -> Vec<&'a Message> {
    let Some(team) = snapshot.core.active_team.as_deref() else {
        return Vec::new();
    };
    if !snapshot
        .core
        .threads_view
        .matches_scope(snapshot.core.account_epoch, team)
    {
        return Vec::new();
    }
    let Some(channel) = item.root_msg.channel.as_deref().filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let Some(root) = item.root_msg.ts.as_deref() else {
        return Vec::new();
    };
    let Some(root_key) = timestamp(root) else {
        return Vec::new();
    };
    if item
        .root_msg
        .thread_ts
        .as_deref()
        .is_some_and(|s| s != root)
        || item.root_msg.team.as_deref().is_some_and(|s| s != team)
    {
        return Vec::new();
    }
    let deleted = snapshot
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(channel));
    item.replies()
        .into_iter()
        .filter(|message| {
            let Some(ts) = message.ts.as_deref() else {
                return false;
            };
            timestamp(ts).is_some_and(|key| key > root_key)
                && message.channel.as_deref().is_none_or(|s| s == channel)
                && message.thread_ts.as_deref().is_none_or(|s| s == root)
                && message.team.as_deref().is_none_or(|s| s == team)
                && !super_platinum_core::channel_threads::hidden(message)
                && !deleted.is_some_and(|messages| messages.was_deleted(ts))
        })
        .collect::<Vec<_>>()
}

pub(super) fn render(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    item: &ThreadViewItem,
    read: Option<&crate::thread_feed::Source>,
) -> Element {
    let replies = selected(snapshot, item);
    let earlier = read.and_then(|source| source.earlier_replies(snapshot, item));
    let network_earlier = if earlier.is_none() {
        read.and_then(|source| source.network_earlier_replies(snapshot, item))
    } else {
        None
    };
    let timestamps = replies
        .iter()
        .filter_map(|m| m.ts.as_deref())
        .collect::<Vec<_>>();
    let unread = crate::thread_feed::unread_marker(snapshot, item, &timestamps);
    let Some(workspace) = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|t| snapshot.core.workspaces.get(t))
    else {
        return rsx! {};
    };
    let more = item
        .root_msg
        .reply_count
        .is_some_and(|count| count as usize > replies.len());
    rsx! {
        section {class:"thread-feed-replies", "aria-label":"Replies",
            if let Some(earlier) = earlier {
                button {r#type:"button",class:"chip",id:earlier.control_id(),
                    onclick:move |_|earlier.start(state),"Load earlier replies"}
            } else if let Some(earlier) = network_earlier {
                button {r#type:"button",class:"chip",id:earlier.control_id(),disabled:earlier.pending(),
                    "aria-busy":earlier.pending(),onclick:move |_|earlier.start(state),"Load earlier replies"}
            } else if crate::thread_feed::earlier_layout(snapshot) {
                button {r#type:"button",class:"chip",disabled:true,"Load earlier replies"}
            }
            for (index, message) in replies.iter().enumerate() {
                {
                    let author = super::author::document_label(snapshot,read,item,message)
                        .unwrap_or_else(||super_platinum_core::state::message_author_name(workspace, message));
                    let ts = message.ts.as_deref().unwrap_or_default();
                    let time = super_platinum_core::state::format_relative_ts(ts);
                    let body = if message.subtype.as_deref() == Some("tombstone") {
                        vec![crate::state::RichNode::Text("This message was deleted.".into())]
                    } else {
                        crate::message_vm::preview::body(workspace, message)
                    };
                    rsx! {
                        div {key:"reply-group-{ts}",class:"thread-feed-reply-group",
                        if let Some((before, ghost)) = unread {
                            if before == index {
                                div {class:if ghost {"thread-feed-new ghost"} else {"thread-feed-new"},
                                    role:"separator",
                                    "data-thread-unread-before":"{ts}","aria-label":if ghost {"Previously unread replies"} else {"New replies"},
                                    hr {"aria-hidden":"true"}
                                    span {"New"}
                                }
                            }
                        }
                        div {class:"channel-discussion thread-feed-reply",key:"{ts}","data-thread-reply":"{ts}",
                            {super::author::render(state,snapshot,read.and_then(|r|r.profile(snapshot,item,message)),&author)}
                            span {" · {time}"}
                            div {class:"message-text thread-feed-reply-body",
                                {super::edited::render(state, snapshot, item, message, &body)}
                            }
                            {reaction_counts(snapshot, workspace, message)}
                        }
                        }
                    }
                }
            }
            if more {p {class:"channel-discussion-context","Open the thread to read more replies."}}
        }
    }
}

// Qualified ordinary pure grouping over the existing admitted native rows.
// No keyed Source lookup, media or action authority. Preserve the original
// duplicate-name/positive/50-row boundary before grouping; glyphs remain text.
fn reaction_counts(
    snapshot: &ShellState,
    workspace: &super_platinum_core::state::Workspace,
    message: &Message,
) -> Element {
    // Native reading identity, recomputed from this render's current snapshot.
    // A reaction user ID alone cannot identify the viewer. This is presentation
    // only, not Source's keyed current-user namespace or permission authority.
    let viewer = snapshot.core.active_team.as_ref().and_then(|team| {
        snapshot
            .core
            .session
            .as_ref()?
            .workspaces
            .get(team)
            .filter(|session| {
                session.team_id == *team
                    && !session.user_id.is_empty()
                    && session.user_id == workspace.self_user_id
            })
            .map(|session| session.user_id.as_str())
    });
    let mut names = std::collections::HashMap::new();
    for reaction in &message.reactions {
        *names.entry(reaction.name.as_str()).or_insert(0usize) += 1;
    }
    let rows = message
        .reactions
        .iter()
        .filter(|r| {
            !r.name.trim().is_empty() && r.count > 0 && names.get(r.name.as_str()) == Some(&1)
        })
        .take(50)
        .map(|r| {
            let mut seen = std::collections::HashSet::new();
            crate::state::ReactionVm {
                name: r.name.clone(),
                count: r.count,
                own: viewer.is_some_and(|me| r.users.iter().any(|user| user == me)),
                // Resolve only the bounded native name sample. No avatar, emoji
                // URL or MediaRegistry work belongs in this glyph-only reader.
                reactors: r
                    .users
                    .iter()
                    .filter(|id| !id.is_empty() && seen.insert(id.as_str()))
                    .take(crate::model::REACTOR_NAMES_SHOWN)
                    .map(|id| crate::model::ReactorVm {
                        user_id: id.clone(),
                        name: if viewer == Some(id.as_str()) {
                            "You".into()
                        } else {
                            workspace.display_name(id)
                        },
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    if message.subtype.as_deref() == Some("tombstone") || rows.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {class:"thread-feed-reactions", "aria-label":"Reactions",
            for group in super::super::rich::grouped_reaction_vms(&rows) {
                {
                    // Preserve singleton labels, including a lone tone spelling.
                    let name:&str=if group.variants.len()==1{&group.variants[0].name}else{group.name};
                    // Names are a sample, never a count of unique reactors.
                    // Keep positive own feedback even when outside that sample.
                    let description=if group.own {format!("{} · You reacted",group.label())}else{group.label()};
                    rsx! {span {class:if group.own {"thread-feed-reaction own"} else {"thread-feed-reaction"},key:"{name}","data-reaction-name":"{name}","aria-label":"{name}: {group.count} reactions",
                        title:description.clone(),"aria-description":description,
                        span {class:"reaction-variants",
                            for variant in group.variants.iter().take(10) {
                                span {class:"reaction-glyph",{super_platinum_core::state::emoji_glyph(&variant.name)}}
                            }
                            if group.variants.len()>10 {span {class:"reaction-variants-overflow",title:format!("{} more emoji variants",group.variants.len()-10),
                                {crate::icons::icon(crate::icons::Icon::More,"icon sm")}
                            }}
                        }
                        span {class:"reaction-count","{group.count}"}
                    }}
                }
            }
        }
    }
}

pub(super) fn root_reading(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    item: &ThreadViewItem,
    read: Option<&crate::thread_feed::Source>,
) -> Element {
    super::reaction_toggle::render(state, snapshot, item, read)
}

pub(super) fn root_counts(snapshot: &ShellState, item: &ThreadViewItem) -> Element {
    let Some(team) = snapshot.core.active_team.as_deref() else {
        return rsx! {};
    };
    let root = &item.root_msg;
    let Some(channel) = root.channel.as_deref().filter(|s| !s.is_empty()) else {
        return rsx! {};
    };
    let Some(ts) = root.ts.as_deref().filter(|s| timestamp(s).is_some()) else {
        return rsx! {};
    };
    if !snapshot
        .core
        .threads_view
        .matches_scope(snapshot.core.account_epoch, team)
        || root.kind.as_deref().is_some_and(|kind| kind != "message")
        || root.user.as_deref().is_none_or(|user| user.is_empty())
        || root.bot_id.is_some()
        || root.bot_profile.is_some()
        || root.subtype.is_some()
        || root.thread_ts.as_deref().is_some_and(|r| r != ts)
        || root.team.as_deref().is_some_and(|t| t != team)
        || super_platinum_core::channel_threads::hidden(root)
    {
        return rsx! {};
    }
    let Some(workspace) = snapshot.core.workspaces.get(team) else {
        return rsx! {};
    };
    if workspace
        .messages
        .get(channel)
        .is_some_and(|messages| messages.was_deleted(ts))
    {
        return rsx! {};
    }
    // Conservative native ordinary-root reading only. The Source generic slot
    // does not establish specialized/custom-footer or normalization defaults.
    reaction_counts(snapshot, workspace, root)
}

#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
