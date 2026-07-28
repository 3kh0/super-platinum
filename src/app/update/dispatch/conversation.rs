use super::super::*;

pub(super) fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::AccountScoped(_, _) => Task::none(),
        Message::Conversation(crate::app::ConversationMessage::WorkspaceSelected(team)) => {
            select_workspace(app, team)
        }

        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(id)) => {
            app.search = None;
            app.text_selection = None;
            let same_channel = app.active_channel.as_deref() == Some(id.as_str());
            if app
                .editing
                .as_ref()
                .is_some_and(|(channel, _)| channel != &id)
            {
                app.editing = None;
                app.edit_content = Content::new();
            }
            app.active_channel = Some(id.clone());
            if let Some(team) = app.active_team.clone() {
                app.last_active_channels.insert(team.clone(), id.clone());
                if let Some(ws) = app.workspaces.get_mut(&team) {
                    ws.last_active_channel = Some(id.clone());
                    ws.touch_recent(&id);
                    ws.record_visit(&id, crate::state::now_secs());
                }
                mark_workspace_dirty(app, &team);
                if !same_channel {
                    app.chat_paused.remove(&id);
                    app.pending_scroll_to = channel_open_scroll_target(app, &team, &id)
                        .map(|target| (id.clone(), target));
                }
            }
            if app
                .active_thread
                .as_ref()
                .is_some_and(|(channel, _)| channel != &id)
            {
                app.active_thread = None;
                app.thread_open = false;
                app.thread_composer = Content::new();
            }

            let focus = if same_channel {
                Task::none()
            } else {
                focus_active_composer(app)
            };
            if let Some(team) = app.active_team.clone() {
                let needs_load = app
                    .workspaces
                    .get(&team)
                    .map(|ws| !ws.messages.get(&id).map(|cm| cm.loaded).unwrap_or(false))
                    .unwrap_or(false);
                if app.transport.is_some() && needs_load {
                    if let Some(cm) = app
                        .workspaces
                        .get_mut(&team)
                        .and_then(|ws| ws.messages.get_mut(&id))
                    {
                        cm.history_failed = false;
                    }
                    return Task::batch([app.load_history(&team, &id), focus]);
                }
                return Task::batch([
                    hydrate_visible_missing_users(app, &team, &id),
                    hydrate_visible_channels(app, &team, &id),
                    mark_latest_visible(app, &team, &id),
                    load_visible_file_previews(app, &team, &id),
                    load_visible_avatar_previews(app, &team, &id),
                    hydrate_visible_emojis(app, &team, &id),
                    load_visible_emoji_previews(app, &team, &id),
                    scroll_to_pending(app, &id),
                    focus,
                ]);
            }
            focus
        }

        Message::Conversation(crate::app::ConversationMessage::ThreadOpened {
            channel,
            ts,
            unread_range,
        }) => {
            app.text_selection = None;
            app.active_channel = Some(channel.clone());
            app.active_thread = Some((channel.clone(), ts.clone()));
            app.thread_open = true;
            app.thread_unread_marker = None;
            let focus = focus_active_composer(app);
            let Some(team) = app.active_team.clone() else {
                return focus;
            };
            let needs_load = unread_range.is_some()
                || !app
                    .threads
                    .get(&(team.clone(), channel.clone(), ts.clone()))
                    .map(|cm| cm.loaded)
                    .unwrap_or(false);
            if needs_load {
                Task::batch([app.load_thread(&team, &channel, &ts, unread_range), focus])
            } else {
                Task::batch([
                    load_thread_file_previews(app, &team, &channel, &ts),
                    load_thread_avatar_previews(app, &team, &channel, &ts),
                    hydrate_thread_emojis(app, &team, &channel, &ts),
                    load_thread_emoji_previews(app, &team, &channel, &ts),
                    focus,
                ])
            }
        }

        Message::Conversation(crate::app::ConversationMessage::ThreadClosed) => {
            app.text_selection = None;
            app.thread_open = false;
            focus_active_composer(app)
        }

        Message::Conversation(crate::app::ConversationMessage::ThreadDismissed) => {
            if !app.thread_open {
                app.active_thread = None;
                app.thread_composer = Content::new();
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::ThreadLoaded {
            team,
            channel,
            root_ts,
            unread_anchor,
            result,
        }) => {
            if app.active_team.as_deref() != Some(&team) {
                return Task::none();
            }
            match result {
                Ok(page) => {
                    let messages: Vec<_> = page
                        .messages
                        .into_iter()
                        .map(crate::state::visible_message)
                        .collect();
                    let key = (team.clone(), channel.clone(), root_ts.clone());
                    let cm = app.threads.entry(key).or_default();
                    let pending_messages = if unread_anchor.is_some() {
                        cm.messages
                            .iter()
                            .filter(|message| {
                                message.ts.as_deref().is_some_and(|ts| cm.is_pending(ts))
                            })
                            .cloned()
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                    if unread_anchor.is_some() {
                        cm.messages.clear();
                    }
                    let n = messages.len();
                    for msg in messages.clone() {
                        cm.upsert(msg);
                    }
                    for msg in pending_messages {
                        cm.upsert(msg);
                    }
                    cm.loaded = true;
                    if let Some(anchor) = unread_anchor.clone() {
                        app.thread_unread_marker =
                            Some(((team.clone(), channel.clone(), root_ts.clone()), anchor));
                    }
                    tracing::info!(%channel, %root_ts, messages = n, "thread loaded");
                    return Task::batch([
                        hydrate_missing_users(app, &team, &messages),
                        hydrate_message_channels(app, &team, &messages),
                        hydrate_emojis(app, &team, &messages),
                        load_avatar_previews(app, &team, messages),
                        load_thread_file_previews(app, &team, &channel, &root_ts),
                        load_thread_emoji_previews(app, &team, &channel, &root_ts),
                        scroll_thread_to_unread(
                            app,
                            &team,
                            &channel,
                            &root_ts,
                            unread_anchor.as_deref(),
                        ),
                    ]);
                }
                Err(e) => {
                    app.toast(format!("thread failed for {channel}/{root_ts}: {e}"));
                    if is_auth_error(&e) {
                        app.screen = Screen::Login;
                    }
                }
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::ThreadSendPressed) => {
            send_thread_pressed(app)
        }

        Message::Conversation(crate::app::ConversationMessage::ThreadReplySent {
            team,
            channel,
            root_ts,
            client_msg_id,
            result,
        }) => {
            if app.active_team.as_deref() != Some(&team) {
                return Task::none();
            }
            match result {
                Ok(sent) => {
                    if let Some(cm) =
                        app.threads
                            .get_mut(&(team.clone(), channel.clone(), root_ts.clone()))
                    {
                        cm.confirm(
                            &client_msg_id,
                            SlackMessage {
                                ts: Some(sent.ts),
                                thread_ts: Some(root_ts),
                                ..sent.message
                            },
                        );
                    }
                }
                Err(e) => {
                    app.toast(format!("reply failed: {e}"));
                    if is_auth_error(&e) {
                        app.screen = Screen::Login;
                    }
                }
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::ComposerAction {
            target,
            action,
        }) => {
            let is_edit = action.is_edit();
            composer_content_mut(app, target).perform(action);
            if is_edit && target == ComposerTarget::Channel {
                maybe_send_typing(app);
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::ComposerDelete {
            target,
            motion,
        }) => {
            let content = composer_content_mut(app, target);
            content.perform(Action::Select(motion));
            if content.selection().is_some_and(|text| !text.is_empty()) {
                content.perform(Action::Edit(Edit::Backspace));
                if target == ComposerTarget::Channel {
                    maybe_send_typing(app);
                }
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::ComposerFormat { target, mark }) => {
            ui::composer::apply_format(composer_content_mut(app, target), mark);
            if target == ComposerTarget::Channel {
                maybe_send_typing(app);
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::AttachmentPickerOpened(target)) => {
            Task::perform(
                async move {
                    rfd::AsyncFileDialog::new()
                        .set_title("Attach files")
                        .pick_files()
                        .await
                        .into_iter()
                        .flatten()
                        .map(|file| file.path().to_owned())
                        .collect()
                },
                move |paths| {
                    Message::Conversation(crate::app::ConversationMessage::AttachmentsPicked {
                        target,
                        paths,
                    })
                },
            )
        }

        Message::Conversation(crate::app::ConversationMessage::AttachmentsPicked {
            target,
            paths,
        }) => {
            let video_paths = paths
                .iter()
                .filter(|path| is_video(path))
                .cloned()
                .collect::<Vec<_>>();
            add_attachments(app, target, paths);
            video_preview_tasks(video_paths)
        }

        Message::Conversation(crate::app::ConversationMessage::FilesDropped(paths)) => {
            let video_paths = paths
                .iter()
                .filter(|path| is_video(path))
                .cloned()
                .collect::<Vec<_>>();
            let target = drop_target(app);
            add_attachments(app, target, paths);
            video_preview_tasks(video_paths)
        }

        Message::Conversation(crate::app::ConversationMessage::VideoPreviewReady {
            source,
            result,
        }) => {
            if let Ok(preview_path) = result {
                for attachment in app
                    .composer_attachments
                    .iter_mut()
                    .chain(app.thread_composer_attachments.iter_mut())
                    .chain(
                        app.pending_file_messages
                            .iter_mut()
                            .flat_map(|pending| pending.attachments.iter_mut()),
                    )
                    .filter(|attachment| attachment.path == source)
                {
                    attachment.preview_path = Some(preview_path.clone());
                }
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::AttachmentRemoved {
            target,
            id,
        }) => {
            for attachment in attachments_mut(app, target)
                .iter()
                .filter(|attachment| attachment.uploading)
            {
                if let Some(cancel) = &attachment.upload_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            attachments_mut(app, target).retain(|attachment| attachment.id != id);
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::PasteAttachmentsRequested(
            target,
        )) => iced::clipboard::read_files().map(move |result| {
            Message::Conversation(crate::app::ConversationMessage::ClipboardFilesRead {
                target,
                result: result
                    .map(|files| files.iter().cloned().collect())
                    .map_err(|error| format!("{error:?}")),
            })
        }),

        Message::Conversation(crate::app::ConversationMessage::ClipboardFilesRead {
            target,
            result,
        }) => match result {
            Ok(paths) if !paths.is_empty() => {
                add_attachments(app, target, paths);
                Task::none()
            }
            _ => iced::clipboard::read_text().map(move |result| {
                Message::Conversation(crate::app::ConversationMessage::ClipboardTextRead {
                    target,
                    result: result
                        .map(|text| text.as_ref().clone())
                        .map_err(|error| format!("{error:?}")),
                })
            }),
        },

        Message::Conversation(crate::app::ConversationMessage::ClipboardTextRead {
            target,
            result,
        }) => {
            if let Ok(text) = result {
                composer_content_mut(app, target.composer())
                    .perform(Action::Edit(Edit::Paste(Arc::new(text))));
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::AttachmentsSent {
            target,
            team,
            channel,
            thread_ts,
            message_ts,
            client_msg_id,
            result,
        }) => {
            let pending_index = app
                .pending_file_messages
                .iter()
                .position(|pending| pending.client_msg_id == client_msg_id);
            match result {
                Ok(()) => {
                    if let Some(index) = pending_index {
                        for attachment in &mut app.pending_file_messages[index].attachments {
                            attachment.uploading = false;
                            attachment.upload_started = None;
                            if let Some(progress) = &attachment.upload_progress {
                                progress.store(attachment.bytes, Ordering::Relaxed);
                            }
                        }
                    }
                    mark_workspace_dirty(app, &team);
                }
                Err(error) => {
                    let pending =
                        pending_index.map(|index| app.pending_file_messages.remove(index));
                    let messages = match thread_ts.as_ref() {
                        Some(root_ts) => {
                            app.threads
                                .get_mut(&(team.clone(), channel.clone(), root_ts.clone()))
                        }
                        None => app
                            .workspaces
                            .get_mut(&team)
                            .and_then(|ws| ws.messages.get_mut(&channel)),
                    };
                    if let Some(messages) = messages {
                        messages.remove(&message_ts);
                    }
                    let mut attachments = pending
                        .map(|message| message.attachments)
                        .unwrap_or_default();
                    for attachment in &mut attachments {
                        attachment.uploading = false;
                        attachment.upload_started = None;
                        attachment.upload_cancel = None;
                        attachment.upload_progress = None;
                    }
                    attachments_mut(app, target).extend(attachments);
                    if !matches!(error, SlackError::UploadCanceled) {
                        app.toast(format!("file upload failed: {error}"));
                    }
                }
            }
            Task::none()
        }

        Message::Conversation(crate::app::ConversationMessage::SendPressed) => send_pressed(app),

        Message::Conversation(crate::app::ConversationMessage::MessageSent {
            team,
            channel,
            client_msg_id,
            result,
        }) => {
            match result {
                Ok(sent) => {
                    if let Some(cm) = app
                        .workspaces
                        .get_mut(&team)
                        .and_then(|ws| ws.messages.get_mut(&channel))
                    {
                        cm.confirm(
                            &client_msg_id,
                            SlackMessage {
                                ts: Some(sent.ts),
                                ..sent.message
                            },
                        );
                    }
                    mark_workspace_dirty(app, &team);
                }
                Err(e) => app.toast(format!("send failed: {e}")),
            }
            Task::none()
        }

        _ => unreachable!("message routed to the wrong conversation reducer"),
    }
}
