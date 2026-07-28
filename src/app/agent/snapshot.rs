use super::*;

pub fn dump_state(app: &App) -> Value {
    let screen = match app.screen {
        Screen::Login => "login",
        Screen::Loading => "loading",
        Screen::Main => "main",
    };

    let workspaces: Vec<Value> = app
        .workspaces
        .values()
        .map(|ws| {
            json!({
                "team_id": ws.team_id,
                "name": ws.name,
                "channel_count": ws.channels.len(),
                "rt_connected": ws.rt.is_connected(),
            })
        })
        .collect();

    let active_channel_name = app.active_workspace().and_then(|ws| {
        app.active_channel
            .as_ref()
            .and_then(|id| ws.channels.get(id))
            .and_then(|c| c.name.clone())
    });

    let recent_messages = app.active_workspace().and_then(|ws| {
        let channel = app.active_channel.as_ref()?;
        let cm = ws.messages.get(channel)?;
        let msgs: Vec<Value> = cm
            .messages
            .iter()
            .rev()
            .filter(|m| crate::state::is_channel_timeline_visible(m))
            .take(12)
            .map(|m| {
                json!({
                    "ts": m.ts,
                    "user": m.user,
                    "author": ws.message_author_name(m),
                    "text": crate::state::message_text(m),
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Some(msgs)
    });

    let search = app.search.as_ref().map(|s| {
        json!({
            "query": s.query,
            "loading": s.loading,
            "page": s.page,
            "page_count": s.page_count,
            "total": s.total,
            "hit_count": s.hits.len(),
            "hits": s.hits.iter().take(20).map(|h| {
                json!({
                    "channel": h.channel,
                    "channel_label": h.channel_label,
                    "ts": h.message.ts,
                    "text": crate::state::message_text(&h.message),
                    "author": app.active_workspace()
                        .map(|ws| ws.message_author_name(&h.message))
                        .unwrap_or_default(),
                })
            }).collect::<Vec<_>>(),
        })
    });

    let activity = json!({
        "view": main_view_label(app.main_view),
        "loading": app.activity.loading,
        "loaded": app.activity.loaded,
        "selected": app.activity.selected,
        "item_count": app.activity.items.len(),
        "unread": app.active_workspace().and_then(|ws| ws.activity_unread_count),
        "loaded_unread": app.activity.items.iter().filter(|i| i.is_unread).count(),
        "items": app.activity.items.iter().take(20).map(|i| json!({
            "key": i.key,
            "kind": i.item.kind,
            "channel": i.channel(),
            "ts": i.ts(),
            "thread_ts": i.thread_ts(),
            "identity": i.identity(),
            "author": i.author(),
            "is_unread": i.is_unread,
        })).collect::<Vec<_>>(),
    });

    let dms = json!({
        "loading": app.dms.loading,
        "loaded": app.dms.loaded,
        "unread_only": app.dms.unread_only,
        "filter": app.dms.filter,
        "entry_count": app.dms.entries.len(),
        "next_cursor": app.dms.next_cursor.is_some(),
        "entries": app.dms.entries.iter().take(20).map(|e| json!({
            "id": e.id,
            "latest": e.latest,
            "user": e.channel.as_ref().and_then(|c| c.user.as_ref()),
            "is_mpim": e.channel.as_ref().map(|c| c.is_mpim).unwrap_or(false),
            "text": e.message.as_ref().and_then(|m| m.text.as_deref()),
        })).collect::<Vec<_>>(),
    });

    let threads = json!({
        "loading": app.threads_view.loading,
        "loaded": app.threads_view.loaded,
        "vip_only": app.threads_view.vip_only,
        "has_more": app.threads_view.has_more,
        "item_count": app.threads_view.items.len(),
        "selected": app.threads_view.selected.as_ref().map(|(channel, root_ts)| {
            json!({"channel": channel, "root_ts": root_ts})
        }),
        "items": app.threads_view.items.iter().take(20).map(|item| json!({
            "channel": item.channel(),
            "root_ts": item.root_ts(),
            "latest_ts": item.latest_ts(),
            "unread_reply_count": item.unread_replies.len(),
            "visible_reply_count": item.replies().len(),
        })).collect::<Vec<_>>(),
    });

    let profile = app.profile_pane.as_ref().map(|pane| {
        let details = app.active_workspace().and_then(|workspace| {
            workspace
                .users
                .get(&pane.user)
                .and_then(|user| user.profile.as_ref())
        });
        json!({
            "user": pane.user,
            "loading": pane.loading,
            "error": pane.error,
            "title": details.and_then(|profile| profile.title.as_deref()),
            "pronouns": details.and_then(|profile| profile.pronouns.as_deref()),
            "email": details.and_then(|profile| profile.email.as_deref()),
            "custom_field_count": details.map(|profile| profile.fields.len()).unwrap_or(0),
        })
    });

    json!({
        "screen": screen,
        "signed_in": app.session.is_some(),
        "main_view": main_view_label(app.main_view),
        "activity": activity,
        "dms": dms,
        "threads": threads,
        "profile": profile,
        "active_team": app.active_team,
        "active_channel": app.active_channel,
        "active_channel_name": active_channel_name,
        "thread_open": app.thread_open,
        "active_thread": app.active_thread.as_ref().map(|(c, ts)| json!({"channel": c, "ts": ts})),
        "palette_open": app.palette_open,
        "palette": app.palette.as_ref().map(|p| {
            json!({
                "query": p.query,
                "selected": p.selected,
                "remote_seq": p.remote_seq,
                "entry_count": p.entries.len(),
                "entries": palette_entries_json(app),
            })
        }),
        "settings_open": app.settings_open || app.show_settings,
        "account_menu_open": app.account_menu_open || app.show_account_menu,
        "search_input": app.search_input,
        "search": search,
        "workspaces": workspaces,
        "recent_messages": recent_messages,
        "toasts": app.errors.iter().rev().take(8).map(|t| &t.text).collect::<Vec<_>>(),
        "allow_destructive": allow_destructive(),
        "agent_socket": socket_path().display().to_string(),
    })
}

pub(super) fn palette_entries_json(app: &App) -> Vec<Value> {
    app.palette
        .as_ref()
        .map(|p| {
            p.entries
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let target = match &e.target {
                        PaletteTarget::Channel(id) => json!({"kind": "channel", "id": id}),
                        PaletteTarget::User { user, dm } => {
                            json!({"kind": "user", "user": user, "dm": dm})
                        }
                    };
                    json!({
                        "index": i,
                        "label": e.label,
                        "sublabel": e.sublabel,
                        "target": target,
                        "selected": i == p.selected,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn help_data() -> Value {
    json!({
        "commands": [
            {"cmd": "ping", "desc": "liveness check"},
            {"cmd": "help", "desc": "this help"},
            {"cmd": "state", "desc": "JSON snapshot of UI state"},
            {"cmd": "open-palette", "desc": "open quick switcher"},
            {"cmd": "close-palette", "desc": "close quick switcher"},
            {"cmd": "set-query", "args": {"query": "string"}, "desc": "set palette query"},
            {"cmd": "type", "args": {"text": "string"}, "desc": "alias for set-query"},
            {"cmd": "move", "args": {"delta": "isize"}, "desc": "move palette selection"},
            {"cmd": "submit", "desc": "activate selected palette entry"},
            {"cmd": "select-entry", "args": {"index": "usize"}, "desc": "activate palette entry by index"},
            {"cmd": "select-channel", "args": {"channel": "id or name"}, "desc": "open a channel"},
            {"cmd": "select-workspace", "args": {"team": "team id"}, "desc": "switch workspace"},
            {"cmd": "search", "args": {"query": "string"}, "desc": "run message search"},
            {"cmd": "clear-search", "desc": "close search"},
            {"cmd": "open-settings", "desc": "open settings"},
            {"cmd": "close-settings", "desc": "close settings"},
            {"cmd": "open-profile", "args": {"user": "user id"}, "desc": "open a user profile pane"},
            {"cmd": "close-profile", "desc": "close the profile pane"},
            {"cmd": "screenshot", "args": {"path": "optional"}, "desc": "capture window PNG"},
            {"cmd": "main-view", "args": {"view": "home|unreads|threads|dms|activity"}, "desc": "switch main surface"},
            {"cmd": "activity-select", "args": {"index": "usize"}, "desc": "open an activity item in the right panel"},
            {"cmd": "toast", "args": {"text": "string"}, "desc": "show a toast"},
            {"cmd": "allow-destructive", "args": {"enabled": "bool"}, "desc": "allow send/etc"},
            {"cmd": "send", "desc": "send composer (requires allow-destructive)"},
        ],
        "notes": [
            "Start app with SNACK_AGENT=1 cargo run",
            "Socket default: $TMPDIR/snack-agent.sock (override SNACK_AGENT_SOCK)",
            "Use scripts/agentctl.sh for a CLI wrapper",
            "Live data uses your real Slack session",
        ]
    })
}
