mod store;

use std::fmt;
use std::str::FromStr;

pub use store::{CachedImage, MediaStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaAssetKind {
    Avatar,
    Emoji,
    Attachment,
    Background,
}

impl MediaAssetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Avatar => "avatar",
            Self::Emoji => "emoji",
            Self::Attachment => "attachment",
            Self::Background => "background",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaAssetId {
    kind: MediaAssetKind,
    opaque: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaAssetIdError;

impl fmt::Display for MediaAssetIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Super Platinum media asset id")
    }
}

impl std::error::Error for MediaAssetIdError {}

impl MediaAssetId {
    pub fn new(kind: MediaAssetKind) -> Self {
        Self {
            kind,
            opaque: uuid::Uuid::new_v4().simple().to_string(),
        }
    }

    pub fn kind(&self) -> MediaAssetKind {
        self.kind
    }

    pub fn opaque(&self) -> &str {
        &self.opaque
    }

    pub fn uri(&self) -> String {
        format!(
            "super-platinum-media://{}/{}",
            self.kind.as_str(),
            self.opaque
        )
    }

    /// URI stamped with a media generation. Sources are registered during
    /// projection and fetched afterwards, so a WebView `img` painted before the
    /// bytes land holds a failed request forever. `src` is a diffed attribute
    /// (an element `key` is not), so folding the generation into the URL is what
    /// actually makes the image retry once the asset arrives.
    pub fn uri_at(&self, generation: u64) -> String {
        format!("{}?v={generation}", self.uri())
    }
}

impl FromStr for MediaAssetId {
    type Err = MediaAssetIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value
            .strip_prefix("super-platinum-media://")
            .ok_or(MediaAssetIdError)?;
        // Ignore the `?v=` generation stamp `uri_at` adds for cache busting.
        let value = value.split(['?', '#']).next().unwrap_or_default();
        let (kind, opaque) = value.split_once('/').ok_or(MediaAssetIdError)?;
        let kind = match kind {
            "avatar" => MediaAssetKind::Avatar,
            "emoji" => MediaAssetKind::Emoji,
            "attachment" => MediaAssetKind::Attachment,
            "background" => MediaAssetKind::Background,
            _ => return Err(MediaAssetIdError),
        };
        if opaque.len() != 32 || !opaque.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MediaAssetIdError);
        }
        Ok(Self {
            kind,
            opaque: opaque.to_ascii_lowercase(),
        })
    }
}

/// The image format the bytes actually are, which is not always what the
/// source advertised: Slack serves PNG avatars from `.jpg` URLs, and a wrong
/// `Content-Type` makes the WebView refuse to paint them.
pub fn detect_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_ids_round_trip_without_credentials() {
        let id = MediaAssetId::new(MediaAssetKind::Attachment);
        let uri = id.uri();
        assert_eq!(uri.parse::<MediaAssetId>().unwrap(), id);
        assert!(!uri.contains("token"));
        assert!(
            "https://files.slack.com/token"
                .parse::<MediaAssetId>()
                .is_err()
        );
    }
}
