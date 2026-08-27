use super::*;

pub type TeamId = String;
pub type ChannelId = String;
pub type UserId = String;
pub type MessageTs = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(flatten)]
    pub body: T,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResponseMetadata {
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryPage {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub unchanged_messages: Vec<MessageTs>,
    #[serde(default)]
    pub latest_updates: BTreeMap<MessageTs, String>,
    #[serde(default)]
    pub pin_count: Option<u32>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CountsPage {
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub ims: Vec<Channel>,
    #[serde(default)]
    pub groups: Vec<Channel>,
    #[serde(default)]
    pub mpims: Vec<Channel>,
    #[serde(default)]
    pub activity_v2: Option<BTreeMap<String, u32>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SidebarDmsPage {
    #[serde(default)]
    pub ims: Vec<Channel>,
    #[serde(default)]
    pub mpdms: Vec<Channel>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SidebarDmsPage {
    pub fn all_channels(&self) -> Vec<Channel> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for channel in self.ims.iter().chain(&self.mpdms) {
            if seen.insert(channel.id.clone()) {
                out.push(channel.clone());
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientDmsPage {
    #[serde(default)]
    pub dms: Vec<DmEntry>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DmEntry {
    pub id: ChannelId,
    #[serde(default)]
    pub latest: Option<MessageTs>,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub channel: Option<Channel>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl CountsPage {
    pub fn activity_unread_count(&self) -> Option<u32> {
        self.activity_v2
            .as_ref()
            .map(|counts| counts.values().copied().fold(0, u32::saturating_add))
    }

    pub fn all_channels(&self) -> Vec<Channel> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for channel in self
            .channels
            .iter()
            .chain(&self.groups)
            .chain(&self.ims)
            .chain(&self.mpims)
        {
            if seen.insert(channel.id.clone()) {
                out.push(channel.clone());
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub user: Option<UserId>,
    #[serde(default)]
    pub bot_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub bot_profile: Option<BotProfile>,
    #[serde(default)]
    pub icons: Option<MessageIcons>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub ts: Option<MessageTs>,
    #[serde(default)]
    pub client_msg_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub team: Option<TeamId>,
    #[serde(default)]
    pub channel: Option<ChannelId>,
    #[serde(default)]
    pub thread_ts: Option<MessageTs>,
    #[serde(default)]
    pub parent_user_id: Option<UserId>,
    #[serde(default)]
    pub reply_count: Option<u32>,
    #[serde(default)]
    pub reply_users_count: Option<u32>,
    #[serde(default)]
    pub latest_reply: Option<MessageTs>,
    #[serde(default)]
    pub reply_users: Vec<UserId>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub blocks: Vec<Value>,
    #[serde(default)]
    pub files: Vec<File>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub edited: Option<Value>,
    #[serde(default)]
    pub message: Option<Box<Message>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotProfile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub user_id: Option<UserId>,
    #[serde(default)]
    pub icons: Option<MessageIcons>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageIcons {
    #[serde(default)]
    pub image_36: Option<String>,
    #[serde(default)]
    pub image_48: Option<String>,
    #[serde(default)]
    pub image_64: Option<String>,
    #[serde(default)]
    pub image_72: Option<String>,
    #[serde(default)]
    pub image_192: Option<String>,
    #[serde(default)]
    pub image_512: Option<String>,
    #[serde(default)]
    pub image_original: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub service_icon: Option<String>,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub author_link: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_link: Option<String>,
    #[serde(default)]
    pub pretext: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub thumb_url: Option<String>,
    #[serde(default)]
    pub from_url: Option<String>,
    #[serde(default)]
    pub original_url: Option<String>,
    #[serde(default)]
    pub fields: Vec<AttachmentField>,
    #[serde(default)]
    pub blocks: Vec<Value>,
    #[serde(default)]
    pub files: Vec<File>,
    #[serde(default)]
    pub footer_icon: Option<String>,
    #[serde(default)]
    pub author_icon: Option<String>,
    #[serde(default)]
    pub author_id: Option<UserId>,
    #[serde(default)]
    pub author_subname: Option<String>,
    /// Origin conversation for a shared-message unfurl.
    #[serde(default)]
    pub channel_id: Option<ChannelId>,
    #[serde(default)]
    pub channel_team: Option<TeamId>,
    /// Timestamp of the *referenced* message, not the one carrying the unfurl.
    /// Slack sends a `"1787153585.078499"` string for message unfurls but a bare
    /// epoch integer for legacy bot attachments, so accept both.
    #[serde(default, deserialize_with = "deserialize_timestamp")]
    pub ts: Option<MessageTs>,
    #[serde(default)]
    pub is_msg_unfurl: bool,
    #[serde(default)]
    pub is_reply_unfurl: bool,
    #[serde(default)]
    pub is_thread_root_unfurl: bool,
    #[serde(default)]
    pub is_app_unfurl: bool,
    #[serde(default)]
    pub is_share: bool,
    #[serde(default)]
    pub image_width: Option<u32>,
    #[serde(default)]
    pub image_height: Option<u32>,
    #[serde(default)]
    pub mrkdwn_in: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttachmentField {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub short: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SentMessage {
    pub channel: ChannelId,
    pub ts: MessageTs,
    pub message: Message,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reaction {
    pub name: String,
    #[serde(default)]
    pub users: Vec<UserId>,
    #[serde(default)]
    pub count: u32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct File {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub mimetype: Option<String>,
    #[serde(default)]
    pub filetype: Option<String>,
    #[serde(default)]
    pub pretty_type: Option<String>,
    #[serde(default)]
    pub url_private: Option<String>,
    #[serde(default)]
    pub thumb_64: Option<String>,
    #[serde(default)]
    pub thumb_80: Option<String>,
    #[serde(default)]
    pub thumb_160: Option<String>,
    #[serde(default)]
    pub thumb_360: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub is_external: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// What a file looks like when it is drawn inline in the transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub url: String,
    pub mime: String,
    /// Intrinsic size of the preview, so the row reserves its space before the
    /// bytes land instead of reflowing the timeline under the reader.
    pub size: Option<(u32, u32)>,
}

impl File {
    fn extra_str(&self, key: &str) -> Option<&str> {
        self.extra
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    }

    fn extra_dim(&self, key: &str) -> Option<u32> {
        let value = self.extra.get(key)?;
        value
            .as_u64()
            .or_else(|| value.as_str()?.parse::<u64>().ok())
            .map(|value| value as u32)
    }

    /// Slack sends the small thumbnails as named fields and the larger ones as
    /// plain extras; both are just URLs.
    fn thumb(&self, key: &str) -> Option<&str> {
        let typed = match key {
            "thumb_64" => self.thumb_64.as_deref(),
            "thumb_80" => self.thumb_80.as_deref(),
            "thumb_160" => self.thumb_160.as_deref(),
            "thumb_360" => self.thumb_360.as_deref(),
            _ => None,
        };
        typed
            .filter(|url| !url.is_empty())
            .or_else(|| self.extra_str(key))
    }

    fn thumb_size(&self, key: &str) -> Option<(u32, u32)> {
        Some((
            self.extra_dim(&format!("{key}_w"))?,
            self.extra_dim(&format!("{key}_h"))?,
        ))
    }

    pub fn mime(&self) -> &str {
        self.mimetype.as_deref().unwrap_or_default()
    }

    /// The bytes worth painting inline — never the original upload.
    ///
    /// Slack transcodes every upload into thumbnails on the way in, and the
    /// originals are routinely enormous: a 61 MB QuickTime, a 7 MB GIF, a
    /// 600 KB photo, all to fill a box a few hundred pixels wide. Downloading
    /// those is what leaves attachments sitting grey — and a video download
    /// times out and retries forever, starving every other image behind it.
    pub fn preview(&self) -> Option<FilePreview> {
        let mime = self.mime();
        if mime.starts_with("video/") {
            // A video's inline face is its poster frame. The movie itself is
            // fetched only when the viewer asks for it.
            let url = self.thumb("thumb_video")?;
            return Some(FilePreview {
                url: url.to_owned(),
                mime: "image/jpeg".to_owned(),
                size: self.thumb_size("thumb_video"),
            });
        }
        if mime == "image/gif" {
            // The `_gif` thumbnails still animate, at a fraction of the weight.
            for key in ["thumb_480_gif", "thumb_360_gif"] {
                if let Some(url) = self.thumb(key) {
                    let base = key.trim_end_matches("_gif");
                    return Some(FilePreview {
                        url: url.to_owned(),
                        mime: "image/gif".to_owned(),
                        size: self.thumb_size(base),
                    });
                }
            }
        }
        if mime.starts_with("image/") {
            for key in ["thumb_800", "thumb_720", "thumb_480", "thumb_360"] {
                if let Some(url) = self.thumb(key) {
                    return Some(FilePreview {
                        url: url.to_owned(),
                        mime: mime.to_owned(),
                        size: self.thumb_size(key),
                    });
                }
            }
            // External or already-small images arrive without thumbnails.
            return Some(FilePreview {
                url: self.url_private.clone().filter(|url| !url.is_empty())?,
                mime: mime.to_owned(),
                size: Some((self.extra_dim("original_w")?, self.extra_dim("original_h")?))
                    .filter(|(w, h)| *w > 0 && *h > 0),
            });
        }
        None
    }

    /// Full-resolution bytes: what the viewer opens and what a download saves.
    pub fn full_url(&self) -> Option<&str> {
        if self.mime().starts_with("video/")
            && let Some(url) = self.extra_str("mp4")
        {
            return Some(url);
        }
        self.url_private
            .as_deref()
            .filter(|url| !url.is_empty())
            .or_else(|| self.thumb("thumb_video"))
    }
}

/// Slack attachment timestamps arrive as either `"1787153585.078499"` or a bare
/// epoch number. Normalize both to the string form the rest of the app uses.
fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<Option<MessageTs>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::String(value)) => Some(value).filter(|value| !value.is_empty()),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod file_tests {
    use super::*;

    fn file(json: serde_json::Value) -> File {
        serde_json::from_value(json).expect("file")
    }

    #[test]
    fn a_video_previews_as_its_poster_and_opens_as_the_mp4() {
        // Shapes taken from a real capture: the original is 61 MB.
        let file = file(serde_json::json!({
            "mimetype": "video/quicktime",
            "url_private": "https://files.slack.com/files-pri/T1-F1/clip.mov",
            "mp4": "https://files.slack.com/files-tmb/T1-F1/clip.mp4",
            "thumb_video": "https://files.slack.com/files-tmb/T1-F1/clip_thumb.jpg",
            "thumb_video_w": 640,
            "thumb_video_h": 360
        }));

        let preview = file.preview().expect("poster");
        assert!(preview.url.ends_with("clip_thumb.jpg"));
        assert_eq!(preview.mime, "image/jpeg");
        assert_eq!(preview.size, Some((640, 360)));
        assert!(file.full_url().expect("movie").ends_with("clip.mp4"));
    }

    #[test]
    fn a_gif_previews_as_the_animated_thumbnail() {
        let file = file(serde_json::json!({
            "mimetype": "image/gif",
            "url_private": "https://files.slack.com/files-pri/T1-F2/huge.gif",
            "thumb_360": "https://files.slack.com/files-tmb/T1-F2/huge_360.png",
            "thumb_360_gif": "https://files.slack.com/files-tmb/T1-F2/huge_360.gif",
            "thumb_480_gif": "https://files.slack.com/files-tmb/T1-F2/huge_480.gif",
            "thumb_480_w": 480,
            "thumb_480_h": 270
        }));

        let preview = file.preview().expect("preview");
        assert!(preview.url.ends_with("huge_480.gif"), "{}", preview.url);
        assert_eq!(preview.mime, "image/gif");
        assert_eq!(preview.size, Some((480, 270)));
    }

    #[test]
    fn a_photo_previews_at_the_largest_thumbnail_slack_made() {
        let file = file(serde_json::json!({
            "mimetype": "image/jpeg",
            "url_private": "https://files.slack.com/files-pri/T1-F3/photo.jpg",
            "thumb_360": "https://files.slack.com/files-tmb/T1-F3/photo_360.jpg",
            "thumb_800": "https://files.slack.com/files-tmb/T1-F3/photo_800.jpg",
            "thumb_800_w": 800,
            "thumb_800_h": 600
        }));

        let preview = file.preview().expect("preview");
        assert!(preview.url.ends_with("photo_800.jpg"));
        assert_eq!(preview.size, Some((800, 600)));
        assert!(file.full_url().expect("original").ends_with("photo.jpg"));
    }

    #[test]
    fn an_image_without_thumbnails_falls_back_to_the_original() {
        let file = file(serde_json::json!({
            "mimetype": "image/png",
            "url_private": "https://example.test/tiny.png",
            "original_w": 32,
            "original_h": 32
        }));

        let preview = file.preview().expect("preview");
        assert_eq!(preview.url, "https://example.test/tiny.png");
        assert_eq!(preview.size, Some((32, 32)));
    }

    #[test]
    fn a_document_has_nothing_to_draw_inline() {
        let file = file(serde_json::json!({
            "mimetype": "application/pdf",
            "url_private": "https://files.slack.com/files-pri/T1-F4/spec.pdf"
        }));

        assert!(file.preview().is_none());
        assert!(file.full_url().expect("download").ends_with("spec.pdf"));
    }
}
