//! Private restart custody. Restoring a record never authorizes remote replay.
//! Audio bytes, Header siblings and device identities deliberately have no Debug.
use super::*;
use serde::{Deserialize, Serialize};
pub mod store;
#[cfg(test)]
mod tests;
pub const MAX_HEADER_BYTES: usize = 256 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    account: String,
    team: String,
    user: String,
}
impl Scope {
    pub fn new(account: &str, team: &str, user: &str) -> Result<Self, Error> {
        let account = uuid::Uuid::parse_str(account)
            .map_err(|_| invalid("Recording account identity is invalid."))?
            .to_string();
        if !identity(team) || !identity(user) {
            return Err(invalid("Recording workspace identity is invalid."));
        }
        Ok(Self {
            account,
            team: team.into(),
            user: user.into(),
        })
    }
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn team(&self) -> &str {
        &self.team
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    fn valid(&self) -> bool {
        Self::new(&self.account, &self.team, &self.user).is_ok_and(|s| s == *self)
    }
}
fn identity(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control)
}
#[derive(Clone, Serialize, Deserialize)]
struct Header {
    section: Value,
    desired: Option<String>,
}
impl Header {
    fn capture(form: &RecordingDraft) -> Self {
        Self {
            section: form.section.clone(),
            desired: form.desired.clone(),
        }
    }
    fn restore(&self, scope: &Scope) -> Result<RecordingDraft, Error> {
        // These identity-only values decode local bytes, not an API response.
        // All dispatch paths must revalidate against a fresh session-bound read.
        let identity = WorkspaceSession {
            team_id: scope.team.clone(),
            user_id: scope.user.clone(),
            enterprise_id: None,
            name: String::new(),
            url: String::new(),
            token: String::new(),
        };
        let raw = json!({"ok":true,"result":{"data":{"user":{"id":scope.user,"isSelf":true,"profileSections":[self.section.clone()]}}}});
        let snapshot = OwnProfileSections::from_response(&identity, &raw)?;
        let mut form = RecordingDraft::open(&snapshot)?;
        match &self.desired {
            Some(file) => form.replace(file)?,
            None => form.remove(),
        }
        Ok(form)
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct AudioInfo {
    mime: String,
    elapsed_ms: u32,
}
#[derive(Clone)]
pub struct Audio {
    info: AudioInfo,
    bytes: Vec<u8>,
}
impl Audio {
    pub fn new(bytes: Vec<u8>, mime: &str, elapsed_ms: u32) -> Result<Self, Error> {
        if bytes.is_empty()
            || bytes.len() > MAX_BYTES
            || elapsed_ms > MAX_DURATION_MS
            || !matches!(mime, "audio/mp4" | "audio/webm" | "audio/webm;codecs=opus")
        {
            return Err(invalid("Invalid retained recording audio."));
        }
        Ok(Self {
            info: AudioInfo {
                mime: mime.into(),
                elapsed_ms,
            },
            bytes,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn mime(&self) -> &str {
        &self.info.mime
    }
    pub fn elapsed_ms(&self) -> u32 {
        self.info.elapsed_ms
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    Upload,
    Save,
    Cleanup,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    Preparing,
    Ticket,
    Bytes,
    Complete,
    Saving,
    Deleting,
    Unknown,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Pending {
    id: String,
    operation: Operation,
    stage: Stage,
}
impl Pending {
    pub fn new(operation: Operation) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            operation,
            stage: Stage::Preparing,
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn stage(&self) -> Stage {
        self.stage
    }
    pub fn mark(&mut self, stage: Stage) -> Result<(), Error> {
        let allowed = match (self.operation, self.stage, stage) {
            (_, _, Stage::Unknown) => true,
            (Operation::Upload, Stage::Preparing, Stage::Ticket)
            | (Operation::Upload, Stage::Ticket, Stage::Bytes)
            | (Operation::Upload, Stage::Bytes, Stage::Complete)
            | (Operation::Save, Stage::Preparing, Stage::Saving)
            | (Operation::Cleanup, Stage::Preparing, Stage::Deleting) => true,
            _ => false,
        };
        if !allowed {
            return Err(invalid("Recording stage cannot be replayed."));
        }
        self.stage = stage;
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Body {
    scope: Scope,
    header: Header,
    audio: Option<AudioInfo>,
    device: String,
    staged_file: Option<String>,
    pending: Option<Pending>,
    retry_at: u64,
    #[serde(default)]
    cancel_requested: bool,
}
#[derive(Clone)]
pub struct Record {
    body: Body,
    audio: Option<Audio>,
}
impl Record {
    pub fn capture(scope: Scope, form: &RecordingDraft) -> Result<Self, Error> {
        if scope.team != form.baseline.team_id() || scope.user != form.baseline.user_id() {
            return Err(invalid("Recording draft owner changed."));
        }
        let record = Self {
            body: Body {
                scope,
                header: Header::capture(form),
                audio: None,
                device: String::new(),
                staged_file: None,
                pending: None,
                retry_at: 0,
                cancel_requested: false,
            },
            audio: None,
        };
        record.validate()?;
        Ok(record)
    }
    pub fn scope(&self) -> &Scope {
        &self.body.scope
    }
    pub fn form(&self) -> Result<RecordingDraft, Error> {
        self.body.header.restore(self.scope())
    }
    pub fn audio(&self) -> Option<&Audio> {
        self.audio.as_ref()
    }
    pub fn pending(&self) -> Option<&Pending> {
        self.body.pending.as_ref()
    }
    pub fn staged_file(&self) -> Option<&str> {
        self.body.staged_file.as_deref()
    }
    pub fn device(&self) -> &str {
        &self.body.device
    }
    pub fn retry_at(&self) -> u64 {
        self.body.retry_at
    }
    pub fn cancel_requested(&self) -> bool {
        self.body.cancel_requested
    }
    pub fn request_cancel(&mut self) -> Result<(), Error> {
        if !self
            .pending()
            .is_some_and(|p| p.operation == Operation::Upload)
        {
            return Err(invalid("No upload is available to cancel."));
        }
        self.body.cancel_requested = true;
        Ok(())
    }
    /// Only the live task that still knows no ticket request was dispatched may
    /// use this. Recovery has no such evidence and must retain Unknown custody.
    pub fn cancel_before_dispatch(&mut self) -> Result<(), Error> {
        if !self.cancel_requested()
            || self.staged_file().is_some()
            || !self.pending().is_some_and(|p| {
                p.operation == Operation::Upload
                    && matches!(p.stage, Stage::Preparing | Stage::Ticket)
            })
        {
            return Err(invalid("Recording dispatch is not known to be absent."));
        }
        self.body.pending = None;
        self.body.cancel_requested = false;
        Ok(())
    }
    /// A canceled upload may enter a fresh guarded cleanup operation. The
    /// caller must first let the original bounded request settle; this method
    /// never proves the file is unused or authorizes a delete by itself.
    pub fn prepare_cleanup(&mut self, now: u64) -> Result<(), Error> {
        if self.body.cancel_requested
            && self.staged_file().is_some()
            && self
                .pending()
                .is_some_and(|p| p.operation == Operation::Upload)
        {
            self.body.pending = None;
        }
        self.begin(Operation::Cleanup, now)
    }
    pub fn set_device(&mut self, device: String) -> Result<(), Error> {
        if device.len() > 1024 || device.chars().any(char::is_control) {
            return Err(invalid("Invalid microphone identity."));
        }
        self.body.device = device;
        Ok(())
    }
    pub fn set_audio(&mut self, audio: Audio) -> Result<(), Error> {
        if self.body.pending.is_some() || self.body.staged_file.is_some() {
            return Err(invalid("Resolve the retained recording operation first."));
        }
        self.body.audio = Some(audio.info.clone());
        self.audio = Some(audio);
        Ok(())
    }
    pub fn set_form(&mut self, form: &RecordingDraft) -> Result<(), Error> {
        if self.body.pending.is_some()
            || form.baseline.team_id() != self.scope().team
            || form.baseline.user_id() != self.scope().user
        {
            return Err(invalid("Recording draft is not available for replacement."));
        }
        self.body.header = Header::capture(form);
        Ok(())
    }
    pub fn begin(&mut self, operation: Operation, now: u64) -> Result<(), Error> {
        if self.body.pending.is_some()
            || self.body.retry_at > now
            || (self.body.cancel_requested && operation != Operation::Cleanup)
        {
            return Err(invalid("Recording operation is retained or cooling down."));
        }
        match operation {
            Operation::Upload if self.audio.is_none() || self.body.staged_file.is_some() => {
                return Err(invalid("No available local recording."));
            }
            Operation::Save if !self.form()?.dirty() => {
                return Err(invalid("No recording change to save."));
            }
            Operation::Cleanup if self.body.staged_file.is_none() => {
                return Err(invalid("No owned staged recording."));
            }
            _ => {}
        }
        self.body.pending = Some(Pending::new(operation));
        Ok(())
    }
    pub fn mark(&mut self, stage: Stage) -> Result<(), Error> {
        self.body
            .pending
            .as_mut()
            .ok_or_else(|| invalid("No retained recording operation."))?
            .mark(stage)
    }
    pub fn remember_ticket(&mut self, file: &str) -> Result<(), Error> {
        if !valid_file(file)
            || self.form()?.original_file() == Some(file)
            || !self
                .pending()
                .is_some_and(|p| p.operation == Operation::Upload && p.stage == Stage::Ticket)
        {
            return Err(invalid("Recording upload ownership is unavailable."));
        }
        self.body.staged_file = Some(file.into());
        Ok(())
    }
    pub fn defer(&mut self, until: u64) {
        self.body.retry_at = self.body.retry_at.max(until);
    }
    pub fn abort_preparation(&mut self) -> Result<(), Error> {
        if !self.pending().is_some_and(|p| p.stage == Stage::Preparing) {
            return Err(invalid(
                "A dispatched recording operation cannot be discarded.",
            ));
        }
        self.body.pending = None;
        self.body.cancel_requested = false;
        Ok(())
    }
    pub fn discard_local_changes(&mut self) -> Result<(), Error> {
        if self.pending().is_some() || self.staged_file().is_some() {
            return Err(invalid("Resolve the retained recording operation first."));
        }
        let mut form = self.form()?;
        form.desired = form.initial.clone();
        self.body.header = Header::capture(&form);
        self.body.audio = None;
        self.audio = None;
        Ok(())
    }
    pub fn confirm_upload(&mut self, file: &str) -> Result<(), Error> {
        if self.body.cancel_requested
            || self.staged_file() != Some(file)
            || !self.pending().is_some_and(|p| {
                p.operation == Operation::Upload
                    && matches!(p.stage, Stage::Complete | Stage::Unknown)
            })
        {
            return Err(invalid("No matching recording upload receipt."));
        }
        let mut form = self.form()?;
        form.replace(file)?;
        self.body.header = Header::capture(&form);
        self.body.pending = None;
        Ok(())
    }
    pub fn confirm_save(&mut self, fresh: &OwnProfileSections) -> Result<(), Error> {
        if !self
            .pending()
            .is_some_and(|p| p.operation == Operation::Save)
            || !self.form()?.confirms(fresh)
        {
            return Err(invalid("The recording save is not confirmed."));
        }
        let form = RecordingDraft::open(fresh)?;
        let adopted = self
            .staged_file()
            .is_some_and(|file| form.current_file() == Some(file));
        self.body.header = Header::capture(&form);
        self.body.pending = None;
        if adopted {
            self.body.staged_file = None;
        }
        if self.body.staged_file.is_none() {
            self.body.audio = None;
            self.audio = None;
        }
        Ok(())
    }
    /// Only an authoritative positive files.delete result or file_deleted read
    /// may be used by the caller. Generic not_found/access errors do not prove it.
    pub fn confirm_cleanup(&mut self, file: &str) -> Result<(), Error> {
        if self.staged_file() != Some(file)
            || !self
                .pending()
                .is_some_and(|p| p.operation == Operation::Cleanup)
        {
            return Err(invalid("No matching recording cleanup receipt."));
        }
        let mut form = self.form()?;
        form.desired = form.initial.clone();
        self.body.header = Header::capture(&form);
        self.body.pending = None;
        self.body.staged_file = None;
        self.body.cancel_requested = false;
        Ok(())
    }
    /// Cleanup is only for the exact ticket owned by this local upload record.
    /// Removing an existing Header reference never calls this method.
    pub fn cleanup_request(
        &self,
        c: &SlackClient,
        w: &WorkspaceSession,
        fresh: &OwnProfileSections,
        file: &Value,
    ) -> Result<PreparedRequest, Error> {
        let staged = self
            .staged_file()
            .ok_or_else(|| invalid("No owned staged recording."))?;
        let current = RecordingDraft::open(fresh)?;
        if !self
            .pending()
            .is_some_and(|p| p.operation == Operation::Cleanup && p.stage == Stage::Preparing)
            || w.team_id != self.scope().team
            || w.user_id != self.scope().user
            || fresh.team_id() != self.scope().team
            || fresh.user_id() != self.scope().user
            || current.current_file() == Some(staged)
            || file["id"] != staged
            || file["user"] != self.scope().user
            || file["subtype"] != SUBTYPE
            || file["mode"] != "hosted"
            || file["is_public"] != false
            || file["public_url_shared"] == true
            || ["channels", "groups", "ims"].iter().any(|key| {
                file.get(*key)
                    .is_some_and(|v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty))
            })
            || file.get("shares").is_some_and(|v| !empty_shares(v))
        {
            return Err(invalid(
                "The staged recording is attached, shared, or no longer owned by this upload.",
            ));
        }
        Ok(crate::slack::api::file_delete(c, w, staged))
    }
    /// Explicit user decision after a fresh owner-bound read; no remote action.
    /// A known unused upload stays owned for a later explicit cleanup.
    pub fn keep_current(&mut self, fresh: &OwnProfileSections) -> Result<(), Error> {
        if fresh.team_id() != self.scope().team || fresh.user_id() != self.scope().user {
            return Err(invalid("Recording owner changed."));
        }
        let form = RecordingDraft::open(fresh)?;
        if form.current_file() == self.staged_file() {
            self.body.staged_file = None;
        }
        self.body.header = Header::capture(&form);
        self.body.pending = None;
        self.body.cancel_requested = false;
        Ok(())
    }
    /// A resumed operation is always uncertain, including a preparing record.
    /// Opening storage never continues its request sequence.
    pub fn recovered(&self) -> Self {
        let mut r = self.clone();
        if let Some(p) = r.body.pending.as_mut() {
            p.stage = Stage::Unknown;
        }
        r
    }
    fn validate(&self) -> Result<(), Error> {
        if !self.scope().valid()
            || self.body.device.len() > 1024
            || self.body.device.chars().any(char::is_control)
            || self.body.retry_at > i64::MAX as u64
        {
            return Err(invalid("Invalid recording custody."));
        }
        let form = self.form()?;
        if self
            .body
            .staged_file
            .as_deref()
            .is_some_and(|s| !valid_file(s) || form.original_file() == Some(s))
        {
            return Err(invalid("Invalid staged recording ownership."));
        }
        if let Some(p) = self.pending() {
            if (p.operation == Operation::Upload && self.audio.is_none())
                || (p.operation == Operation::Cleanup && self.staged_file().is_none())
                || (p.operation == Operation::Save && !form.dirty())
                || (matches!(p.stage, Stage::Bytes | Stage::Complete)
                    && self.staged_file().is_none())
            {
                return Err(invalid("Recording operation custody is incomplete."));
            }
            if uuid::Uuid::parse_str(&p.id).is_err()
                || !matches!(
                    (p.operation, p.stage),
                    (
                        Operation::Upload,
                        Stage::Preparing
                            | Stage::Ticket
                            | Stage::Bytes
                            | Stage::Complete
                            | Stage::Unknown
                    ) | (
                        Operation::Save,
                        Stage::Preparing | Stage::Saving | Stage::Unknown
                    ) | (
                        Operation::Cleanup,
                        Stage::Preparing | Stage::Deleting | Stage::Unknown
                    )
                )
            {
                return Err(invalid("Invalid recording operation custody."));
            }
        }
        match (&self.body.audio, &self.audio) {
            (None, None) => {}
            (Some(info), Some(audio))
                if info.mime == audio.info.mime && info.elapsed_ms == audio.info.elapsed_ms =>
            {
                Audio::new(audio.bytes.clone(), &info.mime, info.elapsed_ms)?;
            }
            _ => return Err(invalid("Recording audio custody is incomplete.")),
        }
        let body =
            serde_json::to_vec(&self.body).map_err(|_| invalid("Invalid recording custody."))?;
        if body.len() > MAX_HEADER_BYTES {
            return Err(invalid("Recording Header exceeds local custody limit."));
        }
        Ok(())
    }
}
fn empty_shares(value: &Value) -> bool {
    value.is_null()
        || value.as_object().is_some_and(|groups| {
            groups
                .values()
                .all(|v| v.as_object().is_some_and(serde_json::Map::is_empty))
        })
}
