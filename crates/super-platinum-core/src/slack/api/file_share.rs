//! Ordinary existing hosted-file Share; not upload, broadcast, or DLP override.
use super::*;
use serde_json::Value;
use std::io::{self, Write};

pub mod batch;

pub const FILE_SHARE_NOTE_BYTES: usize = 256 * 1024;
pub const FILE_SHARE_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileShareInputError {
    FileId,
    Destination,
    ContextTeam,
    NoteShape,
    NoteLimit,
}

/// JSON block objects preserve unknown metadata; no HTML or text flattening.
/// This checks shape/bounds, not Slack's complete Block Kit schema.
#[derive(Clone, PartialEq)]
pub struct FileShareNote {
    blocks: Vec<Value>,
    encoded: String,
}
impl FileShareNote {
    pub fn new(blocks: Vec<Value>) -> Result<Self, FileShareInputError> {
        if blocks.is_empty()
            || blocks.len() > 100
            || blocks.iter().any(|b| {
                !b.is_object()
                    || b.get("type")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
            })
        {
            return Err(FileShareInputError::NoteShape);
        }
        let value = Value::Array(blocks);
        if !bounded(&value, FILE_SHARE_NOTE_BYTES) {
            return Err(FileShareInputError::NoteLimit);
        }
        let encoded = serde_json::to_string(&value).map_err(|_| FileShareInputError::NoteShape)?;
        let Value::Array(blocks) = value else {
            unreachable!()
        };
        Ok(Self { blocks, encoded })
    }
    pub fn blocks(&self) -> &[Value] {
        &self.blocks
    }
}

/// Validated wire identity, not a permission or owner capability. The caller must
/// admit current source/destination/credentials before executing exactly once.
#[derive(Clone, PartialEq)]
pub struct ExistingFileShare {
    file: String,
    destination: String,
    note: Option<FileShareNote>,
    context_team: Option<String>,
}
impl ExistingFileShare {
    pub fn new(
        file: String,
        destination: String,
        note: Option<FileShareNote>,
    ) -> Result<Self, FileShareInputError> {
        if !id(&file, b"F", 2) {
            return Err(FileShareInputError::FileId);
        }
        if !id(&destination, b"CGD", 9) {
            return Err(FileShareInputError::Destination);
        }
        Ok(Self {
            file,
            destination,
            note,
            context_team: None,
        })
    }
    /// Only supply the source-backed Netskope workspace-context team when its
    /// experiment/configuration is known enabled; never infer it from a channel.
    pub fn with_context_team(mut self, team: String) -> Result<Self, FileShareInputError> {
        if !id(&team, b"T", 9) {
            return Err(FileShareInputError::ContextTeam);
        }
        self.context_team = Some(team);
        Ok(self)
    }
    pub fn file_id(&self) -> &str {
        &self.file
    }
    pub fn destination(&self) -> &str {
        &self.destination
    }
}
fn id(s: &str, prefixes: &[u8], min: usize) -> bool {
    (min..=64).contains(&s.len())
        && prefixes.contains(&s.as_bytes()[0])
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// bw5L ordinary modal flags. No client UUID, permissions, thread, broadcast,
/// comment, silent_share or upload fields. Notes must already have resolved links;
/// this bounded route does not support pending_slug_urls.
pub fn existing_file_share(
    client: &SlackClient,
    session: &WorkspaceSession,
    intent: &ExistingFileShare,
) -> PreparedRequest {
    let mut fields = vec![
        ("files", intent.file.clone()),
        ("channel", intent.destination.clone()),
    ];
    if let Some(note) = &intent.note {
        fields.push(("blocks", note.encoded.clone()));
    }
    fields.extend([
        ("resharing_aware", "true".into()),
        ("skip_dlp_user_warning", "false".into()),
        ("from_share_modal", "false".into()),
    ]);
    if let Some(team) = &intent.context_team {
        fields.push(("client_context_team_id", team.clone()));
    }
    client.rest_form(session, "files.share", fields)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSharePolicy {
    FileType,
    SlackConnectFileType,
    SlackConnectLink,
    SlackConnectUpload,
    PostingDenied,
    RestrictedAction,
    ExternalGuest,
    MessageLimit,
    ShareLimit,
    FileUnavailable,
    DestinationUnavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileShareUncertainty {
    MalformedEnvelope,
    ContradictoryEnvelope,
    UnclassifiedError,
    ResponseLimit,
}
#[derive(Clone, PartialEq, Eq)]
pub enum FileShareDisposition {
    /// Server accepted this request; not proof of a particular local message.
    Accepted {
        message_ts: Option<String>,
    },
    Policy(FileSharePolicy),
    DlpWarning {
        message: Option<String>,
    },
    DlpProcessing,
    Uncertain(FileShareUncertainty),
}
// Debug exposes classification and presence only, never server-provided strings.
impl std::fmt::Debug for FileShareDisposition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted { message_ts } => f
                .debug_struct("Accepted")
                .field("has_message_ts", &message_ts.is_some())
                .finish(),
            Self::Policy(policy) => f.debug_tuple("Policy").field(policy).finish(),
            Self::DlpWarning { message } => f
                .debug_struct("DlpWarning")
                .field("has_message", &message.is_some())
                .finish(),
            Self::DlpProcessing => f.write_str("DlpProcessing"),
            Self::Uncertain(reason) => f.debug_tuple("Uncertain").field(reason).finish(),
        }
    }
}
/// No Debug implementation: retained server payloads are not safe log messages.
pub struct FileShareResponse {
    disposition: FileShareDisposition,
    raw: Option<Value>,
}
impl FileShareResponse {
    pub fn disposition(&self) -> &FileShareDisposition {
        &self.disposition
    }
    /// Present only for bounded responses; preserves all returned unknown fields.
    pub fn raw(&self) -> Option<&Value> {
        self.raw.as_ref()
    }
}
/// Retention bound after transport JSON parsing, not a network/parser allocation
/// bound. Caller owns lost-response/429/unsent distinctions and retained custody.
pub fn decode_existing_file_share(raw: Value) -> FileShareResponse {
    if !bounded(&raw, FILE_SHARE_RESPONSE_BYTES) {
        return FileShareResponse {
            disposition: FileShareDisposition::Uncertain(FileShareUncertainty::ResponseLimit),
            raw: None,
        };
    }
    let disposition = classify(&raw);
    FileShareResponse {
        disposition,
        raw: Some(raw),
    }
}
fn classify(raw: &Value) -> FileShareDisposition {
    use FileShareDisposition::*;
    use FileShareUncertainty::*;
    if !raw.is_object() {
        return Uncertain(MalformedEnvelope);
    }
    match raw.get("ok").and_then(Value::as_bool) {
        Some(true) => {
            if raw.get("error").is_some_and(|e| !e.is_null()) {
                return Uncertain(ContradictoryEnvelope);
            }
            // Optional timestamps are navigation hints only. Invalid/absent
            // hints do not turn an accepted response into permission to resend.
            let message_ts = ["ts", "file_msg_ts"]
                .into_iter()
                .filter_map(|k| raw.get(k).and_then(Value::as_str))
                .find(|s| timestamp(s))
                .map(str::to_owned);
            Accepted { message_ts }
        }
        Some(false) => {
            let Some(code) = raw.get("error").and_then(Value::as_str) else {
                return Uncertain(MalformedEnvelope);
            };
            // A rejection carrying a message receipt is not safe to call denied.
            if ["ts", "file_msg_ts"]
                .iter()
                .any(|k| raw.get(*k).is_some_and(|v| !v.is_null()))
            {
                return Uncertain(ContradictoryEnvelope);
            }
            match code {
                "dlp_user_warning" => DlpWarning {
                    message: raw
                        .get("dlp_custom_message")
                        .and_then(Value::as_str)
                        .filter(|s| s.len() <= 16 * 1024)
                        .map(str::to_owned),
                },
                "dlp_processing" => DlpProcessing,
                other => policy(other)
                    .map(Policy)
                    .unwrap_or(Uncertain(UnclassifiedError)),
            }
        }
        None => Uncertain(MalformedEnvelope),
    }
}
fn policy(code: &str) -> Option<FileSharePolicy> {
    use FileSharePolicy::*;
    Some(match code {
        "blocked_file_type" => FileType,
        "slack_connect_blocked_file_type" => SlackConnectFileType,
        "slack_connect_file_link_sharing_blocked" => SlackConnectLink,
        "slack_connect_file_upload_sharing_blocked" => SlackConnectUpload,
        "posting_to_channel_denied" => PostingDenied,
        "restricted_action" => RestrictedAction,
        "user_is_external_guest" => ExternalGuest,
        "message_limit_exceeded" => MessageLimit,
        "max_file_sharing_exceeded" | "file_share_limit_reached" => ShareLimit,
        "file_not_found" | "file_deleted" => FileUnavailable,
        "channel_not_found" => DestinationUnavailable,
        _ => return None,
    })
}
fn timestamp(s: &str) -> bool {
    let Some((seconds, fraction)) = s.split_once('.') else {
        return false;
    };
    (1..=20).contains(&seconds.len())
        && fraction.len() == 6
        && seconds
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
}
fn bounded(value: &Value, limit: usize) -> bool {
    fn depth(value: &Value, left: usize) -> bool {
        if left == 0 {
            return false;
        }
        match value {
            Value::Array(v) => v.iter().all(|v| depth(v, left - 1)),
            Value::Object(v) => v.values().all(|v| depth(v, left - 1)),
            _ => true,
        }
    }
    struct Budget(usize);
    impl Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_sub(bytes.len())
                .ok_or_else(|| io::Error::other("JSON budget"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    depth(value, 64) && serde_json::to_writer(Budget(limit), value).is_ok()
}

#[cfg(test)]
mod tests;
