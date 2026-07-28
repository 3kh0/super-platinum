use super::*;

pub(super) fn append_unique(ids: &mut Vec<ChannelId>, id: ChannelId) {
    if !ids.iter().any(|existing| existing == &id) {
        ids.push(id);
    }
}

pub(super) fn merge_channel_metadata(existing: &mut Channel, update: Channel) {
    if update
        .name
        .as_ref()
        .is_some_and(|name| !name.trim().is_empty())
    {
        existing.name = update.name;
    }
    existing.is_channel |= update.is_channel;
    existing.is_group |= update.is_group;
    existing.is_im |= update.is_im;
    existing.is_mpim |= update.is_mpim;
    existing.is_private |= update.is_private;
    existing.is_archived |= update.is_archived;
    existing.is_starred |= update.is_starred;
    existing.has_unreads |= update.has_unreads;
    existing.updated = update.updated.or(existing.updated);
    existing.user = update.user.or_else(|| existing.user.take());
    existing.unread_count = update.unread_count.or(existing.unread_count);
    existing.unread_count_display = update
        .unread_count_display
        .or(existing.unread_count_display);
    existing.mention_count = update.mention_count.or(existing.mention_count);
    existing.last_read = update.last_read.or_else(|| existing.last_read.take());
    existing.extra.extend(update.extra);
}

pub fn dm_user_id(channel: &Channel) -> Option<&str> {
    if let Some(user) = channel.user.as_deref() {
        return Some(user);
    }
    for key in ["user", "user_id"] {
        if let Some(user) = channel.extra.get(key).and_then(serde_json::Value::as_str) {
            return Some(user);
        }
    }
    None
}

pub fn display_name(user: Option<&User>, user_id: &str) -> String {
    let Some(user) = user else {
        return user_id.to_owned();
    };
    if let Some(profile) = &user.profile {
        if let Some(dn) = non_empty(profile.display_name.as_deref()) {
            return dn.to_owned();
        }
        if let Some(rn) = non_empty(profile.real_name.as_deref()) {
            return rn.to_owned();
        }
    }
    if let Some(rn) = non_empty(user.real_name.as_deref()) {
        return rn.to_owned();
    }
    if let Some(name) = non_empty(user.name.as_deref()) {
        return name.to_owned();
    }
    user_id.to_owned()
}

pub fn user_avatar_url(user: &User) -> Option<&str> {
    let profile = user.profile.as_ref()?;
    non_empty(profile.image_48.as_deref())
        .or_else(|| non_empty(profile.image_72.as_deref()))
        .or_else(|| non_empty(profile.image_32.as_deref()))
        .or_else(|| non_empty(profile.image_24.as_deref()))
        .or_else(|| non_empty(profile.image_192.as_deref()))
        .or_else(|| non_empty(profile.image_512.as_deref()))
        .or_else(|| non_empty(profile.image_original.as_deref()))
}

pub fn user_profile_image_url(user: &User) -> Option<&str> {
    let profile = user.profile.as_ref()?;
    non_empty(profile.image_original.as_deref())
        .or_else(|| non_empty(profile.image_512.as_deref()))
        .or_else(|| non_empty(profile.image_192.as_deref()))
        .or_else(|| non_empty(profile.image_72.as_deref()))
        .or_else(|| non_empty(profile.image_48.as_deref()))
        .or_else(|| non_empty(profile.image_32.as_deref()))
        .or_else(|| non_empty(profile.image_24.as_deref()))
}

pub fn message_author_name(ws: &Workspace, msg: &SlackMessage) -> String {
    if let Some(user) = msg
        .user
        .as_deref()
        .filter(|_| msg.bot_profile.is_none() && msg.bot_id.is_none())
    {
        return ws.display_name(user);
    }
    if let Some(name) = non_empty(msg.username.as_deref()) {
        return name.to_owned();
    }
    if let Some(name) = msg
        .bot_profile
        .as_ref()
        .and_then(|profile| non_empty(profile.name.as_deref()))
    {
        return name.to_owned();
    }
    if let Some(user) = msg.user.as_deref() {
        return ws.display_name(user);
    }
    msg.bot_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

pub fn message_avatar(ws: &Workspace, msg: &SlackMessage) -> (Option<String>, Option<String>) {
    if let Some(user) = msg
        .user
        .as_deref()
        .filter(|_| msg.bot_profile.is_none() && msg.bot_id.is_none())
    {
        return (Some(user.to_owned()), ws.avatar_url(user));
    }
    if let Some((key, url)) = message_bot_avatar(msg) {
        return (Some(key), Some(url));
    }
    if let Some(user) = msg.user.as_deref() {
        return (Some(user.to_owned()), ws.avatar_url(user));
    }
    (None, None)
}

pub fn message_bot_avatar(msg: &SlackMessage) -> Option<(String, String)> {
    let source = msg
        .bot_id
        .as_deref()
        .or_else(|| {
            msg.bot_profile
                .as_ref()
                .and_then(|profile| profile.id.as_deref())
        })
        .or_else(|| {
            msg.bot_profile
                .as_ref()
                .and_then(|profile| profile.user_id.as_deref())
        })?;
    let url = msg
        .bot_profile
        .as_ref()
        .and_then(|profile| profile.icons.as_ref())
        .and_then(message_icon_url)
        .or_else(|| msg.icons.as_ref().and_then(message_icon_url))?
        .to_owned();
    let key = format!("bot-icon:{source}:{url}");
    Some((key, url))
}

pub fn message_icon_url(icons: &crate::slack::models::MessageIcons) -> Option<&str> {
    non_empty(icons.image_48.as_deref())
        .or_else(|| non_empty(icons.image_72.as_deref()))
        .or_else(|| non_empty(icons.image_64.as_deref()))
        .or_else(|| non_empty(icons.image_36.as_deref()))
        .or_else(|| non_empty(icons.image_192.as_deref()))
        .or_else(|| non_empty(icons.image_512.as_deref()))
        .or_else(|| non_empty(icons.image_original.as_deref()))
        .or_else(|| non_empty(icons.icon_url.as_deref()))
}

pub fn channel_label(channel: &Channel) -> String {
    if let Some(name) = non_empty(channel.name.as_deref()) {
        if channel.is_im {
            return name.to_owned();
        }
        return format!("#{name}");
    }
    if channel.is_im {
        return "direct message".to_owned();
    }
    channel.id.clone()
}

pub fn channel_display_name(ws: &Workspace, c: &Channel) -> String {
    if c.is_im || c.is_mpim {
        dm_label(ws, c)
    } else {
        channel_name(c)
    }
}

pub fn channel_name(c: &Channel) -> String {
    c.name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(c.id.as_str())
        .to_owned()
}

pub fn dm_label(ws: &Workspace, c: &Channel) -> String {
    if c.is_im {
        if let Some(user) = dm_user_id(c) {
            return ws.display_name(user);
        }
    }
    if c.is_mpim {
        if let Some(name) = c.name.as_deref().and_then(mpdm_name_label) {
            return name;
        }
    }
    channel_label(c).trim_start_matches('#').to_owned()
}

pub fn is_vip_channel(ws: &Workspace, c: &Channel) -> bool {
    if ws.priority_sidebar_section && !ws.vip_users.is_empty() {
        if c.is_im {
            if dm_user_id(c).is_some_and(|user| ws.vip_users.contains(user)) {
                return true;
            }
        } else if latest_author(ws, &c.id).is_some_and(|user| ws.vip_users.contains(user)) {
            return true;
        }
    }
    c.extra.iter().any(|(key, value)| {
        let key = key.to_ascii_lowercase();
        key.contains("vip")
            || (key.contains("priority") && value.as_bool().unwrap_or(false))
            || value_names_vip(value)
    })
}

pub(super) fn linked_list_order(
    sections: &[crate::slack::models::ChannelSection],
) -> Vec<&crate::slack::models::ChannelSection> {
    let by_id: HashMap<&str, &crate::slack::models::ChannelSection> = sections
        .iter()
        .map(|s| (s.channel_section_id.as_str(), s))
        .collect();
    let pointed_at: HashSet<&str> = sections
        .iter()
        .filter_map(|s| s.next_channel_section_id.as_deref())
        .collect();
    let Some(head) = sections
        .iter()
        .find(|s| !pointed_at.contains(s.channel_section_id.as_str()))
    else {
        return sections.iter().collect();
    };
    let mut out = Vec::with_capacity(sections.len());
    let mut seen = HashSet::new();
    let mut cursor = Some(head);
    while let Some(section) = cursor {
        if !seen.insert(section.channel_section_id.as_str()) {
            break;
        }
        out.push(section);
        cursor = section
            .next_channel_section_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
    }
    for section in sections {
        if !seen.contains(section.channel_section_id.as_str()) {
            out.push(section);
        }
    }
    out
}

fn latest_author<'a>(ws: &'a Workspace, channel_id: &str) -> Option<&'a str> {
    ws.messages
        .get(channel_id)?
        .messages
        .iter()
        .filter(|m| m.ts.is_some())
        .max_by(|a, b| cmp_ts(a.ts.as_deref(), b.ts.as_deref()))?
        .user
        .as_deref()
}

fn value_names_vip(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(value) => value.to_ascii_lowercase().contains("vip"),
        serde_json::Value::Array(values) => values.iter().any(value_names_vip),
        serde_json::Value::Object(values) => values
            .iter()
            .any(|(key, value)| key.to_ascii_lowercase().contains("vip") || value_names_vip(value)),
        _ => false,
    }
}

pub fn mpdm_name_label(name: &str) -> Option<String> {
    let rest = name.strip_prefix("mpdm-")?;
    let rest = rest
        .rsplit_once('-')
        .and_then(|(prefix, suffix)| suffix.parse::<u32>().ok().map(|_| prefix))
        .unwrap_or(rest);
    let names: Vec<_> = rest
        .split("--")
        .map(|name| name.replace('.', " "))
        .filter(|name| !name.trim().is_empty())
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

pub fn message_text(msg: &SlackMessage) -> String {
    if let Some(text) = non_empty(msg.text.as_deref()) {
        return text.to_owned();
    }
    if !msg.files.is_empty() || !msg.attachments.is_empty() {
        return String::new();
    }
    if let Some(subtype) = non_empty(msg.subtype.as_deref()) {
        return format!("[{subtype}]");
    }
    if !msg.blocks.is_empty() {
        return "[rich message]".to_owned();
    }
    "[no text]".to_owned()
}

pub fn visible_message(msg: SlackMessage) -> SlackMessage {
    if msg.subtype.as_deref() != Some("message_replied") {
        return msg;
    }

    let Some(nested) = msg.message.clone() else {
        return msg;
    };
    let mut nested = *nested;
    nested.team = nested.team.or(msg.team);
    nested.channel = nested.channel.or(msg.channel);
    nested.reply_count = nested.reply_count.or(msg.reply_count);
    nested.reply_users_count = nested.reply_users_count.or(msg.reply_users_count);
    nested.latest_reply = nested.latest_reply.or(msg.latest_reply);
    if nested.reply_users.is_empty() {
        nested.reply_users = msg.reply_users;
    }
    if nested.reactions.is_empty() {
        nested.reactions = msg.reactions;
    }
    if nested.files.is_empty() {
        nested.files = msg.files;
    }
    if nested.attachments.is_empty() {
        nested.attachments = msg.attachments;
    }
    nested
}

pub fn is_channel_timeline_visible(msg: &SlackMessage) -> bool {
    if msg.subtype.as_deref() == Some("message_replied") {
        return false;
    }
    match (msg.thread_ts.as_deref(), msg.ts.as_deref()) {
        (Some(root), Some(ts)) if root != ts => msg.subtype.as_deref() == Some("thread_broadcast"),
        _ => true,
    }
}

pub fn file_title(file: &File) -> String {
    non_empty(file.title.as_deref())
        .or_else(|| non_empty(file.name.as_deref()))
        .or_else(|| non_empty(file.id.as_deref()))
        .unwrap_or("file")
        .to_owned()
}

pub fn file_summary(file: &File) -> String {
    let mut parts = Vec::new();
    if let Some(kind) = non_empty(file.pretty_type.as_deref())
        .or_else(|| non_empty(file.filetype.as_deref()))
        .or_else(|| non_empty(file.mimetype.as_deref()))
    {
        parts.push(kind.to_owned());
    }
    if let Some(size) = file.size {
        parts.push(format_file_size(size));
    }
    if parts.is_empty() {
        "attachment".to_owned()
    } else {
        parts.join(" - ")
    }
}

pub fn file_download_name(file: &File) -> String {
    sanitize_file_name(
        non_empty(file.name.as_deref())
            .or_else(|| non_empty(file.title.as_deref()))
            .or_else(|| non_empty(file.id.as_deref()))
            .unwrap_or("download"),
    )
}

pub fn file_preview_key(file: &File) -> Option<String> {
    file_preview_key_ref(file).map(str::to_owned)
}

pub fn file_preview_key_ref(file: &File) -> Option<&str> {
    non_empty(file.id.as_deref()).or_else(|| file_preview_url(file))
}

pub fn file_uploader_id(file: &File) -> Option<&str> {
    ["user", "user_id", "uploader"]
        .into_iter()
        .find_map(|key| file.extra.get(key)?.as_str())
        .and_then(|user| non_empty(Some(user)))
}

pub fn is_image_file(file: &File) -> bool {
    if non_empty(file.mimetype.as_deref()).is_some_and(|mime| mime.starts_with("image/")) {
        return true;
    }
    let extension = non_empty(file.filetype.as_deref()).or_else(|| {
        non_empty(file.name.as_deref())
            .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
    });
    extension.is_some_and(|extension| {
        matches!(
            extension.to_ascii_lowercase().as_str(),
            "avif"
                | "bmp"
                | "gif"
                | "heic"
                | "heif"
                | "jpeg"
                | "jpg"
                | "png"
                | "tif"
                | "tiff"
                | "webp"
        )
    })
}

pub fn is_video_file(file: &File) -> bool {
    if non_empty(file.mimetype.as_deref()).is_some_and(|mime| mime.starts_with("video/")) {
        return true;
    }
    let extension = non_empty(file.filetype.as_deref()).or_else(|| {
        non_empty(file.name.as_deref())
            .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
    });
    extension.is_some_and(|extension| {
        matches!(
            extension.to_ascii_lowercase().as_str(),
            "avi" | "m4v" | "mkv" | "mov" | "mp4" | "webm"
        )
    })
}

pub fn file_original_is_viewer_decodable(file: &File) -> bool {
    let kind = non_empty(file.filetype.as_deref())
        .or_else(|| {
            non_empty(file.name.as_deref())
                .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
        })
        .map(str::to_ascii_lowercase);
    !matches!(kind.as_deref(), Some("heic" | "heif"))
        && !matches!(
            non_empty(file.mimetype.as_deref()),
            Some("image/heic" | "image/heif")
        )
}

pub fn file_viewer_url(file: &File) -> Option<&str> {
    let original = non_empty(file.url_private.as_deref());
    if is_image_file(file) && !file_original_is_viewer_decodable(file) {
        file_preview_url(file).or(original)
    } else if is_video_file(file) {
        file.extra
            .get("mp4")
            .and_then(|value| value.as_str())
            .and_then(|url| non_empty(Some(url)))
            .or(original)
    } else {
        original
    }
}

pub fn file_download_url(file: &File) -> Option<&str> {
    file.extra
        .get("url_private_download")
        .and_then(|value| value.as_str())
        .and_then(|url| non_empty(Some(url)))
        .or_else(|| non_empty(file.url_private.as_deref()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentImage<'a> {
    pub preview_url: &'a str,
    pub full_url: &'a str,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub animated: bool,
    pub alt_text: Option<&'a str>,
}

pub fn attachment_images(
    att: &crate::slack::models::Attachment,
) -> impl Iterator<Item = AttachmentImage<'_>> {
    let preview_url =
        non_empty(att.thumb_url.as_deref()).or_else(|| non_empty(att.image_url.as_deref()));
    let full_url = non_empty(att.image_url.as_deref()).or(preview_url);
    let top_level = preview_url
        .zip(full_url)
        .map(|(preview_url, full_url)| AttachmentImage {
            preview_url,
            full_url,
            width: None,
            height: None,
            animated: is_gif_url(full_url),
            alt_text: non_empty(att.title.as_deref()),
        });
    let nested = att
        .blocks
        .iter()
        .enumerate()
        .filter_map(move |(index, block)| {
            if block.get("type").and_then(serde_json::Value::as_str) != Some("image") {
                return None;
            }
            let url = block
                .get("image_url")
                .and_then(serde_json::Value::as_str)
                .and_then(|url| non_empty(Some(url)))?;
            if preview_url == Some(url)
                || full_url == Some(url)
                || att.blocks[..index].iter().any(|previous| {
                    previous
                        .get("image_url")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|previous| previous == url)
                })
            {
                return None;
            }
            Some(AttachmentImage {
                preview_url: url,
                full_url: url,
                width: block
                    .get("image_width")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|value| *value > 0),
                height: block
                    .get("image_height")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|value| *value > 0),
                animated: block
                    .get("is_animated")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or_else(|| is_gif_url(url)),
                alt_text: block
                    .get("alt_text")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|text| non_empty(Some(text))),
            })
        });

    top_level.into_iter().chain(nested)
}

pub fn is_gif_url(url: &str) -> bool {
    url.split(['?', '#'])
        .next()
        .is_some_and(|path| path.to_ascii_lowercase().ends_with(".gif"))
}

pub fn attachment_preview_url(att: &crate::slack::models::Attachment) -> Option<&str> {
    attachment_images(att).next().map(|image| image.preview_url)
}

pub fn attachment_viewer_url(att: &crate::slack::models::Attachment) -> Option<&str> {
    attachment_images(att).next().map(|image| image.full_url)
}

pub fn attachment_download_name(att: &crate::slack::models::Attachment) -> String {
    if let Some(title) = non_empty(att.title.as_deref()) {
        return sanitize_file_name(title);
    }
    let from_url = attachment_viewer_url(att)
        .and_then(|url| url.split(['?', '#']).next())
        .and_then(|url| url.rsplit('/').next())
        .and_then(|name| non_empty(Some(name)));
    sanitize_file_name(from_url.unwrap_or("image"))
}

pub fn is_browser_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub fn is_slack_authenticated_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    host == "slack.com" || host.ends_with(".slack.com")
}

pub fn file_preview_url(file: &File) -> Option<&str> {
    let large_image = [
        "thumb_1024",
        "thumb_960",
        "thumb_800",
        "thumb_720",
        "thumb_480",
    ]
    .into_iter()
    .find_map(|key| {
        file.extra
            .get(key)
            .and_then(|value| value.as_str())
            .and_then(|url| non_empty(Some(url)))
    });
    let video = file
        .extra
        .get("thumb_video")
        .and_then(|value| value.as_str())
        .and_then(|url| non_empty(Some(url)));
    if is_video_file(file) {
        video.or(large_image)
    } else {
        large_image.or(video)
    }
    .or_else(|| non_empty(file.thumb_360.as_deref()))
    .or_else(|| non_empty(file.thumb_160.as_deref()))
    .or_else(|| non_empty(file.thumb_80.as_deref()))
    .or_else(|| non_empty(file.thumb_64.as_deref()))
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|c| {
            if c.is_ascii_control()
                || matches!(c, '/' | '\\' | ':' | '*')
                || matches!(c, '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim_matches(|c| c == ' ' || c == '.')
        .to_owned();
    if sanitized.is_empty() {
        "download".to_owned()
    } else {
        sanitized
    }
}

pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes < 1024 {
        format!("{bytes} B")
    } else if (bytes as f64) < MB {
        format!("{:.1} KB", bytes as f64 / KB)
    } else if (bytes as f64) < GB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else {
        format!("{:.1} GB", bytes as f64 / GB)
    }
}

pub fn format_relative_ts(ts: &str) -> String {
    let (secs, _) = ts_key(ts);
    let elapsed = now_secs().saturating_sub(secs as i64).max(0) as u64;
    match elapsed {
        0..=59 => "now".to_owned(),
        60..=3_599 => format!("{}m ago", elapsed / 60),
        3_600..=86_399 => format!("{}h ago", elapsed / 3_600),
        86_400..=604_799 => format!("{}d ago", elapsed / 86_400),
        _ => format_ts_date_label(ts),
    }
}

pub fn reaction_summary(reaction: &crate::slack::models::Reaction) -> String {
    format!("{} {}", emoji_glyph(&reaction.name), reaction.count.max(1))
}

/// Map a Slack emoji name to its unicode glyph, falling back to `:name:` for
/// custom/unknown emoji. Slack appends skin-tone modifiers like
/// `thumbsup::skin-tone-3`; the base name resolves the glyph.
pub fn emoji_glyph(name: &str) -> String {
    let base = name.split("::").next().unwrap_or(name);
    emojis::get_by_shortcode(base)
        .map(|e| e.as_str().to_owned())
        .unwrap_or_else(|| format!(":{name}:"))
}

pub fn is_standard_emoji(name: &str) -> bool {
    let base = name.split("::").next().unwrap_or(name);
    emojis::get_by_shortcode(base).is_some()
}

pub fn emoji_text_to_display(text: &str) -> String {
    emoji_text_tokens(text)
        .into_iter()
        .map(|token| match token {
            EmojiTextToken::Text(text) => text,
            EmojiTextToken::Emoji(name) => emoji_glyph(&name),
        })
        .collect()
}

pub fn emoji_preview_key(team: &str, name: &str) -> String {
    format!("{team}:{name}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmojiTextToken {
    Text(String),
    Emoji(String),
}

pub fn emoji_text_tokens(text: &str) -> Vec<EmojiTextToken> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        let (before, after_start) = rest.split_at(start);
        if !before.is_empty() {
            out.push(EmojiTextToken::Text(before.to_owned()));
        }

        let after_start = &after_start[1..];
        let Some(end) = after_start.find(':') else {
            out.push(EmojiTextToken::Text(":".to_owned()));
            rest = after_start;
            continue;
        };
        let name = &after_start[..end];
        if is_emoji_name(name) {
            out.push(EmojiTextToken::Emoji(name.to_owned()));
            rest = &after_start[end + 1..];
        } else {
            out.push(EmojiTextToken::Text(":".to_owned()));
            rest = after_start;
        }
    }
    if !rest.is_empty() {
        out.push(EmojiTextToken::Text(rest.to_owned()));
    }
    merge_text_tokens(out)
}

pub fn emoji_names_in_text(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    visit_emoji_names_in_text(text, |name| names.push(name.to_owned()));
    names
}

pub fn visit_emoji_names_in_text<'a>(text: &'a str, mut visit: impl FnMut(&'a str)) {
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find(':') else {
            break;
        };
        let name = &after_start[..end];
        if is_emoji_name(name) {
            visit(name);
            rest = &after_start[end + 1..];
        } else {
            rest = after_start;
        }
    }
}

pub(super) fn custom_emoji_url<'a>(
    emojis: &'a HashMap<String, Emoji>,
    name: &str,
) -> Option<&'a str> {
    let mut current = name;
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current) {
            return None;
        }
        let emoji = emojis.get(current)?;
        if let Some(alias) = emoji.value.strip_prefix("alias:") {
            current = alias;
            continue;
        }
        return is_browser_url(&emoji.value).then_some(emoji.value.as_str());
    }
}

fn is_emoji_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'))
}

fn merge_text_tokens(tokens: Vec<EmojiTextToken>) -> Vec<EmojiTextToken> {
    let mut merged = Vec::new();
    for token in tokens {
        match (merged.last_mut(), token) {
            (Some(EmojiTextToken::Text(existing)), EmojiTextToken::Text(next)) => {
                existing.push_str(&next);
            }
            (_, token) => merged.push(token),
        }
    }
    merged
}

pub fn reaction_has_user(reaction: &Reaction, user: &str) -> bool {
    !user.is_empty() && reaction.users.iter().any(|u| u == user)
}

pub(super) fn apply_message_reaction(
    msg: &mut SlackMessage,
    user: &str,
    name: &str,
    added: bool,
) -> bool {
    if name.is_empty() {
        return false;
    }

    if added {
        if let Some(reaction) = msg.reactions.iter_mut().find(|r| r.name == name) {
            if reaction_has_user(reaction, user) {
                return false;
            }
            if !user.is_empty() {
                reaction.users.push(user.to_owned());
            }
            reaction.count = reaction.count.saturating_add(1).max(1);
            return true;
        }
        msg.reactions.push(Reaction {
            name: name.to_owned(),
            users: if user.is_empty() {
                Vec::new()
            } else {
                vec![user.to_owned()]
            },
            count: 1,
            ..Default::default()
        });
        return true;
    }

    let Some(i) = msg.reactions.iter().position(|r| r.name == name) else {
        return false;
    };
    let reaction = &mut msg.reactions[i];
    let before_users = reaction.users.len();
    reaction.users.retain(|u| u != user);
    let removed_known_user = before_users != reaction.users.len();
    if removed_known_user || reaction.users.is_empty() {
        reaction.count = reaction.count.saturating_sub(1);
    }
    if reaction.count == 0 {
        msg.reactions.remove(i);
    }
    true
}

pub(super) fn merge_message(existing: &mut SlackMessage, update: SlackMessage) {
    existing.user = update.user.or_else(|| existing.user.take());
    existing.bot_id = update.bot_id.or_else(|| existing.bot_id.take());
    existing.username = update.username.or_else(|| existing.username.take());
    existing.bot_profile = update.bot_profile.or_else(|| existing.bot_profile.take());
    existing.icons = update.icons.or_else(|| existing.icons.take());
    existing.kind = update.kind.or_else(|| existing.kind.take());
    existing.subtype = update.subtype.or_else(|| existing.subtype.take());
    existing.client_msg_id = update
        .client_msg_id
        .or_else(|| existing.client_msg_id.take());
    existing.text = update.text.or_else(|| existing.text.take());
    existing.team = update.team.or_else(|| existing.team.take());
    existing.channel = update.channel.or_else(|| existing.channel.take());
    existing.thread_ts = update.thread_ts.or_else(|| existing.thread_ts.take());
    existing.parent_user_id = update
        .parent_user_id
        .or_else(|| existing.parent_user_id.take());
    existing.reply_count = update.reply_count.or(existing.reply_count);
    existing.reply_users_count = update.reply_users_count.or(existing.reply_users_count);
    existing.latest_reply = update.latest_reply.or_else(|| existing.latest_reply.take());
    if !update.reply_users.is_empty() {
        existing.reply_users = update.reply_users;
    }
    if !update.reactions.is_empty() {
        existing.reactions = update.reactions;
    }
    if !update.blocks.is_empty() {
        existing.blocks = update.blocks;
    }
    if !update.files.is_empty() {
        existing.files = update.files;
    }
    if !update.attachments.is_empty() {
        existing.attachments = update.attachments;
    }
    existing.edited = update.edited.or_else(|| existing.edited.take());
    existing.message = update.message.or_else(|| existing.message.take());
    existing.extra.extend(update.extra);
}

pub fn ts_key(ts: &str) -> (u64, u64) {
    let mut parts = ts.splitn(2, '.');
    let secs = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let seq = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (secs, seq)
}

pub fn cmp_ts(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => ts_key(a).cmp(&ts_key(b)),
        (None, None) => std::cmp::Ordering::Equal,
        (None, _) => std::cmp::Ordering::Less,
        (_, None) => std::cmp::Ordering::Greater,
    }
}

pub fn format_ts_hm(ts: &str) -> String {
    use chrono::{Local, TimeZone};
    let (secs, _) = ts_key(ts);
    match Local.timestamp_opt(secs as i64, 0).single() {
        Some(dt) => dt.format("%H:%M").to_string(),
        None => secs.to_string(),
    }
}

pub fn date_key_for_ts(ts: &str) -> Option<String> {
    use chrono::{Datelike, Local, TimeZone};
    let (secs, _) = ts_key(ts);
    let date = Local.timestamp_opt(secs as i64, 0).single()?.date_naive();
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month(),
        date.day()
    ))
}

pub fn format_ts_date_label(ts: &str) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let (secs, _) = ts_key(ts);
    let Some(date_time) = Local.timestamp_opt(secs as i64, 0).single() else {
        return ts.to_owned();
    };
    let date = date_time.date_naive();
    let today = Local::now().date_naive();
    if date == today {
        return "Today".to_owned();
    }
    if today.signed_duration_since(date).num_days() == 1 {
        return "Yesterday".to_owned();
    }
    format!(
        "{}, {} {}",
        date.format("%A"),
        date.format("%B"),
        ordinal_day(date.day())
    )
}

pub(super) fn ordinal_day(day: u32) -> String {
    let suffix = match day % 100 {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{day}{suffix}")
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

pub fn scroll_ratio_for_ts(messages: &[SlackMessage], ts: &str) -> Option<f32> {
    let index = messages.iter().position(|m| m.ts.as_deref() == Some(ts))?;
    let last = messages.len() - 1;
    if last == 0 {
        Some(0.0)
    } else {
        Some(index as f32 / last as f32)
    }
}
