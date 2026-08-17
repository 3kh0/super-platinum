use dioxus::prelude::*;

use crate::state::{ActivityTab, MainView, Overlay, ShellState};

pub(crate) fn secondary_view(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    match snapshot.main_view {
        MainView::Home => rsx! {},
        MainView::Unreads => {
            let unread_channels = workspace
                .into_iter()
                .flat_map(|workspace| workspace.channels.values())
                .filter(|channel| {
                    workspace.is_some_and(|workspace| workspace.unread_total(channel) > 0)
                })
                .collect::<Vec<_>>();
            rsx! {
                div { class: "secondary-list",
                    div { class: "secondary-toolbar",
                        button { onclick: move |_| { spawn(crate::bootstrap::mark_all_read(state)); }, "Mark all read" }
                    }
                    if unread_channels.is_empty() {
                        p { class: "secondary-empty", "You're all caught up." }
                    }
                    for channel in unread_channels {
                        {
                            let name = workspace
                                .map(|ws| super_platinum_core::state::channel_display_name(ws, channel))
                                .unwrap_or_else(|| channel.id.clone());
                            let title = if channel.is_im || channel.is_mpim {
                                name.clone()
                            } else {
                                format!("#{name}")
                            };
                            let unread = workspace.map(|ws| ws.unread_total(channel)).unwrap_or(0);
                            let snippets = workspace
                                .and_then(|ws| ws.messages.get(&channel.id))
                                .map(|messages| {
                                    messages
                                        .messages
                                        .iter()
                                        .rev()
                                        .take(3)
                                        .rev()
                                        .map(|message| {
                                            format!(
                                                "{}: {}",
                                                workspace
                                                    .map(|ws| {
                                                        super_platinum_core::state::message_author_name(
                                                            ws, message,
                                                        )
                                                    })
                                                    .unwrap_or_else(|| "Slack".into()),
                                                workspace
                                                    .map(|ws| message_preview(snapshot, ws, message))
                                                    .unwrap_or_else(|| super_platinum_core::state::message_text(message))
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            let icon = if channel.is_im || channel.is_mpim {
                                name.chars()
                                    .next()
                                    .map(|c| c.to_uppercase().to_string())
                                    .unwrap_or_else(|| "D".into())
                            } else if channel.is_private {
                                "🔒".into()
                            } else {
                                "#".into()
                            };
                            rsx! {
                                button { class: "secondary-row unread", key: "unread-{channel.id}",
                                    onclick: { let id = channel.id.clone(); move |_| { let index = { state.read().channels.iter().position(|channel| channel.id == id) }; if let Some(index) = index { state.write().select_channel(index); spawn(crate::bootstrap::refresh_selected_channel(state)); } } },
                                    span { class: "secondary-avatar", "{icon}" }
                                    div { class: "secondary-copy",
                                        strong { "{title}" }
                                        for snippet in snippets {
                                            p { "{snippet}" }
                                        }
                                        span { "{unread} unread" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        MainView::Threads => rsx! {
            div { class: "secondary-list",
                if snapshot.core.threads_view.loading { p { class: "secondary-empty", "Loading threads…" } }
                for item in &snapshot.core.threads_view.items {
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
                        let replies = item.replies().len();
                        let avatar = workspace.and_then(|workspace| {
                            let user = item.root_msg.user.as_deref()?;
                            let url = workspace.avatar_url(user)?;
                            let asset = snapshot.media.register(
                                super_platinum_core::MediaAssetKind::Avatar,
                                &url,
                                "image/jpeg",
                                url.contains("slack-edge.com") || url.contains("slack.com"),
                            );
                            Some(asset.uri())
                        });
                        rsx! {
                            button { class: "secondary-row thread-row", key: "thread-feed-{item.root_ts().cloned().unwrap_or_default()}",
                                onclick: { let channel = item.channel().cloned(); let root = item.root_ts().cloned(); move |_| if let (Some(channel), Some(root)) = (channel.clone(), root.clone()) { spawn(crate::bootstrap::open_thread(state, channel, root)); } },
                                span { class: "secondary-avatar",
                                    if let Some(uri) = avatar.as_ref() {
                                        img { key: "thread-av-{snapshot.media_epoch}-{uri}", src: "{uri}", alt: "{author}" }
                                    } else {
                                        "{initials}"
                                    }
                                }
                                div { class: "secondary-copy",
                                    strong { "{author}" }
                                    p { "{preview}" }
                                    span { "{replies} replies · {time}" }
                                }
                            }
                        }
                    }
                }
            }
        },
        MainView::Dms | MainView::Activity => rsx! {},
    }
}

pub(crate) fn activity_channel_label(
    workspace: Option<&super_platinum_core::state::Workspace>,
    item: &super_platinum_core::slack::models::ActivityItem,
) -> String {
    let Some(id) = item.channel() else {
        return String::new();
    };
    let Some(ws) = workspace else {
        return String::new();
    };
    let Some(channel) = ws.channels.get(id) else {
        return String::new();
    };
    if channel.is_im || channel.is_mpim {
        super_platinum_core::state::channel_display_name(ws, channel)
    } else {
        format!(
            "#{}",
            super_platinum_core::state::channel_display_name(ws, channel)
        )
    }
}

pub(crate) fn dm_list_panel(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    let compose_src = crate::icons::compose_uri();
    let active_id = snapshot
        .channels
        .get(snapshot.active_channel)
        .map(|channel| channel.id.as_str())
        .unwrap_or_default();
    rsx! {
        aside { class: "list-panel",
            div { class: "list-panel-header",
                h2 { "Direct messages" }
                div { class: "spacer" }
                label { class: "toggle",
                    "Unreads"
                    input {
                        r#type: "checkbox",
                        checked: snapshot.dm_unread_only,
                        onchange: move |event| state.write().dm_unread_only = event.checked()
                    }
                }
                button {
                    class: "icon-btn",
                    title: "Compose",
                    onclick: move |_| state.write().overlay = Some(Overlay::Palette),
                    img { class: "icon sm", src: "{compose_src}", alt: "Compose" }
                }
            }
            input {
                class: "list-find",
                value: "{snapshot.dm_query}",
                placeholder: "Find a DM…",
                oninput: move |event| state.write().dm_query = event.value()
            }
            div { class: "list-body",
                if snapshot.core.dms.loading {
                    p { class: "list-empty", "Loading direct messages…" }
                }
                for entry in snapshot.core.dms.entries.iter().filter(|entry| {
                    let label = workspace
                        .and_then(|ws| entry.channel.as_ref().map(|channel| super_platinum_core::state::channel_display_name(ws, channel)))
                        .or_else(|| entry.channel.as_ref().and_then(|channel| channel.name.clone()))
                        .unwrap_or_else(|| "Direct message".into());
                    (!snapshot.dm_unread_only || dm_entry_unread(workspace, entry) > 0)
                        && (snapshot.dm_query.is_empty()
                            || label.to_lowercase().contains(&snapshot.dm_query.to_lowercase()))
                }) {
                    {
                        let label = workspace
                            .and_then(|ws| entry.channel.as_ref().map(|channel| super_platinum_core::state::channel_display_name(ws, channel)))
                            .or_else(|| entry.channel.as_ref().and_then(|channel| channel.name.clone()))
                            .unwrap_or_else(|| "Direct message".into());
                        let snippet = dm_preview(snapshot, workspace, entry);
                        let time = entry
                            .latest
                            .as_deref()
                            .or_else(|| entry.message.as_ref().and_then(|m| m.ts.as_deref()))
                            .map(dm_time_label)
                            .unwrap_or_default();
                        let initials = label
                            .chars()
                            .find(|ch| ch.is_alphanumeric())
                            .map(|ch| ch.to_uppercase().to_string())
                            .unwrap_or_else(|| "?".into());
                        let unread = dm_entry_unread(workspace, entry);
                        let is_active = entry.id == active_id;
                        let avatar = workspace
                            .and_then(|ws| {
                                let channel = ws.channels.get(&entry.id).or(entry.channel.as_ref())?;
                                let user = super_platinum_core::state::dm_user_id(channel)?;
                                ws.avatar_url(user)
                            })
                            .and_then(|_url| {
                                snapshot
                                    .channels
                                    .iter()
                                    .find(|channel| channel.id == entry.id)
                                    .and_then(|channel| channel.avatar.clone())
                            });
                        let presence = snapshot
                            .channels
                            .iter()
                            .find(|channel| channel.id == entry.id)
                            .map(|channel| channel.presence)
                            .unwrap_or_default();
                        rsx! {
                            button {
                                class: if is_active {
                                    if unread > 0 { "dm-row active unread" } else { "dm-row active" }
                                } else if unread > 0 {
                                    "dm-row unread"
                                } else {
                                    "dm-row"
                                },
                                key: "dm-{entry.id}",
                                onclick: {
                                    let channel = entry.id.clone();
                                    move |_| {
                                        let index = {
                                            state.read().channels.iter().position(|candidate| candidate.id == channel)
                                        };
                                        if let Some(index) = index {
                                            state.write().select_channel(index);
                                            spawn(crate::bootstrap::refresh_selected_channel(state));
                                        }
                                    }
                                },
                                div { class: "dm-avatar-wrap",
                                    div { class: "dm-avatar",
                                        if let Some(avatar) = avatar.as_ref() {
                                            img { key: "dm-av-{snapshot.media_epoch}-{avatar.uri()}", src: "{avatar.uri()}", alt: "{label}" }
                                        } else {
                                            "{initials}"
                                        }
                                    }
                                    span {
                                        class: if presence == crate::model::PresenceVm::Active {
                                            "presence-dot active"
                                        } else {
                                            "presence-dot"
                                        }
                                    }
                                }
                                div { class: "dm-mid",
                                    span { class: "dm-name", "{label}" }
                                    p { class: "dm-snippet", "{snippet}" }
                                }
                                div { class: "dm-meta",
                                    span { class: "dm-time", "{time}" }
                                    if unread > 0 {
                                        span { class: "ping-badge",
                                            if unread > 99 { "99+" } else { "{unread}" }
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
}

pub(crate) fn dm_preview(
    snapshot: &ShellState,
    workspace: Option<&super_platinum_core::state::Workspace>,
    entry: &super_platinum_core::slack::models::DmEntry,
) -> String {
    let Some(message) = entry.message.as_ref() else {
        return String::new();
    };
    let Some(ws) = workspace else {
        return super_platinum_core::state::message_text(message);
    };
    let body = message_preview(snapshot, ws, message);
    let is_mpim = ws
        .channels
        .get(&entry.id)
        .or(entry.channel.as_ref())
        .map(|channel| channel.is_mpim)
        .unwrap_or(false);
    match message.user.as_deref() {
        Some(user) if user == ws.self_user_id => format!("You: {body}"),
        Some(user) if is_mpim => format!("{}: {body}", ws.display_name(user)),
        _ => body,
    }
}

fn message_preview(
    snapshot: &ShellState,
    workspace: &super_platinum_core::state::Workspace,
    message: &super_platinum_core::slack::models::Message,
) -> String {
    let vm = crate::message_vm::message_vm(workspace, message, &snapshot.media);
    let text = super::rich::message_plain_text(&vm);
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned()
}

pub(crate) fn dm_entry_unread(
    workspace: Option<&super_platinum_core::state::Workspace>,
    entry: &super_platinum_core::slack::models::DmEntry,
) -> u32 {
    if let Some(ws) = workspace
        && let Some(channel) = ws.channels.get(&entry.id)
    {
        return ws.unread_total(channel);
    }
    entry
        .channel
        .as_ref()
        .and_then(|channel| channel.unread_count_display.or(channel.unread_count))
        .unwrap_or(0)
}

pub(crate) fn dm_time_label(ts: &str) -> String {
    let (secs, _) = super_platinum_core::state::ts_key(ts);
    let now = super_platinum_core::state::now_secs().max(0) as u64;
    if now < secs {
        return super_platinum_core::state::format_ts_hm(ts);
    }
    let elapsed = now.saturating_sub(secs);
    if elapsed < 60 {
        "now".into()
    } else if elapsed < 86_400 {
        // Same-ish day window: wall-clock like Slack DM list rows.
        super_platinum_core::state::format_ts_hm(ts)
    } else if elapsed < 172_800 {
        "Yesterday".into()
    } else {
        super_platinum_core::state::format_ts_date_label(ts)
    }
}

pub(crate) fn activity_list_panel(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    let unread_only = snapshot.core.activity.unread_only;
    let tab = snapshot.activity_tab;
    let all_items = &snapshot.core.activity.items;
    let count_all = all_items.iter().filter(|item| item.is_unread).count();
    let count_dms = all_items
        .iter()
        .filter(|item| item.is_unread && ActivityTab::Dms.matches(item))
        .count();
    let count_mentions = all_items
        .iter()
        .filter(|item| item.is_unread && ActivityTab::Mentions.matches(item))
        .count();
    let count_threads = all_items
        .iter()
        .filter(|item| item.is_unread && ActivityTab::Threads.matches(item))
        .count();
    let count_reactions = all_items
        .iter()
        .filter(|item| item.is_unread && ActivityTab::Reactions.matches(item))
        .count();
    let items: Vec<_> = all_items
        .iter()
        .filter(|item| tab.matches(item))
        .filter(|item| !unread_only || item.is_unread)
        .collect();
    // Precompute day headers so we don't mutate inside rsx.
    let mut rows: Vec<(
        Option<String>,
        &super_platinum_core::slack::models::ActivityItem,
    )> = Vec::new();
    let mut last_day: Option<String> = None;
    for item in &items {
        let day = super_platinum_core::state::date_key_for_ts(&item.feed_ts);
        let day_label = if day.as_ref() != last_day.as_ref() {
            last_day = day.clone();
            Some(super_platinum_core::state::format_ts_date_label(
                &item.feed_ts,
            ))
        } else {
            None
        };
        rows.push((day_label, *item));
    }
    rsx! {
        aside { class: "list-panel",
            div { class: "list-panel-header",
                h2 { "Activity" }
                if let Some(count) = workspace.and_then(|ws| ws.activity_unread_count) {
                    if count > 0 {
                        span { class: "ping-badge", "{count}" }
                    }
                }
                div { class: "spacer" }
            }
            div { class: "activity-tabs",
                {activity_tab_btn(state, tab, ActivityTab::All, count_all)}
                {activity_tab_btn(state, tab, ActivityTab::Dms, count_dms)}
                {activity_tab_btn(state, tab, ActivityTab::Mentions, count_mentions)}
                {activity_tab_btn(state, tab, ActivityTab::Threads, count_threads)}
                {activity_tab_btn(state, tab, ActivityTab::Reactions, count_reactions)}
            }
            div { class: "activity-toolbar",
                button {
                    class: if unread_only { "chip on" } else { "chip" },
                    onclick: move |_| {
                        let next = !state.read().core.activity.unread_only;
                        state.write().core.activity.unread_only = next;
                    },
                    "Unreads"
                }
            }
            div { class: "list-body",
                if snapshot.core.activity.loading && items.is_empty() {
                    p { class: "list-empty", "Loading activity…" }
                } else if items.is_empty() {
                    p { class: "list-empty",
                        if unread_only { "No unread activity." } else { "No activity yet." }
                    }
                }
                for (day_label, item) in rows {
                    {
                        let verb = activity_verb(item);
                        let target = activity_channel_label(workspace, item);
                        let time = activity_time_label(item);
                        let preview = activity_preview(snapshot, workspace, item);
                        let badge = item.unread_msg_count();
                        let event_icon = activity_event_glyph(item);
                        let avatar = activity_avatar(snapshot, workspace, item);
                        rsx! {
                            // Wrapper so each row is a single keyed root: Dioxus reads
                            // the key off the first root only, so a key on the button
                            // below the optional day divider would be dropped and the
                            // list would fall back to positional diffing.
                            div { class: "activity-group", key: "activity-{item.key}",
                            if let Some(label) = day_label.clone() {
                                div { class: "activity-day", "{label}" }
                            }
                            button {
                                class: if snapshot.core.activity.selected.as_deref() == Some(item.key.as_str()) {
                                    if item.is_unread { "activity-row active unread" } else { "activity-row active" }
                                } else if item.is_unread { "activity-row unread" } else { "activity-row" },
                                onclick: {
                                    let key = item.key.clone();
                                    let channel = item.channel().map(str::to_owned);
                                    let ts = item.ts().map(str::to_owned);
                                    let root = item.thread_ts().map(str::to_owned);
                                    move |_| {
                                        {
                                            let mut shell = state.write();
                                            shell.activity_detail_open = true;
                                            shell.core.activity.selected = Some(key.clone());
                                        }
                                        if let (Some(channel), Some(ts)) = (channel.clone(), ts.clone())
                                            && state.write().open_search_result(&channel, &ts)
                                        {
                                            if let Some(root) = root.clone() {
                                                spawn(crate::bootstrap::open_thread(state, channel, root));
                                            } else {
                                                spawn(crate::bootstrap::refresh_selected_channel(state));
                                            }
                                        }
                                    }
                                },
                                div { class: "activity-bar" }
                                div { class: "dm-avatar activity-event-icon",
                                    if let Some((uri, label)) = avatar.as_ref() {
                                        img { key: "act-{snapshot.media_epoch}-{uri}", src: "{uri}", alt: "{label}" }
                                    } else {
                                        "{event_icon}"
                                    }
                                }
                                div { class: "activity-copy",
                                    div { class: "activity-head",
                                        span { class: "activity-verb", "{verb}" }
                                        if !target.is_empty() {
                                            span { class: "activity-target", "{target}" }
                                        }
                                        span { class: "activity-time", "{time}" }
                                        if badge > 0 {
                                            span { class: "ping-badge",
                                                if badge > 99 { "99+" } else { "{badge}" }
                                            }
                                        }
                                    }
                                    if !preview.is_empty() {
                                        p { class: "activity-preview", "{preview}" }
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
}

pub(crate) fn activity_tab_btn(
    mut state: Signal<ShellState>,
    active: ActivityTab,
    tab: ActivityTab,
    count: usize,
) -> Element {
    rsx! {
        button {
            class: if active == tab { "activity-tab on" } else { "activity-tab" },
            onclick: move |_| state.write().activity_tab = tab,
            "{tab.label()}"
            if count > 0 {
                span { class: "tab-count", "{count}" }
            }
        }
    }
}

pub(crate) fn activity_event_glyph(
    item: &super_platinum_core::slack::models::ActivityItem,
) -> &'static str {
    match item.item.kind.as_str() {
        "message_reaction" => "♥",
        "thread_v2" | "thread_reply" => "↩",
        "at_user" | "mention" | "at_user_group" => "@",
        "at_channel" | "at_everyone" => "@",
        "dm" | "bot_dm_bundle" => "💬",
        "channel" => "#",
        _ => "•",
    }
}

pub(crate) fn activity_verb(
    item: &super_platinum_core::slack::models::ActivityItem,
) -> &'static str {
    match item.item.kind.as_str() {
        "message_reaction" => "Reacted in",
        "thread_v2" | "thread_reply" => "Thread in",
        "at_user_group" => "Group mention in",
        "at_channel" | "at_everyone" => "Channel mention in",
        "at_user" | "mention" => "Mention in",
        "keyword" | "unjoined_channel_mention" => "Keyword mention in",
        "channel" => "Post in",
        "dm" | "bot_dm_bundle" => "DM",
        _ => "Activity in",
    }
}

pub(crate) fn activity_time_label(
    item: &super_platinum_core::slack::models::ActivityItem,
) -> String {
    if item.feed_ts.is_empty() {
        return String::new();
    }
    let (secs, _) = super_platinum_core::state::ts_key(&item.feed_ts);
    let now = super_platinum_core::state::now_secs().max(0) as u64;
    if now < secs {
        return super_platinum_core::state::format_ts_hm(&item.feed_ts);
    }
    let elapsed = now - secs;
    if elapsed < 60 {
        "now".into()
    } else if elapsed < 3600 {
        let m = elapsed / 60;
        if m == 1 {
            "1 min".into()
        } else {
            format!("{m} mins")
        }
    } else if elapsed < 86_400 {
        let h = elapsed / 3600;
        if h == 1 {
            "1 hour".into()
        } else {
            format!("{h} hours")
        }
    } else {
        super_platinum_core::state::format_ts_hm(&item.feed_ts)
    }
}

pub(crate) fn activity_preview(
    snapshot: &ShellState,
    workspace: Option<&super_platinum_core::state::Workspace>,
    item: &super_platinum_core::slack::models::ActivityItem,
) -> String {
    if let (Some(workspace), Some(message)) = (workspace, activity_hydrated_message(snapshot, item))
    {
        let vm = crate::message_vm::message_vm(workspace, message, &snapshot.media);
        let text = super::rich::message_plain_text(&vm);
        let body = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim();
        if item.item.reaction.is_some() {
            let author = message
                .user
                .as_deref()
                .map(|user| {
                    if user == workspace.self_user_id {
                        "You".to_owned()
                    } else {
                        workspace.display_name(user)
                    }
                })
                .unwrap_or_else(|| "You".into());
            return format!("{author}: {body}");
        }
        if !body.is_empty() {
            return body.to_owned();
        }
    }
    if let Some(text) = item
        .item
        .message
        .as_ref()
        .and_then(|message| message.extra.get("text"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        if let Some(workspace) = workspace {
            let message = super_platinum_core::slack::models::Message {
                text: Some(text.to_owned()),
                ..Default::default()
            };
            let vm = crate::message_vm::message_vm(workspace, &message, &snapshot.media);
            let rendered = super::rich::message_plain_text(&vm);
            return rendered.lines().next().unwrap_or(&rendered).to_owned();
        }
        return text.lines().next().unwrap_or(text).to_owned();
    }
    if let Some(reaction) = item.item.reaction.as_ref() {
        let name = if reaction.name.is_empty() {
            "reaction"
        } else {
            reaction.name.as_str()
        };
        let glyph = super_platinum_core::state::emoji_glyph(name);
        let who = reaction
            .user
            .as_deref()
            .map(|user| {
                workspace
                    .map(|ws| ws.display_name(user))
                    .unwrap_or_else(|| user.to_owned())
            })
            .unwrap_or_else(|| "Someone".into());
        return format!("{who}: {glyph}");
    }
    String::new()
}

fn activity_hydrated_message<'a>(
    snapshot: &'a ShellState,
    item: &super_platinum_core::slack::models::ActivityItem,
) -> Option<&'a super_platinum_core::slack::models::Message> {
    let channel = item.channel()?;
    let get = |ts: &str| {
        snapshot
            .core
            .activity
            .hydrated
            .get(&(channel.to_owned(), ts.to_owned()))
    };
    item.preview_ts()
        .and_then(get)
        .or_else(|| item.ts().and_then(get))
}

fn activity_avatar(
    snapshot: &ShellState,
    workspace: Option<&super_platinum_core::state::Workspace>,
    item: &super_platinum_core::slack::models::ActivityItem,
) -> Option<(String, String)> {
    let workspace = workspace?;
    let user = item.author().or_else(|| {
        activity_hydrated_message(snapshot, item).and_then(|message| message.user.as_deref())
    })?;
    if user == workspace.self_user_id {
        return None;
    }
    let url = workspace.avatar_url(user)?;
    let asset = snapshot.media.register(
        super_platinum_core::MediaAssetKind::Avatar,
        &url,
        "image/jpeg",
        url.contains("slack-edge.com") || url.contains("slack.com"),
    );
    Some((asset.uri(), workspace.display_name(user)))
}
