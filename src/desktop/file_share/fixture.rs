//! Private disconnected fixture capability. Not wired into live admission.
//! Load once during exact fixture setup; never call filesystem validation in render.
use crate::state::ShellState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use super_platinum_core::{
    config::WorkspaceSession,
    file_sharing::{Intent, Receipt, State},
    slack::{
        Error, PreparedRequest, SlackClient, api::file_share::existing_file_share,
        client::RequestBody,
    },
};
use tokio::sync::oneshot;

pub(super) mod reopen;
mod request;

pub(super) const ACCOUNT: &str = "89fce7ae-a2c9-4c74-919b-4176535d5d70";
pub(super) const TEAM: &str = "T00000001";
pub(super) const USER: &str = "U00000001";
pub(super) const FILE: &str = "F00000001";
pub(super) const DESTINATION: &str = "C00000001";
const ORIGIN: &str = "https://file-share-fixture.invalid";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Case {
    Accepted,
    DlpWarning,
    Processing,
    Unconfirmed,
    StorageFailure,
    SourceStale,
}
impl Case {
    fn parse(value: &str) -> Result<Self, Rejected> {
        serde_json::from_value(Value::String(value.into())).map_err(|_| Rejected::Gate)
    }
    fn response(self) -> Result<Value, Error> {
        match self {
            Self::Accepted | Self::StorageFailure => {
                Ok(json!({"ok":true,"file_msg_ts":"1719800400.000001"}))
            }
            Self::DlpWarning => Ok(
                json!({"ok":false,"error":"dlp_user_warning","dlp_custom_message":"Review <script>fixture</script> & 日本語. This synthetic file needs review in Slack before another share."}),
            ),
            Self::Processing => Ok(json!({"ok":false,"error":"dlp_processing"})),
            Self::Unconfirmed => Err(Error::Transport(
                "synthetic response lost after prepared dispatch".into(),
            )),
            Self::SourceStale => Err(Error::Transport(
                "source-stale case must never dispatch".into(),
            )),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rejected {
    Gate,
    Path,
    Owner,
    Read,
    Request,
    Replay,
    Poisoned,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema: u8,
    kind: String,
    nonce: String,
    fixture: String,
    case: Case,
}
struct Inputs {
    fixture_mode: bool,
    fixture: String,
    enabled: bool,
    case: Case,
    nonce: String,
    ack: PathBuf,
    database: PathBuf,
    saved_database: PathBuf,
}
impl Inputs {
    fn environment() -> Result<Self, Rejected> {
        let text = |key| std::env::var(key).map_err(|_| Rejected::Gate);
        let path = |key| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .ok_or(Rejected::Gate)
        };
        Ok(Self {
            fixture_mode: crate::fixture::is_fixture(),
            fixture: text("SUPER_PLATINUM_FIXTURE")?,
            enabled: text("SUPER_PLATINUM_FILE_SHARE_CHECK")? == "1",
            case: Case::parse(&text("SUPER_PLATINUM_FILE_SHARE_CASE")?)?,
            nonce: text("SUPER_PLATINUM_FILE_SHARE_NONCE")?,
            ack: path("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")?,
            database: path("SUPER_PLATINUM_FILE_SHARE_JOURNAL")?,
            saved_database: path("SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB")?,
        })
    }
    fn gate(&self) -> Result<(), Rejected> {
        let nonce = self.nonce.as_bytes();
        let nonce_ok = nonce.len() == 36
            && nonce.iter().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    *c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            });
        if !self.fixture_mode
            || !self.enabled
            || !nonce_ok
            || !matches!(
                self.fixture.as_str(),
                "file-share"
                    | "file-share-narrow"
                    | "file-share-pressure"
                    | "file-share-pressure-narrow"
            )
            || (self.fixture.starts_with("file-share-pressure") && self.case != Case::Accepted)
        {
            return Err(Rejected::Gate);
        }
        Ok(())
    }
}
struct Paths {
    ack: PathBuf,
    database: PathBuf,
    saved_database: PathBuf,
}
impl Paths {
    fn load(inputs: &Inputs) -> Result<Self, Rejected> {
        inputs.gate()?;
        if !inputs.ack.is_absolute()
            || inputs.database != inputs.ack.join("journal/file-share.sqlite")
            || inputs.saved_database != inputs.ack.join("saved-file-recovery.sqlite")
        {
            return Err(Rejected::Path);
        }
        let plain_directory = |path: &Path| {
            std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !redirected(&m))
        };
        // Startup validation only; the runner retains custody of this fresh
        // private hierarchy. Arc identity is not retained filesystem identity.
        if inputs.ack.ancestors().any(|path| !plain_directory(path))
            || !plain_directory(&inputs.ack.join("journal"))
        {
            return Err(Rejected::Path);
        }
        // SQLite can consult sidecars even when the main file is absent.
        // Reject all pre-existing names, including dangling links.
        for path in [&inputs.database, &inputs.saved_database] {
            for suffix in ["", "-wal", "-shm", "-journal"] {
                let mut name = path.as_os_str().to_os_string();
                name.push(suffix);
                match std::fs::symlink_metadata(PathBuf::from(name)) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    _ => return Err(Rejected::Path),
                }
            }
        }
        let marker_path = inputs.ack.join("file-share-fixture.json");
        let metadata = std::fs::symlink_metadata(&marker_path).map_err(|_| Rejected::Path)?;
        if !metadata.is_file() || redirected(&metadata) || metadata.len() > 2048 {
            return Err(Rejected::Path);
        }
        let mut bytes = Vec::new();
        std::fs::File::open(marker_path)
            .map_err(|_| Rejected::Path)?
            .take(2049)
            .read_to_end(&mut bytes)
            .map_err(|_| Rejected::Path)?;
        if bytes.len() > 2048 {
            return Err(Rejected::Path);
        }
        let marker: Marker = serde_json::from_slice(&bytes).map_err(|_| Rejected::Path)?;
        if marker.schema != 1
            || marker.kind != "file-share-fixture"
            || marker.nonce != inputs.nonce
            || marker.fixture != inputs.fixture
            || marker.case != inputs.case
        {
            return Err(Rejected::Gate);
        }
        let ack = std::fs::canonicalize(&inputs.ack).map_err(|_| Rejected::Path)?;
        let journal =
            std::fs::canonicalize(inputs.ack.join("journal")).map_err(|_| Rejected::Path)?;
        if journal.parent() != Some(ack.as_path()) {
            return Err(Rejected::Path);
        }
        Ok(Self {
            database: journal.join("file-share.sqlite"),
            saved_database: ack.join("saved-file-recovery.sqlite"),
            ack,
        })
    }
}

struct Inner {
    paths: Paths,
    restart: Option<reopen::Seal>,
    case: Case,
    epoch: u64,
    generation: u64,
    ledger: Mutex<Ledger>,
}
#[derive(Default)]
struct Ledger {
    reads: Vec<&'static str>,
    attempt: Option<String>,
    fields: Option<Vec<(String, String)>>,
    response: Option<oneshot::Sender<Result<Value, Error>>>,
    released: bool,
    begun: Option<Receipt>,
    begin_release: Option<oneshot::Sender<()>>,
}
/// No public constructor and no Debug; arbitrary environment flags alone never
/// create admission. Parent state must retain this exact allocation explicitly.
#[derive(Clone)]
pub(crate) struct Capability(Arc<Inner>);
#[derive(Serialize)]
pub(super) struct Evidence {
    pub case: Case,
    pub reads: Vec<&'static str>,
    pub dispatch_count: usize,
    pub attempt: Option<String>,
    pub fields: Option<Vec<(String, String)>>,
    pub awaiting_release: bool,
    pub released: bool,
    pub begin_attempt: Option<String>,
    pub awaiting_begin_release: bool,
}
impl Capability {
    /// Fixture startup only, after synthetic canonical shell seeding. No Session
    /// or Transport is manufactured, loaded, accepted or installed here.
    pub(super) fn load(shell: &ShellState) -> Result<Self, Rejected> {
        let inputs = Inputs::environment()?;
        inputs.gate()?;
        let workspace = shell.core.workspaces.get(TEAM).ok_or(Rejected::Owner)?;
        if !disconnected_owner(shell) {
            return Err(Rejected::Owner);
        }
        Ok(Self(Arc::new(Inner {
            paths: Paths::load(&inputs)?,
            restart: None,
            case: inputs.case,
            epoch: shell.core.account_epoch,
            generation: workspace.rt_generation,
            ledger: Mutex::new(Ledger::default()),
        })))
    }
    /// Tests provide a fresh owned directory and the same validated marker;
    /// no process environment, real storage or credential backend is consulted.
    #[cfg(test)]
    pub(super) fn for_test(
        ack: PathBuf,
        case: Case,
        epoch: u64,
        generation: u64,
    ) -> Result<Self, Rejected> {
        let inputs = Inputs {
            fixture_mode: true,
            fixture: "file-share".into(),
            enabled: true,
            case,
            nonce: ACCOUNT.into(),
            database: ack.join("journal/file-share.sqlite"),
            saved_database: ack.join("saved-file-recovery.sqlite"),
            ack,
        };
        Ok(Self(Arc::new(Inner {
            paths: Paths::load(&inputs)?,
            restart: None,
            case,
            epoch,
            generation,
            ledger: Mutex::new(Ledger::default()),
        })))
    }
    /// Check the exact currently installed capability. This is pure; no IO.
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell
            .file_share_fixture
            .as_ref()
            .is_some_and(|installed| self.same_identity(installed))
            && disconnected_owner(shell)
            && shell.core.account_epoch == self.0.epoch
            && shell
                .core
                .workspaces
                .get(TEAM)
                .is_some_and(|w| w.rt_generation == self.0.generation)
    }
    pub(super) fn key(&self) -> super::admission::Key {
        super::admission::Key {
            account: ACCOUNT.into(),
            team: TEAM.into(),
            user: USER.into(),
        }
    }
    pub(super) fn epoch(&self) -> u64 {
        self.0.epoch
    }
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(super) fn storage_path(&self) -> &Path {
        self.database()
    }
    pub(super) fn builder_session(&self) -> WorkspaceSession {
        WorkspaceSession {
            team_id: TEAM.into(),
            user_id: USER.into(),
            enterprise_id: None,
            name: "Share fixture".into(),
            url: ORIGIN.into(),
            token: String::new(),
        }
    }
    pub(super) async fn file_info(&self, id: &str) -> Result<Value, Error> {
        self.source_info(id).map_err(rejected)
    }
    pub(super) async fn channel_info(&self, id: &str) -> Result<Value, Error> {
        self.destination_info(id).map_err(rejected)
    }
    pub(super) async fn share(
        &self,
        receipt: &Receipt,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        let receive = self.record_prepared(receipt, request).map_err(rejected)?;
        tokio::time::timeout(std::time::Duration::from_secs(45), receive)
            .await
            .map_err(|_| rejected(Rejected::Replay))?
            .map_err(|_| rejected(Rejected::Replay))?
    }
    /// Only the common worker calls this after durable Begin and lease admission.
    /// The SourceStale diagnostic pauses here; the ordinary final fence decides
    /// whether the captured source is still eligible after the pause.
    pub(super) async fn after_begin(&self, receipt: &Receipt) -> Result<(), Error> {
        let receive = self.record_begin(receipt).map_err(rejected)?;
        if let Some(receive) = receive {
            tokio::time::timeout(std::time::Duration::from_secs(45), receive)
                .await
                .map_err(|_| rejected(Rejected::Replay))?
                .map_err(|_| rejected(Rejected::Replay))?;
        }
        Ok(())
    }
    fn record_begin(&self, receipt: &Receipt) -> Result<Option<oneshot::Receiver<()>>, Rejected> {
        if self.0.restart.is_some() || !valid_receipt(receipt) {
            return Err(Rejected::Request);
        }
        let mut ledger = self.0.ledger.lock().map_err(|_| Rejected::Poisoned)?;
        if ledger.begun.is_some() || ledger.attempt.is_some() {
            return Err(Rejected::Replay);
        }
        ledger.begun = Some(receipt.clone());
        if self.case() == Case::SourceStale {
            let (sender, receiver) = oneshot::channel();
            ledger.begin_release = Some(sender);
            Ok(Some(receiver))
        } else {
            Ok(None)
        }
    }
    pub(super) fn release_begin(&self, attempt: &str) -> Result<(), Rejected> {
        let mut ledger = self.0.ledger.lock().map_err(|_| Rejected::Poisoned)?;
        if self.case() != Case::SourceStale
            || ledger.begun.as_ref().map(|r| r.attempt_id.as_str()) != Some(attempt)
        {
            return Err(Rejected::Replay);
        }
        ledger
            .begin_release
            .take()
            .ok_or(Rejected::Replay)?
            .send(())
            .map_err(|_| Rejected::Replay)
    }
    pub(super) fn case(&self) -> Case {
        self.0.case
    }
    pub(super) fn database(&self) -> &Path {
        &self.0.paths.database
    }
    pub(super) fn ack_directory(&self) -> &Path {
        &self.0.paths.ack
    }
    pub(super) fn saved_database(&self) -> &Path {
        &self.0.paths.saved_database
    }
    pub(super) fn restart(&self) -> Option<&reopen::Seal> {
        self.0.restart.as_ref()
    }
    fn read(&self, kind: &'static str) -> Result<(), Rejected> {
        if self.0.restart.is_some() {
            return Err(Rejected::Read);
        }
        let mut ledger = self.0.ledger.lock().map_err(|_| Rejected::Poisoned)?;
        if ledger.reads.len() >= 8 || ledger.attempt.is_some() {
            return Err(Rejected::Read);
        }
        ledger.reads.push(kind);
        Ok(())
    }
    pub(super) fn source_info(&self, id: &str) -> Result<Value, Rejected> {
        if id != FILE {
            return Err(Rejected::Read);
        }
        self.read("files.info")?;
        Ok(json!({"ok":true,"file":source_file()}))
    }
    pub(super) fn destination_info(&self, id: &str) -> Result<Value, Rejected> {
        if id != DESTINATION {
            return Err(Rejected::Read);
        }
        self.read("conversations.info")?;
        Ok(json!({"ok":true,"channel":destination()}))
    }
    /// Transient zero-credential builder input, never ShellState.session.
    /// The reserved .invalid origin cannot enter the real mutation allowlist.
    pub(super) fn prepare(&self, intent: &Intent) -> Result<PreparedRequest, Rejected> {
        let intent = intent.request().map_err(|_| Rejected::Request)?;
        if intent.file_id() != FILE || intent.destination() != DESTINATION {
            return Err(Rejected::Request);
        }
        let input = self.builder_session();
        Ok(existing_file_share(
            &SlackClient::default(),
            &input,
            &intent,
        ))
    }
    /// Called only by the common worker after durable Begin and the unchanged
    /// final fences. Holds the injected result until checker captures Sending.
    pub(super) fn record_prepared(
        &self,
        receipt: &Receipt,
        request: PreparedRequest,
    ) -> Result<oneshot::Receiver<Result<Value, Error>>, Rejected> {
        if self.0.restart.is_some()
            || self.0.case == Case::SourceStale
            || !valid_receipt(receipt)
            || !request::matches(&request, &self.prepare(&receipt.intent)?)
        {
            return Err(Rejected::Request);
        }
        let RequestBody::Form(fields) = &request.body else {
            return Err(Rejected::Request);
        };
        let names: Vec<_> = fields.iter().map(|(k, _)| k.as_str()).collect();
        let expected = if receipt.intent.note().is_some() {
            vec![
                "token",
                "files",
                "channel",
                "blocks",
                "resharing_aware",
                "skip_dlp_user_warning",
                "from_share_modal",
            ]
        } else {
            vec![
                "token",
                "files",
                "channel",
                "resharing_aware",
                "skip_dlp_user_warning",
                "from_share_modal",
            ]
        };
        if names != expected
            || fields[0].1 != ""
            || request.method != "POST"
            || !request
                .url
                .starts_with(&format!("{ORIGIN}/api/files.share?"))
        {
            return Err(Rejected::Request);
        }
        let mut ledger = self.0.ledger.lock().map_err(|_| Rejected::Poisoned)?;
        if ledger.attempt.is_some() {
            return Err(Rejected::Replay);
        }
        if ledger.begun.as_ref() != Some(receipt) {
            return Err(Rejected::Request);
        }
        if ledger.reads != ["files.info", "conversations.info"] {
            return Err(Rejected::Read);
        }
        let (sender, receiver) = oneshot::channel();
        ledger.attempt = Some(receipt.attempt_id.clone());
        ledger.fields = Some(fields.clone());
        ledger.response = Some(sender);
        Ok(receiver)
    }
    /// Fixture checker releases only the exact ledger attempt after Sending PNG
    /// ACK (and after a real private SQLite failure trigger for StorageFailure).
    pub(super) fn release(&self, attempt: &str) -> Result<(), Rejected> {
        let mut ledger = self.0.ledger.lock().map_err(|_| Rejected::Poisoned)?;
        if ledger.attempt.as_deref() != Some(attempt) || ledger.released {
            return Err(Rejected::Replay);
        }
        let sender = ledger.response.take().ok_or(Rejected::Replay)?;
        ledger.released = true;
        sender
            .send(self.0.case.response())
            .map_err(|_| Rejected::Replay)
    }
    pub(super) fn evidence(&self) -> Result<Evidence, Rejected> {
        let ledger = self.0.ledger.lock().map_err(|_| Rejected::Poisoned)?;
        Ok(Evidence {
            case: self.0.case,
            reads: ledger.reads.clone(),
            dispatch_count: usize::from(ledger.attempt.is_some()),
            attempt: ledger.attempt.clone(),
            fields: ledger.fields.clone(),
            awaiting_release: ledger.response.is_some(),
            released: ledger.released,
            begin_attempt: ledger.begun.as_ref().map(|r| r.attempt_id.clone()),
            awaiting_begin_release: ledger.begin_release.is_some(),
        })
    }
}
fn redirected(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT includes junctions and other providers,
        // not only the symlinks represented by FileType::is_symlink().
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn rejected(_error: Rejected) -> Error {
    Error::Transport("disconnected file Share fixture rejected an operation".into())
}
fn valid_receipt(receipt: &Receipt) -> bool {
    receipt.state() == State::Unconfirmed
        && receipt.scope.team == TEAM
        && receipt.scope.user == USER
        && receipt.scope.file == FILE
        && receipt.scope.destination == DESTINATION
}
fn disconnected_owner(shell: &ShellState) -> bool {
    shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.core.active_team.as_deref() == Some(TEAM)
        && shell.self_account.user_id == USER
        && shell
            .core
            .workspaces
            .get(TEAM)
            .is_some_and(|w| w.team_id == TEAM && w.self_user_id == USER)
}
pub(super) fn source_file() -> Value {
    json!({"id":FILE,"title":"Fixture release notes 日本語","name":"fixture-notes.txt","user":"U00000002",
        "mode":"hosted","is_external":false,"filetype":"text","mimetype":"text/plain","size":42,
        "url_private":"https://file-share-fixture.invalid/source","permalink":"https://fixture.slack.com/files/U00000002/F00000001"})
}
pub(super) fn destination() -> Value {
    json!({"id":DESTINATION,"name":"share-fixture","is_channel":true,"is_member":true,
        "is_ext_shared":false,"is_read_only":false,"is_thread_only":false})
}
#[cfg(test)]
mod tests;
