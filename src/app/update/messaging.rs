use super::*;

pub(super) fn send_pressed(app: &mut App) -> Task<Message> {
    let text = app.composer.text().trim().to_owned();
    if text.is_empty() && app.composer_attachments.is_empty() {
        return Task::none();
    }
    let (Some(team), Some(channel)) = (app.active_team.clone(), app.active_channel.clone()) else {
        return Task::none();
    };

    if !app.composer_attachments.is_empty() {
        return send_attachments(app, AttachTarget::Channel, team, channel, None, text);
    }

    let seq = next_seq(app);
    let client_msg_id = uuid::Uuid::new_v4().to_string();
    let ts = format!("{}.{:06}", chrono::Utc::now().timestamp(), seq);

    let Some(ws) = app.workspaces.get_mut(&team) else {
        return Task::none();
    };
    let self_user = ws.self_user_id.clone();

    let pending = SlackMessage {
        user: Some(self_user),
        kind: Some("message".to_owned()),
        ts: Some(ts.clone()),
        client_msg_id: Some(client_msg_id.clone()),
        text: Some(text.clone()),
        channel: Some(channel.clone()),
        ..Default::default()
    };
    let cm = ws.messages.entry(channel.clone()).or_default();
    cm.upsert(pending);
    cm.pending.push(ts);
    app.composer = Content::new();
    mark_workspace_dirty(app, &team);
    app.pending_scroll_to = Some((channel.clone(), PendingScrollTarget::Latest));
    let scroll = scroll_to_pending(app, &channel);

    let Some((transport, session)) = app.live() else {
        return scroll;
    };
    let Some(ws_session) = session.workspaces.get(&team) else {
        return scroll;
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let send_channel = channel.clone();
    let send = Task::perform(
        async move {
            api::send_message(&transport, &client, &ws_session, send_channel, text, None).await
        },
        move |result| {
            Message::Conversation(crate::app::ConversationMessage::MessageSent {
                team: team.clone(),
                channel: channel.clone(),
                client_msg_id: client_msg_id.clone(),
                result,
            })
        },
    );
    Task::batch([scroll, send])
}

pub(super) fn send_thread_pressed(app: &mut App) -> Task<Message> {
    let text = app.thread_composer.text().trim().to_owned();
    if text.is_empty() && app.thread_composer_attachments.is_empty() {
        return Task::none();
    }
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let Some((channel, root_ts)) = app.active_thread.clone() else {
        return Task::none();
    };

    if !app.thread_composer_attachments.is_empty() {
        return send_attachments(
            app,
            AttachTarget::Thread,
            team,
            channel,
            Some(root_ts),
            text,
        );
    }

    let seq = next_seq(app);
    let client_msg_id = uuid::Uuid::new_v4().to_string();
    let ts = format!("{}.{:06}", chrono::Utc::now().timestamp(), seq);

    let Some(ws) = app.workspaces.get(&team) else {
        return Task::none();
    };
    let self_user = ws.self_user_id.clone();
    let pending = SlackMessage {
        user: Some(self_user),
        kind: Some("message".to_owned()),
        ts: Some(ts.clone()),
        client_msg_id: Some(client_msg_id.clone()),
        text: Some(text.clone()),
        channel: Some(channel.clone()),
        thread_ts: Some(root_ts.clone()),
        ..Default::default()
    };
    let cm = app
        .threads
        .entry((team.clone(), channel.clone(), root_ts.clone()))
        .or_default();
    cm.upsert(pending);
    cm.pending.push(ts);
    app.thread_composer = Content::new();

    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let send_channel = channel.clone();
    let send_thread_ts = root_ts.clone();
    Task::perform(
        async move {
            api::send_message(
                &transport,
                &client,
                &ws_session,
                send_channel,
                text,
                Some(send_thread_ts),
            )
            .await
        },
        move |result| {
            Message::Conversation(crate::app::ConversationMessage::ThreadReplySent {
                team: team.clone(),
                channel: channel.clone(),
                root_ts: root_ts.clone(),
                client_msg_id: client_msg_id.clone(),
                result,
            })
        },
    )
}

pub(super) fn send_attachments(
    app: &mut App,
    target: AttachTarget,
    team: TeamId,
    channel: ChannelId,
    thread_ts: Option<MessageTs>,
    text: String,
) -> Task<Message> {
    let Some((transport, session)) = app.live() else {
        app.toast("file upload unavailable while offline");
        return Task::none();
    };
    let Some(workspace) = session.workspaces.get(&team) else {
        app.toast("file upload unavailable for this workspace");
        return Task::none();
    };
    let transport = transport.clone();
    let workspace = workspace.clone();
    let client = app.client.clone();
    let upload_cancel = Arc::new(AtomicBool::new(false));
    if attachments_mut(app, target)
        .iter()
        .any(|attachment| attachment.uploading)
    {
        return Task::none();
    }
    let mut attachments = std::mem::take(attachments_mut(app, target));
    let files = attachments
        .iter_mut()
        .map(|attachment| {
            attachment.uploading = true;
            attachment.upload_started = Some(Instant::now());
            attachment.upload_cancel = Some(upload_cancel.clone());
            let progress = Arc::new(AtomicU64::new(0));
            attachment.upload_progress = Some(progress.clone());
            (attachment.path.clone(), progress)
        })
        .collect::<Vec<_>>();
    let seq = next_seq(app);
    let client_msg_id = uuid::Uuid::new_v4().to_string();
    let message_ts = format!("{}.{:06}", chrono::Utc::now().timestamp(), seq);
    let Some(ws) = app.workspaces.get(&team) else {
        attachments_mut(app, target).extend(attachments);
        return Task::none();
    };
    let pending_message = SlackMessage {
        user: Some(ws.self_user_id.clone()),
        kind: Some("message".to_owned()),
        ts: Some(message_ts.clone()),
        client_msg_id: Some(client_msg_id.clone()),
        text: Some(text.clone()),
        channel: Some(channel.clone()),
        thread_ts: thread_ts.clone(),
        ..Default::default()
    };
    match thread_ts.as_ref() {
        Some(root_ts) => {
            let messages = app
                .threads
                .entry((team.clone(), channel.clone(), root_ts.clone()))
                .or_default();
            messages.upsert(pending_message);
            messages.pending.push(message_ts.clone());
        }
        None => {
            let messages = app
                .workspaces
                .get_mut(&team)
                .expect("workspace checked above")
                .messages
                .entry(channel.clone())
                .or_default();
            messages.upsert(pending_message);
            messages.pending.push(message_ts.clone());
        }
    }
    app.pending_file_messages.push(PendingFileMessage {
        team: team.clone(),
        channel: channel.clone(),
        thread_ts: thread_ts.clone(),
        message_ts: message_ts.clone(),
        client_msg_id: client_msg_id.clone(),
        text: text.clone(),
        attachments,
    });
    *composer_content_mut(app, target.composer()) = Content::new();
    mark_workspace_dirty(app, &team);
    let scroll = if target == AttachTarget::Channel {
        app.pending_scroll_to = Some((channel.clone(), PendingScrollTarget::Latest));
        scroll_to_pending(app, &channel)
    } else {
        Task::none()
    };
    let sent_team = team.clone();
    let sent_channel = channel.clone();
    let sent_thread_ts = thread_ts.clone();
    let sent_message_ts = message_ts.clone();
    let sent_client_msg_id = client_msg_id.clone();
    let upload = Task::perform(
        async move {
            api::upload_files(
                &transport,
                &client,
                &workspace,
                channel,
                thread_ts,
                text,
                files,
                upload_cancel,
            )
            .await
        },
        move |result| {
            Message::Conversation(crate::app::ConversationMessage::AttachmentsSent {
                target,
                team: sent_team.clone(),
                channel: sent_channel.clone(),
                thread_ts: sent_thread_ts.clone(),
                message_ts: sent_message_ts.clone(),
                client_msg_id: sent_client_msg_id.clone(),
                result,
            })
        },
    );
    Task::batch([scroll, upload])
}

pub(super) fn toggle_reaction(
    app: &mut App,
    channel: ChannelId,
    ts: MessageTs,
    name: String,
) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let Some(ws) = app.workspaces.get_mut(&team) else {
        return Task::none();
    };
    let user = ws.self_user_id.clone();
    let active = ws
        .messages
        .get(&channel)
        .and_then(|cm| reaction_has_user_in(cm, &ts, &name, &user))
        .or_else(|| {
            app.threads
                .iter()
                .filter(|((thread_team, thread_channel, _), _)| {
                    thread_team == &team && thread_channel == &channel
                })
                .find_map(|(_, cm)| reaction_has_user_in(cm, &ts, &name, &user))
        });
    let Some(active) = active else {
        return Task::none();
    };
    let adding = !active;

    let mut applied = false;
    if let Some(cm) = ws.messages.get_mut(&channel) {
        if reaction_has_user_in(cm, &ts, &name, &user).is_some() {
            cm.apply_reaction(&ts, &user, &name, adding);
            applied = true;
        }
    }
    if !applied {
        for ((thread_team, thread_channel, _), cm) in &mut app.threads {
            if thread_team == &team
                && thread_channel == &channel
                && reaction_has_user_in(cm, &ts, &name, &user).is_some()
            {
                cm.apply_reaction(&ts, &user, &name, adding);
                break;
            }
        }
    }
    mark_workspace_dirty(app, &team);

    let send_channel = channel.clone();
    let send_ts = ts.clone();
    let send_name = name.clone();
    Task::perform(
        async move {
            if adding {
                api::add_reaction(
                    &transport,
                    &client,
                    &ws_session,
                    send_channel,
                    send_ts,
                    send_name,
                )
                .await
            } else {
                api::remove_reaction(
                    &transport,
                    &client,
                    &ws_session,
                    send_channel,
                    send_ts,
                    send_name,
                )
                .await
            }
        },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::ReactionUpdated {
                team: team.clone(),
                channel: channel.clone(),
                ts: ts.clone(),
                user: user.clone(),
                name: name.clone(),
                added: adding,
                result,
            })
        },
    )
}

pub(super) fn edit_submit(app: &mut App) -> Task<Message> {
    let text = app.edit_content.text().trim().to_owned();
    let Some((channel, ts)) = app.editing.clone() else {
        return Task::none();
    };
    if text.is_empty() {
        app.toast("message cannot be empty");
        return Task::none();
    }
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };

    app.editing = None;
    app.edit_content = Content::new();
    apply_message_edit(app, &team, &channel, &ts, Some(text.clone()));
    mark_workspace_dirty(app, &team);

    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let send_channel = channel.clone();
    let send_ts = ts.clone();
    Task::perform(
        async move {
            api::edit_message(
                &transport,
                &client,
                &ws_session,
                send_channel,
                send_ts,
                text,
            )
            .await
        },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::MessageEdited {
                team: team.clone(),
                channel: channel.clone(),
                ts: ts.clone(),
                result,
            })
        },
    )
}

pub(super) fn delete_pressed(app: &mut App, channel: ChannelId, ts: MessageTs) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    if app.editing.as_ref() == Some(&(channel.clone(), ts.clone())) {
        app.editing = None;
        app.edit_content = Content::new();
    }
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let send_channel = channel.clone();
    let send_ts = ts.clone();
    Task::perform(
        async move {
            api::delete_message(&transport, &client, &ws_session, send_channel, send_ts).await
        },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::MessageDeleted {
                team: team.clone(),
                channel: channel.clone(),
                ts: ts.clone(),
                result,
            })
        },
    )
}

pub(super) fn find_message_text(app: &App, channel: &str, ts: &str) -> Option<String> {
    let team = app.active_team.as_deref()?;
    let ws = app.workspaces.get(team)?;
    if let Some(text) = ws
        .messages
        .get(channel)
        .and_then(|cm| cm.messages.iter().find(|m| m.ts.as_deref() == Some(ts)))
        .and_then(|m| m.text.clone())
    {
        return Some(text);
    }
    app.threads
        .iter()
        .filter(|((t, c, _), _)| t == team && c == channel)
        .find_map(|(_, cm)| {
            cm.messages
                .iter()
                .find(|m| m.ts.as_deref() == Some(ts))
                .and_then(|m| m.text.clone())
        })
}

pub(super) fn selected_text(app: &App) -> Option<String> {
    let selection = app.text_selection.as_ref()?;
    if selection.anchor.surface != selection.focus.surface {
        return None;
    }
    let ws = app.active_workspace()?;
    let messages = selected_surface_messages(app, ws, &selection.anchor.surface)?;

    let mut parts = Vec::new();
    for (index, msg) in messages.into_iter().enumerate() {
        let full = ui::message::selectable_copy_text(ws, msg);
        if full.is_empty() {
            continue;
        }
        let len = full.graphemes(true).count();
        if let Some((lo, hi)) = selection_range_for_index(selection, index, len) {
            let text = slice_graphemes(&full, lo, hi);
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }

    (!parts.is_empty()).then(|| parts.join("\n"))
}

pub(super) fn selected_surface_messages<'a>(
    app: &'a App,
    ws: &'a Workspace,
    surface: &TextSelectionSurface,
) -> Option<Vec<&'a SlackMessage>> {
    match surface {
        TextSelectionSurface::Channel { channel } => {
            let cm = ws.messages.get(channel)?;
            let mut visible: Vec<_> = cm
                .messages
                .iter()
                .rev()
                .filter(|m| crate::state::is_channel_timeline_visible(m))
                .take(ui::channel::VISIBLE_MESSAGE_LIMIT)
                .collect();
            visible.reverse();
            Some(visible)
        }
        TextSelectionSurface::Thread { channel, root_ts } => {
            let team = app.active_team.as_ref()?;
            let replies = app
                .threads
                .get(&(team.clone(), channel.clone(), root_ts.clone()));
            match replies {
                Some(cm) if !cm.messages.is_empty() => Some(cm.messages.iter().collect()),
                _ => ui::thread::root_message(ws, channel, root_ts).map(|root| vec![root]),
            }
        }
    }
}

pub(super) fn selection_range_for_index(
    selection: &TextSelection,
    index: usize,
    len: usize,
) -> Option<(usize, usize)> {
    if len == 0 {
        return None;
    }
    let anchor = &selection.anchor;
    let focus = &selection.focus;
    let start_index = anchor.message_index.min(focus.message_index);
    let end_index = anchor.message_index.max(focus.message_index);
    if index < start_index || index > end_index {
        return None;
    }

    let anchor_offset = anchor.offset.min(len - 1);
    let focus_offset = focus.offset.min(len - 1);
    let forward = anchor.message_index < focus.message_index
        || (anchor.message_index == focus.message_index && anchor.offset <= focus.offset);

    if anchor.message_index == focus.message_index {
        return Some((
            anchor_offset.min(focus_offset),
            anchor_offset.max(focus_offset),
        ));
    }
    if index != anchor.message_index && index != focus.message_index {
        return Some((0, len - 1));
    }
    if forward {
        if index == anchor.message_index {
            Some((anchor_offset, len - 1))
        } else {
            Some((0, focus_offset))
        }
    } else if index == focus.message_index {
        Some((focus_offset, len - 1))
    } else {
        Some((0, anchor_offset))
    }
}

pub(super) fn slice_graphemes(value: &str, lo: usize, hi: usize) -> String {
    value
        .graphemes(true)
        .skip(lo)
        .take(hi.saturating_sub(lo) + 1)
        .collect()
}

pub(super) fn apply_message_edit(
    app: &mut App,
    team: &str,
    channel: &str,
    ts: &str,
    text: Option<String>,
) {
    if let Some(msg) = app
        .workspaces
        .get_mut(team)
        .and_then(|ws| ws.messages.get_mut(channel))
        .and_then(|cm| cm.messages.iter_mut().find(|m| m.ts.as_deref() == Some(ts)))
    {
        mark_edited(msg, text.clone());
    }
    for ((thread_team, thread_channel, _), cm) in &mut app.threads {
        if thread_team == team && thread_channel == channel {
            if let Some(msg) = cm.messages.iter_mut().find(|m| m.ts.as_deref() == Some(ts)) {
                mark_edited(msg, text.clone());
            }
        }
    }
}

pub(super) fn mark_edited(msg: &mut SlackMessage, text: Option<String>) {
    if let Some(text) = text {
        msg.text = Some(text);
    }
    if msg.edited.is_none() {
        msg.edited = Some(serde_json::json!({ "ts": msg.ts.clone() }));
    }
}

pub(super) fn remove_message_everywhere(app: &mut App, team: &str, channel: &str, ts: &str) {
    if let Some(cm) = app
        .workspaces
        .get_mut(team)
        .and_then(|ws| ws.messages.get_mut(channel))
    {
        cm.remove(ts);
    }
    for ((thread_team, thread_channel, _), cm) in &mut app.threads {
        if thread_team == team && thread_channel == channel {
            cm.remove(ts);
        }
    }
}
