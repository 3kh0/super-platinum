use super::super::*;

pub(super) fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Discovery(crate::app::DiscoveryMessage::SearchInputChanged(value)) => {
            app.search_input = value;
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::SearchSubmitted) => search_submitted(app),

        Message::Discovery(crate::app::DiscoveryMessage::SearchCleared) => {
            app.search = None;
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::SearchPageRequested(page)) => {
            search_page_requested(app, page)
        }

        Message::Discovery(crate::app::DiscoveryMessage::SearchLoaded {
            team,
            query,
            page,
            result,
        }) => {
            let matches = app
                .search
                .as_ref()
                .is_some_and(|s| s.team == team && s.query == query && s.page == page);
            if !matches {
                return Task::none();
            }
            match result {
                Ok(response) => {
                    if let Some(ws) = app.workspaces.get(&team) {
                        if let Some(state) = app.search.as_mut() {
                            state.hits = search_hits(ws, &response);
                            if let Some(p) = &response.pagination {
                                state.page = p.page.unwrap_or(page);
                                state.page_count = p.page_count.unwrap_or(state.page_count);
                                state.total = p.total_count.unwrap_or(state.total);
                            }
                            state.loading = false;
                        }
                    }
                }
                Err(e) => {
                    if let Some(state) = app.search.as_mut() {
                        state.loading = false;
                    }
                    app.toast(format!("search failed: {e}"));
                    if is_auth_error(&e) {
                        app.screen = Screen::Login;
                    }
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::SearchResultSelected {
            channel,
            ts,
            thread_ts,
        }) => open_search_result(app, channel, ts, thread_ts),

        Message::Discovery(crate::app::DiscoveryMessage::PaletteToggled) => palette_toggled(app),
        Message::Discovery(crate::app::DiscoveryMessage::PaletteClosed) => {
            app.palette_open = false;
            focus_active_composer(app)
        }
        Message::Discovery(crate::app::DiscoveryMessage::PaletteDismissed) => {
            if !app.palette_open {
                app.palette = None;
            }
            Task::none()
        }
        Message::Discovery(crate::app::DiscoveryMessage::PaletteQueryChanged(query)) => {
            palette_query_changed(app, query)
        }
        Message::Discovery(crate::app::DiscoveryMessage::PaletteMoved(delta)) => {
            palette_moved(app, delta);
            Task::none()
        }
        Message::Discovery(crate::app::DiscoveryMessage::PaletteSubmitted) => {
            palette_activate_selected(app)
        }
        Message::Discovery(crate::app::DiscoveryMessage::PaletteEntryPressed(index)) => {
            palette_activate(app, index)
        }
        Message::Discovery(crate::app::DiscoveryMessage::PaletteRemoteUsersLoaded {
            team,
            seq,
            result,
        }) => palette_remote_users_loaded(app, team, seq, result),
        Message::Discovery(crate::app::DiscoveryMessage::PaletteRemoteChannelsLoaded {
            team,
            seq,
            result,
        }) => palette_remote_channels_loaded(app, team, seq, result),
        Message::Discovery(crate::app::DiscoveryMessage::DmOpened { team, user, result }) => {
            dm_opened(app, team, user, result)
        }

        Message::Discovery(crate::app::DiscoveryMessage::FileDownloadPressed {
            url,
            filename,
            auth,
        }) => download_file_pressed(app, url, filename, auth),

        Message::Discovery(crate::app::DiscoveryMessage::FileDownloaded(result)) => {
            match result {
                Ok(path) => app.toast(format!("downloaded file to {}", path.display())),
                Err(e) => app.toast(format!("download failed: {e}")),
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerOpened(source)) => {
            image_viewer_opened(app, source)
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerFullLoaded {
            generation,
            result,
        }) => {
            let Some(viewer) = app
                .image_viewer
                .as_mut()
                .filter(|viewer| viewer.open && viewer.generation == generation)
            else {
                return Task::none();
            };
            match result {
                Ok(bytes) => {
                    viewer.image = ImageViewerImage::Loaded(ImageHandle::from_bytes(bytes));
                }
                Err(error) => {
                    tracing::warn!(error = %error, "full image failed");
                    viewer.image = ImageViewerImage::Failed;
                    app.toast("Original image unavailable");
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoPrepared {
            generation,
            result,
        }) => {
            let Some(viewer) = app
                .image_viewer
                .as_mut()
                .filter(|viewer| viewer.open && viewer.generation == generation)
            else {
                return result
                    .ok()
                    .map(|prepared| remove_viewer_video(prepared.path))
                    .unwrap_or_else(Task::none);
            };
            match result {
                Ok(prepared) => {
                    let video = viewer
                        .video
                        .get_or_insert_with(VideoViewerPlayback::default);
                    video.path = Some(prepared.path);
                    video.duration = prepared.player.duration().as_secs_f32();
                    video.position = 0.0;
                    video.playing = true;
                    video.player = Some(prepared.player);
                }
                Err(error) => {
                    tracing::warn!(error = %error, "video preparation failed");
                    viewer.image = ImageViewerImage::Failed;
                    app.toast("Video unavailable");
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoFrame(generation)) => {
            let Some(video) = app.image_viewer.as_mut().and_then(|viewer| {
                (viewer.open && viewer.generation == generation)
                    .then_some(viewer)
                    .and_then(|viewer| viewer.video.as_mut())
            }) else {
                return Task::none();
            };
            if !video.seeking
                && let Some(player) = video.player.as_ref()
            {
                video.position = player.position().as_secs_f32().min(video.duration);
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoEnded(generation)) => {
            if let Some(video) = app.image_viewer.as_mut().and_then(|viewer| {
                (viewer.open && viewer.generation == generation)
                    .then_some(viewer)
                    .and_then(|viewer| viewer.video.as_mut())
            }) {
                video.position = video.duration;
                video.playing = false;
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoFailed {
            generation,
            error,
        }) => {
            let Some(viewer) = app
                .image_viewer
                .as_mut()
                .filter(|viewer| viewer.open && viewer.generation == generation)
            else {
                return Task::none();
            };
            tracing::warn!(%error, "video playback failed");
            if let Some(video) = viewer.video.as_mut() {
                video.playing = false;
                video.player = None;
            }
            viewer.image = ImageViewerImage::Failed;
            app.toast("Video playback failed");
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoPlayPause) => {
            if let Some(video) = app
                .image_viewer
                .as_mut()
                .and_then(|viewer| viewer.video.as_mut())
                .filter(|video| video.player.is_some())
            {
                let should_play = !video.playing;
                if let Some(player) = video.player.as_mut().and_then(Arc::get_mut) {
                    if should_play && video.position >= video.duration {
                        if let Err(error) = player.restart_stream() {
                            tracing::warn!(%error, "video restart failed");
                            return Task::none();
                        }
                        video.position = 0.0;
                    } else {
                        player.set_paused(!should_play);
                    }
                    video.playing = should_play;
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoSeekChanged(position)) => {
            if let Some(video) = app
                .image_viewer
                .as_mut()
                .and_then(|viewer| viewer.video.as_mut())
            {
                if !video.seeking {
                    video.resume_after_seek = video.playing;
                    video.playing = false;
                    video.seeking = true;
                    if let Some(player) = video.player.as_mut().and_then(Arc::get_mut) {
                        player.set_paused(true);
                    }
                }
                video.position = position.clamp(0.0, video.duration);
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoSeekReleased) => {
            if let Some(video) = app
                .image_viewer
                .as_mut()
                .and_then(|viewer| viewer.video.as_mut())
                .filter(|video| video.seeking)
            {
                if let Some(player) = video.player.as_mut().and_then(Arc::get_mut) {
                    if let Err(error) = player.seek(Duration::from_secs_f32(video.position), true) {
                        tracing::warn!(%error, "video seek failed");
                    }
                    player.set_paused(!video.resume_after_seek);
                }
                video.seeking = false;
                video.playing = video.resume_after_seek;
                video.resume_after_seek = false;
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoVolumeChanged(volume)) => {
            if let Some(video) = app
                .image_viewer
                .as_mut()
                .and_then(|viewer| viewer.video.as_mut())
            {
                video.volume = volume.clamp(0.0, 1.0);
                video.muted = false;
                if let Some(player) = video.player.as_mut().and_then(Arc::get_mut) {
                    player.set_volume(video.volume as f64);
                    player.set_muted(false);
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoVolumeReleased) => {
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerVideoMuteToggled) => {
            if let Some(video) = app
                .image_viewer
                .as_mut()
                .and_then(|viewer| viewer.video.as_mut())
            {
                video.muted = !video.muted;
                if let Some(player) = video.player.as_mut().and_then(Arc::get_mut) {
                    player.set_muted(video.muted);
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerClosed) => {
            if let Some(viewer) = app.image_viewer.as_mut() {
                viewer.open = false;
                if let Some(video) = viewer.video.as_mut() {
                    video.player = None;
                }
                if let Some(path) = viewer.video.as_mut().and_then(|video| video.path.take()) {
                    return remove_viewer_video(path);
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerDismissed) => {
            if app.image_viewer.as_ref().is_some_and(|viewer| !viewer.open) {
                app.image_viewer = None;
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerZoomChanged(zoom)) => {
            if let Some(viewer) = app.image_viewer.as_mut() {
                let previous = viewer.zoom;
                viewer.zoom = zoom.clamp(1.0, 5.0);
                if viewer.zoom <= 1.0 {
                    viewer.offset = iced::Vector::ZERO;
                } else if previous > 0.0 {
                    viewer.offset = viewer.offset * (viewer.zoom / previous);
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerTransformed {
            zoom,
            offset,
        }) => {
            if let Some(viewer) = app.image_viewer.as_mut() {
                viewer.zoom = zoom.clamp(1.0, 5.0);
                viewer.offset = if viewer.zoom <= 1.0 {
                    iced::Vector::ZERO
                } else {
                    offset
                };
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ImageViewerDownloadPressed) => {
            let Some(viewer) = app.image_viewer.as_ref() else {
                return Task::none();
            };
            download_file_pressed(
                app,
                viewer.source.download_url.clone(),
                viewer.source.filename.clone(),
                viewer.source.fetch_auth,
            )
        }

        Message::Discovery(crate::app::DiscoveryMessage::OpenUrl(url)) => {
            open_url_pressed(app, url)
        }

        Message::Discovery(crate::app::DiscoveryMessage::UrlOpened(result)) => {
            if let Err(e) = result {
                app.toast(format!("could not open link: {e}"));
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::FilePreviewLoaded { key, result }) => {
            match result {
                Ok(preview) => {
                    if matches!(
                        &preview,
                        FilePreview::Animated { allocations, .. } if allocations.is_empty()
                    ) {
                        let result_key = key.clone();
                        return allocate_animated_preview(preview).map(move |result| {
                            Message::Discovery(crate::app::DiscoveryMessage::FilePreviewLoaded {
                                key: result_key.clone(),
                                result,
                            })
                        });
                    }
                    app.file_previews.insert(key, preview);
                }
                Err(e) => {
                    tracing::warn!(%key, error = %e, "file preview failed");
                    app.file_previews.insert(key, FilePreview::Failed);
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::AvatarLoaded { user, result }) => {
            match result {
                Ok(bytes) => {
                    app.avatar_previews
                        .insert(user, FilePreview::Loaded(ImageHandle::from_bytes(bytes)));
                }
                Err(e) => {
                    tracing::debug!(%user, error = %e, "avatar failed");
                    app.avatar_previews.insert(user, FilePreview::Failed);
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::EmojiPreviewLoaded { key, result }) => {
            match result {
                Ok(preview) => {
                    if matches!(
                        &preview,
                        FilePreview::Animated { allocations, .. } if allocations.is_empty()
                    ) {
                        let result_key = key.clone();
                        return allocate_animated_preview(preview).map(move |result| {
                            Message::Discovery(crate::app::DiscoveryMessage::EmojiPreviewLoaded {
                                key: result_key.clone(),
                                result,
                            })
                        });
                    }
                    app.emoji_previews.insert(key, preview);
                }
                Err(e) => {
                    tracing::debug!(%key, error = %e, "emoji preview failed");
                    app.emoji_previews.insert(key, FilePreview::Failed);
                }
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::UsersLoaded { team, result }) => {
            match result {
                Ok(users) => {
                    let ids: Vec<_> = users.iter().map(|user| user.id.clone()).collect();
                    app.avatar_profile_hydrated.extend(ids.iter().cloned());
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        for user in users {
                            ws.users.insert(user.id.clone(), user);
                        }
                    }
                    mark_workspace_dirty(app, &team);
                    return load_user_avatar_previews(app, &team, ids);
                }
                Err(e) => tracing::debug!(%team, error = %e, "users info failed"),
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::EmojisLoaded {
            team,
            requested,
            result,
        }) => {
            app.emoji_hydrated
                .extend(requested.iter().map(|name| (team.clone(), name.clone())));
            match result {
                Ok(emojis) => {
                    let names: Vec<_> = emojis.iter().map(|emoji| emoji.name.clone()).collect();
                    let alias_targets: Vec<_> = emojis
                        .iter()
                        .filter_map(|emoji| emoji.value.strip_prefix("alias:"))
                        .filter(|name| !crate::state::is_standard_emoji(name))
                        .map(str::to_owned)
                        .collect();
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        ws.apply_emojis(emojis);
                    }
                    return Task::batch([
                        load_emoji_previews_for_names(app, &team, names),
                        hydrate_emoji_names(app, &team, alias_targets),
                    ]);
                }
                Err(e) => tracing::debug!(%team, error = %e, "emojis info failed"),
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::ChannelsLoaded {
            team,
            requested,
            result,
        }) => {
            app.channel_hydrated
                .extend(requested.into_iter().map(|channel| (team.clone(), channel)));
            match result {
                Ok(channels) => {
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        ws.apply_channels_info(channels);
                    }
                    mark_workspace_dirty(app, &team);
                }
                Err(e) => tracing::debug!(%team, error = %e, "channels info failed"),
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::DesktopNotificationShown(result)) => {
            if let Err(e) = result {
                tracing::debug!(error = %e, "desktop notification failed");
            }
            Task::none()
        }

        Message::Discovery(crate::app::DiscoveryMessage::CacheSaved {
            team,
            started_at,
            result,
        }) => {
            app.cache_saving.remove(&team);
            match result {
                Ok(()) => {
                    let clean = app
                        .cache_dirty
                        .get(&team)
                        .is_some_and(|dirty_at| *dirty_at <= started_at);
                    if clean {
                        app.cache_dirty.remove(&team);
                    }
                }
                Err(e) => tracing::warn!(%team, error = %e, "cache save failed"),
            }
            flush_due_cache(app, Instant::now())
        }

        _ => unreachable!("message routed to the wrong discovery reducer"),
    }
}
