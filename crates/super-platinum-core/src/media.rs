use std::fmt;
use std::str::FromStr;

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
}

impl FromStr for MediaAssetId {
    type Err = MediaAssetIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value
            .strip_prefix("super-platinum-media://")
            .ok_or(MediaAssetIdError)?;
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
