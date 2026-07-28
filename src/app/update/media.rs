use super::*;

pub(super) fn channel_scrolled(
    app: &mut App,
    channel: ChannelId,
    y: f32,
    bottom_gap: f32,
) -> Task<Message> {
    if app.active_channel.as_deref() != Some(channel.as_str()) {
        return Task::none();
    }
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    if app
        .pending_scroll_to
        .as_ref()
        .is_some_and(|(pending_channel, _)| pending_channel == &channel)
    {
        return Task::none();
    }
    let pause = update_chat_pause(app, &channel, y, bottom_gap);
    let load_older = load_older_on_scroll(app, &team, &channel, y);
    Task::batch([pause, load_older])
}

pub(super) fn update_chat_pause(
    app: &mut App,
    channel: &ChannelId,
    y: f32,
    bottom_gap: f32,
) -> Task<Message> {
    let paused = app.chat_paused.contains_key(channel);
    if bottom_gap > CHAT_PIN_BOTTOM_PX {
        if !paused {
            app.chat_paused.insert(channel.clone(), 0);
            return operation::scroll_to(
                ui::channel::scrollable_id(channel),
                AbsoluteOffset { x: 0.0, y },
            );
        }
    } else if paused {
        app.chat_paused.remove(channel);
        return operation::scroll_to(
            ui::channel::scrollable_id(channel),
            AbsoluteOffset { x: 0.0, y: 0.0 },
        );
    }
    Task::none()
}

pub(super) fn load_older_on_scroll(
    app: &mut App,
    team: &str,
    channel: &ChannelId,
    y: f32,
) -> Task<Message> {
    if app.transport.is_none() {
        return Task::none();
    }
    let oldest = {
        let Some(cm) = app
            .workspaces
            .get_mut(team)
            .and_then(|ws| ws.messages.get_mut(channel))
        else {
            return Task::none();
        };
        if !should_load_older_history(cm, y) {
            return Task::none();
        }
        let Some(oldest) = cm.oldest_ts() else {
            return Task::none();
        };
        cm.history_loading_older = true;
        oldest
    };
    app.pending_scroll_to = Some((
        channel.clone(),
        PendingScrollTarget::Message(oldest.clone()),
    ));
    app.load_older_history(team, channel, oldest)
}

pub(in crate::app) fn should_load_older_history(cm: &ChannelMessages, y: f32) -> bool {
    y <= LOAD_OLDER_SCROLL_TOP_PX && cm.loaded && cm.has_more_older && !cm.history_loading_older
}

pub(super) fn scroll_to_pending(app: &mut App, channel: &ChannelId) -> Task<Message> {
    let Some((pending_channel, target)) = app.pending_scroll_to.clone() else {
        return Task::none();
    };
    if pending_channel != *channel {
        return Task::none();
    }
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let loaded_messages = app
        .workspaces
        .get(&team)
        .and_then(|ws| ws.messages.get(channel))
        .filter(|cm| cm.loaded)
        .map(|cm| &cm.messages);
    let Some(messages) = loaded_messages else {
        return Task::none();
    };
    let Some(ts) = pending_target_ts(messages, target) else {
        return Task::none();
    };
    match crate::state::scroll_ratio_for_ts(messages, &ts) {
        Some(ratio) => {
            app.pending_scroll_to = None;
            if ratio >= 1.0 {
                app.chat_paused.remove(channel);
                operation::scroll_to(
                    ui::channel::scrollable_id(channel),
                    AbsoluteOffset { x: 0.0, y: 0.0 },
                )
            } else {
                app.chat_paused.entry(channel.clone()).or_insert(0);
                operation::snap_to(
                    ui::channel::scrollable_id(channel),
                    RelativeOffset { x: 0.0, y: ratio },
                )
            }
        }
        None => Task::none(),
    }
}

pub(super) fn scroll_thread_to_unread(
    app: &App,
    team: &str,
    channel: &ChannelId,
    root_ts: &MessageTs,
    unread_anchor: Option<&str>,
) -> Task<Message> {
    let Some(unread_anchor) = unread_anchor else {
        return Task::none();
    };
    let messages = app
        .threads
        .get(&(team.to_owned(), channel.clone(), root_ts.clone()))
        .map(|thread| thread.messages.as_slice())
        .unwrap_or_default();
    let Some(ratio) = crate::state::scroll_ratio_for_ts(messages, unread_anchor) else {
        return Task::none();
    };
    operation::snap_to(
        ui::thread::scrollable_id(channel, root_ts),
        RelativeOffset { x: 0.0, y: ratio },
    )
}

pub(in crate::app) fn channel_open_scroll_target(
    app: &App,
    team: &str,
    channel: &ChannelId,
) -> Option<PendingScrollTarget> {
    let ws = app.workspaces.get(team)?;
    let cm = ws.messages.get(channel);
    let unread = ws
        .channels
        .get(channel)
        .map(|channel| ws.unread_total(channel) > 0)
        .unwrap_or_else(|| cm.is_some_and(|cm| cm.unread_count > 0 || cm.mention_count > 0));
    if unread {
        if let Some(last_read) = cm.and_then(|cm| cm.last_read.clone()) {
            return Some(PendingScrollTarget::FirstUnreadAfter(last_read));
        }
    }
    Some(PendingScrollTarget::Latest)
}

pub(in crate::app) fn pending_target_ts(
    messages: &[SlackMessage],
    target: PendingScrollTarget,
) -> Option<MessageTs> {
    match target {
        PendingScrollTarget::Message(ts) => Some(ts),
        PendingScrollTarget::FirstUnreadAfter(last_read) => messages
            .iter()
            .filter_map(|message| message.ts.as_deref())
            .find(|ts| crate::state::ts_key(ts) > crate::state::ts_key(&last_read))
            .map(str::to_owned)
            .or_else(|| {
                messages
                    .iter()
                    .filter_map(|message| message.ts.clone())
                    .last()
            }),
        PendingScrollTarget::Latest => messages
            .iter()
            .filter_map(|message| message.ts.clone())
            .last(),
    }
}

pub(super) fn open_url_pressed(app: &mut App, url: String) -> Task<Message> {
    if !crate::state::is_browser_url(&url) {
        app.toast("could not open link: unsupported URL scheme");
        return Task::none();
    }
    Task::perform(open_url_in_browser(url), Message::UrlOpened)
}

pub(super) async fn open_url_in_browser(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = tokio::process::Command::new("open");
        cmd.arg(&url);
        cmd
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut cmd = tokio::process::Command::new("xdg-open");
        cmd.arg(&url);
        cmd
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.args(["/C", "start", "", &url]);
        cmd
    };

    let status = command
        .status()
        .await
        .map_err(|e| format!("spawn failed: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("exited with {status}"))
    }
}

pub(super) fn image_viewer_opened(app: &mut App, source: ImageViewerSource) -> Task<Message> {
    let cleanup = app
        .image_viewer
        .as_mut()
        .and_then(|viewer| viewer.video.as_mut())
        .and_then(|video| video.path.take())
        .map(remove_viewer_video);
    app.image_viewer_generation = app.image_viewer_generation.wrapping_add(1);
    let generation = app.image_viewer_generation;
    let image = if source.full_url == source.preview_key {
        app.file_previews
            .get(&source.preview_key)
            .and_then(file_preview_handle)
            .map(ImageViewerImage::Loaded)
            .unwrap_or(ImageViewerImage::Loading)
    } else {
        ImageViewerImage::Loading
    };
    let already_loaded = matches!(image, ImageViewerImage::Loaded(_));
    app.profile_hover = None;
    app.image_viewer = Some(ImageViewerState {
        source: source.clone(),
        image,
        generation,
        open: true,
        zoom: 1.0,
        offset: iced::Vector::ZERO,
        video: (source.kind == MediaViewerKind::Video).then(VideoViewerPlayback::default),
    });
    if already_loaded {
        return cleanup.unwrap_or_else(Task::none);
    }
    let Some(transport) = app.transport.clone() else {
        if let Some(viewer) = app.image_viewer.as_mut() {
            viewer.image = ImageViewerImage::Failed;
        }
        return cleanup.unwrap_or_else(Task::none);
    };
    let user_agent = crate::slack::xparams::Identity::from_capture().user_agent;
    let load = match source.kind {
        MediaViewerKind::Image => Task::perform(
            fetch_image_bytes(transport, source.full_url, source.fetch_auth, user_agent),
            move |result| {
                Message::Discovery(crate::app::DiscoveryMessage::ImageViewerFullLoaded {
                    generation,
                    result,
                })
            },
        ),
        MediaViewerKind::Video => Task::perform(
            prepare_viewer_video(
                transport,
                source.full_url,
                source.fetch_auth,
                user_agent,
                source.filename,
            ),
            move |result| {
                Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoPrepared {
                    generation,
                    result,
                })
            },
        ),
    };
    match cleanup {
        Some(cleanup) => Task::batch([cleanup, load]),
        None => load,
    }
}

pub(super) fn remove_viewer_video(path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            let _ = tokio::fs::remove_file(path).await;
        },
        |_| Message::Runtime(crate::app::RuntimeMessage::AnimationTick),
    )
}

pub(super) async fn prepare_viewer_video(
    transport: Arc<Transport>,
    url: String,
    auth: ImageFetchAuth,
    user_agent: String,
    filename: String,
) -> Result<PreparedVideo, SlackError> {
    let bytes = fetch_image_bytes(transport, url, auth, user_agent).await?;
    let extension = Path::new(&filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("mp4");
    let path = std::env::temp_dir().join(format!(
        "snack-viewer-{}.{}",
        uuid::Uuid::new_v4(),
        extension
    ));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| SlackError::Transport(format!("write video preview: {error}")))?;
    let player_path = path.clone();
    let player = tokio::task::spawn_blocking(move || {
        let uri = url::Url::from_file_path(&player_path)
            .map_err(|_| "could not create the local video URL".to_owned())?;
        iced_video_player::Video::new(&uri).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| SlackError::Transport(format!("start video player: {error}")))?;
    match player {
        Ok(player) => Ok(PreparedVideo {
            path,
            player: Arc::new(player),
        }),
        Err(error) => {
            let _ = tokio::fs::remove_file(&path).await;
            Err(SlackError::Transport(format!("open video: {error}")))
        }
    }
}

pub(super) fn file_preview_handle(preview: &FilePreview) -> Option<ImageHandle> {
    match preview {
        FilePreview::Loaded(handle) => Some(handle.clone()),
        FilePreview::Animated { frames, .. } => frames.first().cloned(),
        FilePreview::Loading | FilePreview::Failed => None,
    }
}

pub(super) async fn fetch_image_bytes(
    transport: Arc<Transport>,
    url: String,
    auth: ImageFetchAuth,
    user_agent: String,
) -> Result<Vec<u8>, SlackError> {
    match auth {
        ImageFetchAuth::Slack => transport.get_bytes(&url, &user_agent).await,
        ImageFetchAuth::Public if crate::state::is_gif_url(&url) => {
            transport.get_public_gif_bytes(&url, &user_agent).await
        }
        ImageFetchAuth::Public => transport.get_public_bytes(&url, &user_agent).await,
    }
}

pub(super) fn download_file_pressed(
    app: &mut App,
    url: String,
    filename: String,
    auth: ImageFetchAuth,
) -> Task<Message> {
    let Some(transport) = app.transport.clone() else {
        app.toast("download failed: transport not connected");
        return Task::none();
    };
    Task::perform(
        async move { download_file_to_disk(transport, url, filename, auth).await },
        Message::FileDownloaded,
    )
}

pub(super) async fn download_file_to_disk(
    transport: Arc<Transport>,
    url: String,
    filename: String,
    auth: ImageFetchAuth,
) -> Result<PathBuf, SlackError> {
    let user_agent = crate::slack::xparams::Identity::from_capture().user_agent;
    let bytes = fetch_image_bytes(transport, url, auth, user_agent).await?;
    let dir = config::data_dir()
        .map_err(|e| SlackError::Transport(format!("download dir: {e}")))?
        .join("downloads");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| SlackError::Transport(format!("create download dir: {e}")))?;
    let path = unique_download_path(&dir, &filename).await?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| SlackError::Transport(format!("write download: {e}")))?;
    Ok(path)
}

pub(in crate::app) async fn unique_download_path(
    dir: &Path,
    filename: &str,
) -> Result<PathBuf, SlackError> {
    let filename = if filename.trim().is_empty() {
        "download"
    } else {
        filename
    };
    let candidate = dir.join(filename);
    if !tokio::fs::try_exists(&candidate)
        .await
        .map_err(|e| SlackError::Transport(format!("check download path: {e}")))?
    {
        return Ok(candidate);
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = path.extension().and_then(|s| s.to_str());
    for i in 1..1000 {
        let name = match ext {
            Some(ext) if !ext.is_empty() => format!("{stem}-{i}.{ext}"),
            _ => format!("{stem}-{i}"),
        };
        let candidate = dir.join(name);
        if !tokio::fs::try_exists(&candidate)
            .await
            .map_err(|e| SlackError::Transport(format!("check download path: {e}")))?
        {
            return Ok(candidate);
        }
    }

    Err(SlackError::Transport(
        "could not choose download path".to_owned(),
    ))
}

pub(super) fn load_visible_file_previews(
    app: &mut App,
    team: &str,
    channel: &str,
) -> Task<Message> {
    let messages = visible_channel_messages(app, team, channel);
    load_file_previews(app, messages)
}

pub(super) fn load_visible_avatar_previews(
    app: &mut App,
    team: &str,
    channel: &str,
) -> Task<Message> {
    let messages = visible_channel_messages(app, team, channel);
    load_avatar_previews(app, team, messages)
}

pub(super) fn load_thread_file_previews(
    app: &mut App,
    team: &str,
    channel: &str,
    root_ts: &str,
) -> Task<Message> {
    let mut messages = Vec::new();
    if let Some(root) = app
        .workspaces
        .get(team)
        .and_then(|ws| ws.messages.get(channel))
        .and_then(|cm| {
            cm.messages
                .iter()
                .find(|msg| msg.ts.as_deref() == Some(root_ts))
        })
    {
        messages.push(root.clone());
    }
    if let Some(replies) =
        app.threads
            .get(&(team.to_owned(), channel.to_owned(), root_ts.to_owned()))
    {
        messages.extend(replies.messages.clone());
    }
    load_file_previews(app, messages)
}

pub(super) fn load_thread_avatar_previews(
    app: &mut App,
    team: &str,
    channel: &str,
    root_ts: &str,
) -> Task<Message> {
    let mut messages = Vec::new();
    if let Some(root) = app
        .workspaces
        .get(team)
        .and_then(|ws| ws.messages.get(channel))
        .and_then(|cm| {
            cm.messages
                .iter()
                .find(|msg| msg.ts.as_deref() == Some(root_ts))
        })
    {
        messages.push(root.clone());
    }
    if let Some(replies) =
        app.threads
            .get(&(team.to_owned(), channel.to_owned(), root_ts.to_owned()))
    {
        messages.extend(replies.messages.clone());
    }
    load_avatar_previews(app, team, messages)
}

pub(super) fn load_file_previews(app: &mut App, messages: Vec<SlackMessage>) -> Task<Message> {
    let Some(transport) = app.transport.clone() else {
        return Task::none();
    };
    let file_requests = messages
        .iter()
        .flat_map(|msg| &msg.files)
        .filter_map(|file| {
            let key = crate::state::file_preview_key(file)?;
            let url = crate::state::file_preview_url(file)?.to_owned();
            let auth = if crate::state::is_slack_authenticated_url(&url) {
                ImageFetchAuth::Slack
            } else {
                ImageFetchAuth::Public
            };
            Some((key, url, auth))
        });
    let attachment_requests = messages
        .iter()
        .flat_map(|msg| &msg.attachments)
        .flat_map(crate::state::attachment_images)
        .map(|image| {
            let url = image.preview_url.to_owned();
            (url.clone(), url, ImageFetchAuth::Public)
        });
    let mut seen = std::collections::HashSet::new();
    let requests: Vec<_> = file_requests
        .chain(attachment_requests)
        .filter(|(key, _, _)| !app.file_previews.contains_key(key) && seen.insert(key.clone()))
        .collect();

    if requests.is_empty() {
        return Task::none();
    }

    for (key, _, _) in &requests {
        app.file_previews.insert(key.clone(), FilePreview::Loading);
    }

    let user_agent = crate::slack::xparams::Identity::from_capture().user_agent;
    Task::batch(requests.into_iter().map(|(key, url, auth)| {
        let transport = transport.clone();
        let user_agent = user_agent.clone();
        Task::perform(
            fetch_image_preview(transport, url, auth, user_agent),
            move |result| {
                Message::Discovery(crate::app::DiscoveryMessage::FilePreviewLoaded {
                    key: key.clone(),
                    result,
                })
            },
        )
    }))
}

pub(super) fn hydrate_visible_emojis(app: &App, team: &str, channel: &str) -> Task<Message> {
    let messages = visible_channel_messages(app, team, channel);
    hydrate_emojis(app, team, &messages)
}

pub(super) fn hydrate_thread_emojis(
    app: &App,
    team: &str,
    channel: &str,
    root_ts: &str,
) -> Task<Message> {
    let messages = thread_messages(app, team, channel, root_ts);
    hydrate_emojis(app, team, &messages)
}

pub(super) fn hydrate_emojis(app: &App, team: &str, messages: &[SlackMessage]) -> Task<Message> {
    let names = messages.iter().flat_map(message_emoji_names).collect();
    hydrate_emoji_names(app, team, names)
}

pub(super) fn hydrate_emoji_names(app: &App, team: &str, names: Vec<String>) -> Task<Message> {
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(team) else {
        return Task::none();
    };
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };

    let mut seen = HashSet::new();
    let names: Vec<_> = names
        .into_iter()
        .filter(|name| !crate::state::is_standard_emoji(name))
        .filter(|name| !ws.custom_emoji.contains_key(name))
        .filter(|name| {
            !app.emoji_hydrated
                .contains(&(team.to_owned(), name.clone()))
        })
        .filter(|name| seen.insert(name.clone()))
        .collect();

    if names.is_empty() {
        return Task::none();
    }

    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let team = team.to_owned();
    Task::perform(
        {
            let names = names.clone();
            async move { api::fetch_emojis_info(&transport, &client, &ws_session, names).await }
        },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::EmojisLoaded {
                team: team.clone(),
                requested: names.clone(),
                result,
            })
        },
    )
}

pub(super) fn load_visible_emoji_previews(
    app: &mut App,
    team: &str,
    channel: &str,
) -> Task<Message> {
    let messages = visible_channel_messages(app, team, channel);
    load_emoji_previews(app, team, &messages)
}

pub(super) fn load_thread_emoji_previews(
    app: &mut App,
    team: &str,
    channel: &str,
    root_ts: &str,
) -> Task<Message> {
    let messages = thread_messages(app, team, channel, root_ts);
    load_emoji_previews(app, team, &messages)
}

pub(super) fn load_emoji_previews(
    app: &mut App,
    team: &str,
    messages: &[SlackMessage],
) -> Task<Message> {
    let mut seen = HashSet::new();
    let names = messages
        .iter()
        .flat_map(message_emoji_names)
        .filter(|name| seen.insert(name.clone()))
        .collect();
    load_emoji_previews_for_names(app, team, names)
}

pub(super) fn load_emoji_previews_for_names(
    app: &mut App,
    team: &str,
    names: Vec<String>,
) -> Task<Message> {
    let Some(transport) = app.transport.clone() else {
        return Task::none();
    };
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };

    let mut seen = HashSet::new();
    let requests: Vec<_> = names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .filter_map(|name| {
            let key = crate::state::emoji_preview_key(team, &name);
            if app.emoji_previews.contains_key(&key) {
                return None;
            }
            let url = ws.custom_emoji_url(&name)?.to_owned();
            Some((key, url))
        })
        .collect();

    if requests.is_empty() {
        return Task::none();
    }

    for (key, _) in &requests {
        app.emoji_previews.insert(key.clone(), FilePreview::Loading);
    }

    let user_agent = crate::slack::xparams::Identity::from_capture().user_agent;
    Task::batch(requests.into_iter().map(|(key, url)| {
        let transport = transport.clone();
        let user_agent = user_agent.clone();
        Task::perform(
            async move {
                let bytes = transport.get_bytes(&url, &user_agent).await?;
                decode_preview_bytes(bytes).await
            },
            move |result| {
                Message::Discovery(crate::app::DiscoveryMessage::EmojiPreviewLoaded {
                    key: key.clone(),
                    result,
                })
            },
        )
    }))
}

pub(in crate::app) fn emoji_preview_from_bytes(bytes: Vec<u8>) -> FilePreview {
    preview_from_bytes(bytes)
}

pub(in crate::app) fn allocate_animated_preview(
    preview: FilePreview,
) -> Task<Result<FilePreview, SlackError>> {
    let FilePreview::Animated {
        frames,
        allocations,
        delays,
        total,
    } = preview
    else {
        return Task::done(Ok(preview));
    };
    if !allocations.is_empty() {
        return Task::done(Ok(FilePreview::Animated {
            frames,
            allocations,
            delays,
            total,
        }));
    }

    Task::batch(frames.iter().cloned().map(iced::widget::image::allocate))
        .collect()
        .map(move |results| {
            let allocations =
                results
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| {
                        SlackError::Transport(format!("allocate animated image frames: {error}"))
                    })?;
            Ok(FilePreview::Animated {
                frames: frames.clone(),
                allocations,
                delays: delays.clone(),
                total,
            })
        })
}

const MAX_GIF_BYTES: usize = 10 * 1024 * 1024;
const MAX_GIF_PIXELS: usize = 2_000_000;
const MAX_GIF_FRAMES: usize = 300;
const MAX_GIF_DECODED_BYTES: usize = 64 * 1024 * 1024;

async fn fetch_image_preview(
    transport: Arc<Transport>,
    url: String,
    auth: ImageFetchAuth,
    user_agent: String,
) -> Result<FilePreview, SlackError> {
    let bytes = fetch_image_bytes(transport, url, auth, user_agent).await?;
    decode_preview_bytes(bytes).await
}

async fn decode_preview_bytes(bytes: Vec<u8>) -> Result<FilePreview, SlackError> {
    tokio::task::spawn_blocking(move || preview_from_bytes(bytes))
        .await
        .map_err(|error| SlackError::Transport(format!("decode image preview: {error}")))
}

fn preview_from_bytes(bytes: Vec<u8>) -> FilePreview {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        if bytes.len() > MAX_GIF_BYTES {
            return FilePreview::Failed;
        }
        if let Some(preview) = decode_gif_preview(&bytes) {
            return preview;
        }
        return FilePreview::Failed;
    }
    FilePreview::Loaded(ImageHandle::from_bytes(bytes))
}

pub(super) fn decode_gif_preview(bytes: &[u8]) -> Option<FilePreview> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(std::io::Cursor::new(bytes)).ok()?;

    let width = usize::from(decoder.width());
    let height = usize::from(decoder.height());
    let pixels = width.checked_mul(height)?;
    let frame_bytes = pixels.checked_mul(4)?;
    if width == 0 || height == 0 || pixels > MAX_GIF_PIXELS {
        return None;
    }

    let mut frames = Vec::new();
    let mut delays = Vec::new();
    let mut canvas = vec![0u8; frame_bytes];
    while let Some(frame) = decoder.read_next_frame().ok()? {
        if frames.len() >= MAX_GIF_FRAMES
            || frames.len().checked_add(1)?.checked_mul(frame_bytes)? > MAX_GIF_DECODED_BYTES
        {
            return frames.into_iter().next().map(FilePreview::Loaded);
        }
        let snapshot =
            matches!(frame.dispose, gif::DisposalMethod::Previous).then(|| canvas.clone());

        composite_frame(&mut canvas, width, height, frame);
        frames.push(png_frame_handle(width as u32, height as u32, &canvas)?);
        delays.push(gif_delay(frame.delay));

        match frame.dispose {
            gif::DisposalMethod::Background => {
                clear_frame_rect(&mut canvas, width, height, frame);
            }
            gif::DisposalMethod::Previous => {
                if let Some(prev) = snapshot {
                    canvas = prev;
                }
            }
            gif::DisposalMethod::Keep | gif::DisposalMethod::Any => {}
        }
    }

    match frames.len() {
        0 => None,
        1 => Some(FilePreview::Loaded(frames.remove(0))),
        _ => {
            let total = delays.iter().copied().sum();
            Some(FilePreview::Animated {
                frames,
                allocations: Vec::new(),
                delays,
                total,
            })
        }
    }
}

fn png_frame_handle(width: u32, height: u32, pixels: &[u8]) -> Option<ImageHandle> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(pixels).ok()?;
    }
    Some(ImageHandle::from_bytes(bytes))
}

pub(super) fn gif_delay(delay_cs: u16) -> Duration {
    if delay_cs == 0 {
        Duration::from_millis(100)
    } else {
        Duration::from_millis((delay_cs as u64 * 10).max(20))
    }
}

pub(super) fn composite_frame(canvas: &mut [u8], width: usize, height: usize, frame: &gif::Frame) {
    let fx = frame.left as usize;
    let fy = frame.top as usize;
    let fw = frame.width as usize;
    let fh = frame.height as usize;
    for row in 0..fh {
        let cy = fy + row;
        if cy >= height {
            break;
        }
        for col in 0..fw {
            let cx = fx + col;
            if cx >= width {
                break;
            }
            let src = (row * fw + col) * 4;
            if frame.buffer[src + 3] == 0 {
                continue;
            }
            let dst = (cy * width + cx) * 4;
            canvas[dst..dst + 4].copy_from_slice(&frame.buffer[src..src + 4]);
        }
    }
}

pub(super) fn clear_frame_rect(canvas: &mut [u8], width: usize, height: usize, frame: &gif::Frame) {
    let fx = frame.left as usize;
    let fy = frame.top as usize;
    let fw = frame.width as usize;
    let fh = frame.height as usize;
    for row in 0..fh {
        let cy = fy + row;
        if cy >= height {
            break;
        }
        let start = (cy * width + fx.min(width)) * 4;
        let end = (cy * width + (fx + fw).min(width)) * 4;
        canvas[start..end].fill(0);
    }
}

pub(super) fn thread_messages(
    app: &App,
    team: &str,
    channel: &str,
    root_ts: &str,
) -> Vec<SlackMessage> {
    let mut messages = Vec::new();
    if let Some(root) = app
        .workspaces
        .get(team)
        .and_then(|ws| ws.messages.get(channel))
        .and_then(|cm| {
            cm.messages
                .iter()
                .find(|msg| msg.ts.as_deref() == Some(root_ts))
        })
    {
        messages.push(root.clone());
    }
    if let Some(replies) =
        app.threads
            .get(&(team.to_owned(), channel.to_owned(), root_ts.to_owned()))
    {
        messages.extend(replies.messages.clone());
    }
    messages
}

pub(super) fn message_emoji_names(msg: &SlackMessage) -> Vec<String> {
    let mut names = Vec::new();
    visit_message_emoji_names(msg, |name| names.push(name.to_owned()));
    names
}

pub(in crate::app) fn visit_message_emoji_names<'a>(
    msg: &'a SlackMessage,
    mut visit: impl FnMut(&'a str),
) {
    if let Some(text) = msg.text.as_deref() {
        crate::state::visit_emoji_names_in_text(text, &mut visit);
    }
    for reaction in &msg.reactions {
        visit(&reaction.name);
    }
    for block in &msg.blocks {
        visit_value_emoji_names(block, &mut visit);
    }
    for att in &msg.attachments {
        for text in [
            att.service_name.as_deref(),
            att.author_name.as_deref(),
            att.title.as_deref(),
            att.pretext.as_deref(),
            att.text.as_deref(),
            att.footer.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            crate::state::visit_emoji_names_in_text(text, &mut visit);
        }
    }
}

pub(super) fn collect_value_emoji_names(value: &serde_json::Value, names: &mut Vec<String>) {
    visit_value_emoji_names(value, &mut |name| names.push(name.to_owned()));
}

fn visit_value_emoji_names<'a>(value: &'a serde_json::Value, visit: &mut impl FnMut(&'a str)) {
    match value {
        serde_json::Value::String(text) => {
            crate::state::visit_emoji_names_in_text(text, &mut *visit);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                visit_value_emoji_names(value, visit);
            }
        }
        serde_json::Value::Object(map) => {
            if map
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind == "emoji")
            {
                if let Some(name) = map.get("name").and_then(serde_json::Value::as_str) {
                    visit(name);
                }
            }
            for value in map.values() {
                visit_value_emoji_names(value, visit);
            }
        }
        _ => {}
    }
}
