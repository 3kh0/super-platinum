use crate::slack::models::File;

use super::util::non_empty;

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
