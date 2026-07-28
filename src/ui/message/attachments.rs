use super::*;

pub(super) fn attachment_row<'a>(
    ws: &Workspace,
    channel_id: &str,
    msg: &SlackMessage,
    att: &crate::slack::models::Attachment,
    file_previews: &HashMap<String, FilePreview>,
    animation_elapsed: Duration,
) -> Element<'a, Message> {
    let mut content = Column::new().spacing(theme::SPACE_XS);

    if let Some(service) = non_empty(att.service_name.as_deref()) {
        content = content.push(
            text(service.to_owned())
                .size(theme::TEXT_SM)
                .color(theme::muted()),
        );
    }
    if let Some(author) = non_empty(att.author_name.as_deref()) {
        content = content.push(
            text(author.to_owned())
                .size(theme::TEXT_SM)
                .color(theme::muted()),
        );
    }
    if let Some(title) = non_empty(att.title.as_deref()) {
        let styled = text(title.to_owned()).size(theme::TEXT_MD).font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::default()
        });
        let widget: Element<'a, Message> = match non_empty(att.title_link.as_deref()) {
            Some(link) => button(styled.color(theme::accent()))
                .padding(0)
                .style(theme::link_button)
                .on_press(Message::Discovery(crate::app::DiscoveryMessage::OpenUrl(
                    link.to_owned(),
                )))
                .into(),
            None => styled.into(),
        };
        content = content.push(widget);
    }
    if let Some(body) = non_empty(att.text.as_deref()) {
        content = content.push(text(body.to_owned()).size(theme::TEXT_SM));
    }
    for field in &att.fields {
        let title = non_empty(field.title.as_deref());
        let value = non_empty(field.value.as_deref());
        let line = match (title, value) {
            (Some(t), Some(v)) => format!("{t}: {v}"),
            (Some(t), None) => t.to_owned(),
            (None, Some(v)) => v.to_owned(),
            (None, None) => continue,
        };
        content = content.push(text(line).size(theme::TEXT_SM));
    }

    for media in state::attachment_images(att) {
        let Some(preview) = file_previews.get(media.preview_url) else {
            continue;
        };
        let handle = match preview {
            FilePreview::Loaded(handle) => Some(handle.clone()),
            FilePreview::Animated {
                frames,
                delays,
                total,
                ..
            } => animated_frame(frames, delays, *total, animation_elapsed),
            FilePreview::Loading => {
                content = content.push(
                    text("Loading preview...")
                        .size(theme::TEXT_SM)
                        .color(theme::muted()),
                );
                None
            }
            FilePreview::Failed => {
                content = content.push(
                    text("Preview unavailable")
                        .size(theme::TEXT_SM)
                        .color(theme::muted()),
                );
                None
            }
        };
        let Some(handle) = handle else {
            continue;
        };
        let (width, height) = attachment_preview_dimensions(media.width, media.height);
        let open = Message::Discovery(crate::app::DiscoveryMessage::ImageViewerOpened(
            image_viewer_source(
                ws,
                channel_id,
                msg,
                None,
                MediaViewerKind::Image,
                media.preview_url.to_owned(),
                media.full_url.to_owned(),
                media.full_url.to_owned(),
                ImageFetchAuth::Public,
                state::attachment_download_name(att),
            ),
        ));
        let preview: Element<'a, Message> = image::Image::new(handle)
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .content_fit(ContentFit::Contain)
            .border_radius(6.0)
            .into();
        content = content.push(
            container(
                mouse_area(preview)
                    .on_press(open)
                    .interaction(iced::mouse::Interaction::Pointer),
            )
            .id(iced::widget::Id::from(format!(
                "image-preview:{}",
                media.preview_url
            ))),
        );
    }

    container(content)
        .padding(theme::SPACE_SM)
        .style(theme::file_attachment)
        .into()
}

pub(super) fn non_empty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

pub(super) fn avatar<'a>(
    key: Option<&str>,
    url: Option<&str>,
    avatar_previews: &HashMap<String, FilePreview>,
    fallback: Option<char>,
) -> Element<'a, Message> {
    avatar_with_size(
        key,
        url,
        avatar_previews,
        fallback,
        theme::MSG_AVATAR,
        theme::MSG_AVATAR_RADIUS,
    )
}

pub fn avatar_with_size<'a>(
    key: Option<&str>,
    url: Option<&str>,
    avatar_previews: &HashMap<String, FilePreview>,
    fallback: Option<char>,
    size: f32,
    radius: f32,
) -> Element<'a, Message> {
    if let Some(key) = key {
        if url.is_some() {
            if let Some(FilePreview::Loaded(handle)) = avatar_previews.get(key) {
                return image::Image::new(handle.clone())
                    .width(Length::Fixed(size))
                    .height(Length::Fixed(size))
                    .content_fit(ContentFit::Cover)
                    .border_radius(radius)
                    .into();
            }
        }
    }

    let initial = fallback
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_owned());
    container(text(initial).size(theme::TEXT_SM).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::default()
    }))
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .center_x(Length::Fixed(size))
    .center_y(Length::Fixed(size))
    .style(theme::avatar_placeholder)
    .into()
}

pub(super) fn file_row<'a>(
    ws: &Workspace,
    channel_id: &str,
    msg: &SlackMessage,
    file: &crate::slack::models::File,
    file_previews: &HashMap<String, FilePreview>,
    animation_elapsed: Duration,
    hovered: bool,
) -> Element<'a, Message> {
    let title = file
        .name
        .as_deref()
        .and_then(|name| non_empty(Some(name)))
        .map(str::to_owned)
        .unwrap_or_else(|| state::file_title(file));
    let (preview_width, preview_height) = file_preview_dimensions(file);
    let download = state::file_download_url(file)
        .map(str::to_owned)
        .map(|url| {
            Message::Discovery(crate::app::DiscoveryMessage::FileDownloadPressed {
                auth: if state::is_slack_authenticated_url(&url) {
                    ImageFetchAuth::Slack
                } else {
                    ImageFetchAuth::Public
                },
                url,
                filename: state::file_download_name(file),
            })
        });
    let kind = if state::is_video_file(file) {
        Some(MediaViewerKind::Video)
    } else if state::is_image_file(file) {
        Some(MediaViewerKind::Image)
    } else {
        None
    };
    let open = kind.and_then(|kind| {
        state::file_viewer_url(file)
            .zip(state::file_preview_key(file))
            .map(|(url, key)| {
                Message::Discovery(crate::app::DiscoveryMessage::ImageViewerOpened(
                    image_viewer_source(
                        ws,
                        channel_id,
                        msg,
                        state::file_uploader_id(file),
                        kind,
                        key,
                        url.to_owned(),
                        state::file_download_url(file).unwrap_or(url).to_owned(),
                        if state::is_slack_authenticated_url(url) {
                            ImageFetchAuth::Slack
                        } else {
                            ImageFetchAuth::Public
                        },
                        state::file_download_name(file),
                    ),
                ))
            })
    });
    let mut content = Column::new()
        .spacing(theme::SPACE_XS)
        .push(text(title).size(theme::TEXT_SM).color(theme::text_3()));

    if let Some(preview) = state::file_preview_key(file).and_then(|key| file_previews.get(&key)) {
        match preview {
            FilePreview::Loaded(handle) => {
                let preview = image::Image::new(handle.clone())
                    .width(Length::Fixed(preview_width))
                    .height(Length::Fixed(preview_height))
                    .content_fit(ContentFit::Contain)
                    .border_radius(6.0)
                    .into();
                content = content.push(file_preview(
                    preview,
                    download.clone(),
                    open.clone(),
                    state::file_preview_key(file).map(|key| format!("image-preview:{key}")),
                    hovered,
                    preview_width,
                    preview_height,
                ));
            }
            FilePreview::Loading => {
                content = content.push(
                    text("Loading preview...")
                        .size(theme::TEXT_SM)
                        .color(theme::muted()),
                );
            }
            FilePreview::Failed => {
                content = content.push(
                    text("Preview unavailable")
                        .size(theme::TEXT_SM)
                        .color(theme::muted()),
                );
            }
            FilePreview::Animated {
                frames,
                delays,
                total,
                ..
            } => {
                if let Some(handle) = animated_frame(frames, delays, *total, animation_elapsed) {
                    let preview = image::Image::new(handle)
                        .width(Length::Fixed(preview_width))
                        .height(Length::Fixed(preview_height))
                        .content_fit(ContentFit::Contain)
                        .border_radius(6.0)
                        .into();
                    content = content.push(file_preview(
                        preview,
                        download.clone(),
                        open.clone(),
                        state::file_preview_key(file).map(|key| format!("image-preview:{key}")),
                        hovered,
                        preview_width,
                        preview_height,
                    ));
                }
            }
        }
    }

    content.into()
}

pub(super) fn file_preview<'a>(
    preview: Element<'a, Message>,
    download: Option<Message>,
    open: Option<Message>,
    preview_id: Option<String>,
    hovered: bool,
    width: f32,
    height: f32,
) -> Element<'a, Message> {
    let preview: Element<'a, Message> = match open {
        Some(message) => {
            let area = mouse_area(preview)
                .on_press(message)
                .interaction(iced::mouse::Interaction::Pointer);
            match preview_id {
                Some(id) => container(area).id(iced::widget::Id::from(id)).into(),
                None => area.into(),
            }
        }
        None => preview,
    };
    let Some(download) = download.filter(|_| hovered) else {
        return preview;
    };

    let icon = svg(icons::download())
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(theme::sidebar_icon(theme::text_1()));
    let action = container(
        button(icon)
            .padding(7.0)
            .style(theme::action_button)
            .on_press(download),
    )
    .width(Length::Fixed(width))
    .height(Length::Fixed(height))
    .padding(8.0)
    .align_right(Length::Fill)
    .align_bottom(Length::Fill);

    stack![preview, action].into()
}

pub(super) fn image_viewer_source(
    ws: &Workspace,
    channel_id: &str,
    msg: &SlackMessage,
    author_override: Option<&str>,
    kind: MediaViewerKind,
    preview_key: String,
    full_url: String,
    download_url: String,
    fetch_auth: ImageFetchAuth,
    filename: String,
) -> ImageViewerSource {
    let (author_name, avatar_key) = match author_override {
        Some(user) => (ws.display_name(user), Some(user.to_owned())),
        None => {
            let (avatar_key, _) = ws.message_avatar(msg);
            (ws.message_author_name(msg), avatar_key)
        }
    };
    let conversation = ws
        .channels
        .get(channel_id)
        .map(|channel| {
            let label = state::channel_display_name(ws, channel);
            if channel.is_im || channel.is_mpim {
                label
            } else {
                format!("#{label}")
            }
        })
        .unwrap_or_else(|| format!("#{channel_id}"));
    ImageViewerSource {
        kind,
        preview_key,
        full_url,
        download_url,
        fetch_auth,
        filename,
        author_name,
        avatar_key,
        timestamp: msg.ts.clone().unwrap_or_default(),
        conversation,
    }
}

pub(super) fn file_preview_dimensions(file: &crate::slack::models::File) -> (f32, f32) {
    const MAX: f32 = 320.0;
    const FALLBACK: (f32, f32) = (260.0, 160.0);
    let dimension = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| file.extra.get(*key)?.as_f64())
            .filter(|value| *value > 0.0)
    };
    let Some(width) = dimension(&["original_w", "thumb_video_w", "thumb_360_w"]) else {
        return FALLBACK;
    };
    let Some(height) = dimension(&["original_h", "thumb_video_h", "thumb_360_h"]) else {
        return FALLBACK;
    };
    let scale = (MAX / width.max(height) as f32).min(1.0);
    (width as f32 * scale, height as f32 * scale)
}

pub(super) fn attachment_preview_dimensions(width: Option<u32>, height: Option<u32>) -> (f32, f32) {
    const MAX: f32 = 320.0;
    const FALLBACK: (f32, f32) = (260.0, 160.0);
    let Some((width, height)) = width.zip(height) else {
        return FALLBACK;
    };
    let (width, height) = (width as f32, height as f32);
    let scale = (MAX / width.max(height)).min(1.0);
    (width * scale, height * scale)
}
