use super::*;

pub fn handle(app: &mut App, id: u64, command: AgentCommand) -> iced::Task<Message> {
    match command {
        AgentCommand::Ping | AgentCommand::Help => {
            complete(id, AgentResponse::ok(id, json!({})));
            iced::Task::none()
        }
        AgentCommand::State => {
            complete(id, AgentResponse::ok(id, dump_state(app)));
            iced::Task::none()
        }
        AgentCommand::AllowDestructive { enabled } => {
            set_allow_destructive(enabled);
            complete(
                id,
                AgentResponse::ok(id, json!({ "allow_destructive": allow_destructive() })),
            );
            iced::Task::none()
        }
        AgentCommand::OpenPalette => {
            let task = if app.palette_open {
                iced::Task::none()
            } else {
                update(
                    app,
                    Message::Discovery(crate::app::DiscoveryMessage::PaletteToggled),
                )
            };
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "palette_open": app.palette_open,
                        "entries": palette_entries_json(app),
                    }),
                ),
            );
            task
        }
        AgentCommand::ClosePalette => {
            let task = if app.palette.is_some() {
                let mut t = update(
                    app,
                    Message::Discovery(crate::app::DiscoveryMessage::PaletteClosed),
                );
                t = t.chain(update(
                    app,
                    Message::Discovery(crate::app::DiscoveryMessage::PaletteDismissed),
                ));
                t
            } else {
                iced::Task::none()
            };
            complete(id, AgentResponse::ok(id, json!({ "palette_open": false })));
            task
        }
        AgentCommand::SetQuery { query } => {
            if app.palette.is_none() {
                let _ = update(
                    app,
                    Message::Discovery(crate::app::DiscoveryMessage::PaletteToggled),
                );
            }
            let task = update(
                app,
                Message::Discovery(crate::app::DiscoveryMessage::PaletteQueryChanged(query)),
            );
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "palette_open": app.palette_open,
                        "query": app.palette.as_ref().map(|p| p.query.clone()).unwrap_or_default(),
                        "entries": palette_entries_json(app),
                        "selected": app.palette.as_ref().map(|p| p.selected).unwrap_or(0),
                    }),
                ),
            );
            task
        }
        AgentCommand::Type { text } => handle(app, id, AgentCommand::SetQuery { query: text }),
        AgentCommand::Move { delta } => {
            if app.palette.is_none() {
                complete(id, AgentResponse::err(id, "palette is not open"));
                return iced::Task::none();
            }
            let task = update(
                app,
                Message::Discovery(crate::app::DiscoveryMessage::PaletteMoved(delta)),
            );
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "selected": app.palette.as_ref().map(|p| p.selected).unwrap_or(0),
                        "entries": palette_entries_json(app),
                    }),
                ),
            );
            task
        }
        AgentCommand::Submit => {
            if app.palette.is_none() {
                complete(id, AgentResponse::err(id, "palette is not open"));
                return iced::Task::none();
            }
            let selected = app.palette.as_ref().map(|p| p.selected).unwrap_or(0);
            let label = app
                .palette
                .as_ref()
                .and_then(|p| p.entries.get(selected))
                .map(|e| e.label.clone());
            let task = update(
                app,
                Message::Discovery(crate::app::DiscoveryMessage::PaletteSubmitted),
            );
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "submitted_index": selected,
                        "submitted_label": label,
                        "active_channel": app.active_channel,
                        "palette_open": app.palette_open,
                    }),
                ),
            );
            task
        }
        AgentCommand::SelectEntry { index } => {
            if app.palette.is_none() {
                complete(id, AgentResponse::err(id, "palette is not open"));
                return iced::Task::none();
            }
            let task = update(
                app,
                Message::Discovery(crate::app::DiscoveryMessage::PaletteEntryPressed(index)),
            );
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "index": index,
                        "active_channel": app.active_channel,
                        "palette_open": app.palette_open,
                    }),
                ),
            );
            task
        }
        AgentCommand::SelectChannel { channel } => match resolve_channel(app, &channel) {
            Some(id_channel) => {
                let task = update(
                    app,
                    Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
                        id_channel.clone(),
                    )),
                );
                complete(
                    id,
                    AgentResponse::ok(
                        id,
                        json!({
                            "active_channel": id_channel,
                            "active_team": app.active_team,
                        }),
                    ),
                );
                task
            }
            None => {
                complete(
                    id,
                    AgentResponse::err(id, format!("channel not found: {channel}")),
                );
                iced::Task::none()
            }
        },
        AgentCommand::SelectWorkspace { team } => {
            if app.workspaces.contains_key(&team) {
                let task = update(
                    app,
                    Message::Conversation(crate::app::ConversationMessage::WorkspaceSelected(
                        team.clone(),
                    )),
                );
                complete(
                    id,
                    AgentResponse::ok(
                        id,
                        json!({
                            "active_team": app.active_team,
                            "active_channel": app.active_channel,
                        }),
                    ),
                );
                task
            } else {
                complete(
                    id,
                    AgentResponse::err(id, format!("workspace not found: {team}")),
                );
                iced::Task::none()
            }
        }
        AgentCommand::Search { query } => {
            app.search_input = query.clone();
            let task = update(
                app,
                Message::Discovery(crate::app::DiscoveryMessage::SearchSubmitted),
            );
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "query": query,
                        "search_active": app.search.is_some(),
                        "loading": app.search.as_ref().map(|s| s.loading).unwrap_or(false),
                    }),
                ),
            );
            task
        }
        AgentCommand::ClearSearch => {
            let task = update(
                app,
                Message::Discovery(crate::app::DiscoveryMessage::SearchCleared),
            );
            complete(id, AgentResponse::ok(id, json!({ "search_active": false })));
            task
        }
        AgentCommand::OpenSettings => {
            let task = update(
                app,
                Message::Runtime(crate::app::RuntimeMessage::SettingsOpened),
            );
            complete(id, AgentResponse::ok(id, json!({ "settings_open": true })));
            task
        }
        AgentCommand::CloseSettings => {
            let mut task = update(
                app,
                Message::Runtime(crate::app::RuntimeMessage::SettingsClosed),
            );
            task = task.chain(update(
                app,
                Message::Runtime(crate::app::RuntimeMessage::SettingsDismissed),
            ));
            complete(id, AgentResponse::ok(id, json!({ "settings_open": false })));
            task
        }
        AgentCommand::OpenProfile { user } => {
            let task = update(
                app,
                Message::Workspace(crate::app::WorkspaceMessage::ProfilePressed(user.clone())),
            );
            complete(
                id,
                AgentResponse::ok(id, json!({ "profile_user": user, "loading": true })),
            );
            task
        }
        AgentCommand::CloseProfile => {
            let task = update(
                app,
                Message::Workspace(crate::app::WorkspaceMessage::ProfileDismissed),
            );
            complete(id, AgentResponse::ok(id, json!({ "profile_user": null })));
            task
        }
        AgentCommand::Screenshot { path } => {
            let path = resolve_screenshot_path(path);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            window::latest().then(move |maybe_id| {
                let path = path.clone();
                match maybe_id {
                    Some(window_id) => {
                        let path = path.clone();
                        window::screenshot(window_id).map(move |screenshot| {
                            Message::Runtime(crate::app::RuntimeMessage::AgentScreenshotCaptured {
                                id,
                                path: path.clone(),
                                result: Ok(screenshot),
                            })
                        })
                    }
                    None => iced::Task::done(Message::Runtime(
                        crate::app::RuntimeMessage::AgentScreenshotCaptured {
                            id,
                            path,
                            result: Err("no window available for screenshot".into()),
                        },
                    )),
                }
            })
        }
        AgentCommand::Send => {
            let task = update(
                app,
                Message::Conversation(crate::app::ConversationMessage::SendPressed),
            );
            complete(id, AgentResponse::ok(id, json!({ "sent": true })));
            task
        }
        AgentCommand::Toast { text } => {
            app.toast(text.clone());
            complete(id, AgentResponse::ok(id, json!({ "toast": text })));
            iced::Task::none()
        }
        AgentCommand::MainView { view } => {
            let target = match view.trim().to_ascii_lowercase().as_str() {
                "activity" | "notifications" | "bell" => crate::state::MainView::Activity,
                "dms" | "dm" | "direct-messages" => crate::state::MainView::Dms,
                "unreads" | "unread" => crate::state::MainView::Unreads,
                "home" | "channels" => crate::state::MainView::Home,
                other => {
                    complete(
                        id,
                        AgentResponse::err(id, format!("unknown main view: {other}")),
                    );
                    return iced::Task::none();
                }
            };
            let task = update(
                app,
                Message::Runtime(crate::app::RuntimeMessage::MainViewSelected(target)),
            );
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "main_view": main_view_label(app.main_view),
                        "activity_loading": app.activity.loading,
                        "activity_item_count": app.activity.items.len(),
                        "dms_loading": app.dms.loading,
                        "dms_entry_count": app.dms.entries.len(),
                    }),
                ),
            );
            task
        }
        AgentCommand::ActivitySelect { index } => {
            let Some(key) = app.activity.items.get(index).map(|i| i.key.clone()) else {
                complete(
                    id,
                    AgentResponse::err(id, format!("no activity item at {index}")),
                );
                return iced::Task::none();
            };
            let task = update(
                app,
                Message::Runtime(crate::app::RuntimeMessage::ActivitySelected(key.clone())),
            );
            complete(
                id,
                AgentResponse::ok(
                    id,
                    json!({
                        "key": key,
                        "active_channel": app.active_channel,
                        "active_thread": app.active_thread.as_ref().map(|(c, ts)| json!({"channel": c, "ts": ts})),
                        "thread_open": app.thread_open,
                    }),
                ),
            );
            task
        }
    }
}

pub(super) fn main_view_label(view: crate::state::MainView) -> &'static str {
    match view {
        crate::state::MainView::Home => "home",
        crate::state::MainView::Unreads => "unreads",
        crate::state::MainView::Dms => "dms",
        crate::state::MainView::Activity => "activity",
    }
}

pub fn handle_screenshot(
    id: u64,
    path: PathBuf,
    result: Result<Screenshot, String>,
) -> iced::Task<Message> {
    match result {
        Ok(screenshot) => match write_png(&path, &screenshot) {
            Ok(()) => {
                complete(
                    id,
                    AgentResponse::ok(
                        id,
                        json!({
                            "path": path.display().to_string(),
                            "width": screenshot.size.width,
                            "height": screenshot.size.height,
                        }),
                    ),
                );
            }
            Err(e) => complete(id, AgentResponse::err(id, e)),
        },
        Err(e) => complete(id, AgentResponse::err(id, e)),
    }
    iced::Task::none()
}

fn write_png(path: &Path, screenshot: &Screenshot) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }
    let file = std::fs::File::create(path).map_err(|e| format!("create png: {e}"))?;
    let mut encoder = png::Encoder::new(file, screenshot.size.width, screenshot.size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("png header: {e}"))?;
    writer
        .write_image_data(screenshot.as_ref())
        .map_err(|e| format!("png data: {e}"))?;
    writer.finish().map_err(|e| format!("png finish: {e}"))?;
    Ok(())
}

fn resolve_screenshot_path(path: Option<String>) -> PathBuf {
    match path {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => {
            let dir = PathBuf::from("tmp/agent-ui");
            let _ = std::fs::create_dir_all(&dir);
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            dir.join(format!("live-{stamp}.png"))
        }
    }
}

fn resolve_channel(app: &App, needle: &str) -> Option<String> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let ws = app.active_workspace()?;
    if ws.channels.contains_key(needle) {
        return Some(needle.to_owned());
    }
    let lower = needle.trim_start_matches('#').to_ascii_lowercase();
    ws.channels
        .values()
        .find(|c| {
            c.name
                .as_deref()
                .map(|n| n.eq_ignore_ascii_case(&lower))
                .unwrap_or(false)
                || c.id.eq_ignore_ascii_case(needle)
        })
        .map(|c| c.id.clone())
}
