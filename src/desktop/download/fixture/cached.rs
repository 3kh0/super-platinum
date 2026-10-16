//! Exact offline cached Save capability. No destination or outcome is synthesized.
use super::*;
use crate::file_preview::refresh;
use serde_json::{Value, json};
use std::time::Duration;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Error,
};
mod check;
mod ledger;
mod paths;
#[cfg(test)]
mod tests;
use ledger::Ledger;
pub(crate) const FILE: &str = "F_REFRESH";
pub(crate) const ALIAS: &str = "cached-viewer-save-check";
pub(super) const OLD: &str = "Cached original A\nSafe overwrite fixture.\n";
pub(super) const NEW: &str = "Cached original B\nSafe overwrite fixture.\n";
const URL: &str = "https://files.slack.com/refresh-fixture";
static CAP: OnceLock<Arc<Capability>> = OnceLock::new();
struct Capability {
    root: PathBuf,
    nonce: String,
    directory: paths::Directory,
    owner: owner::Owner,
    old: MediaAssetId,
    bytes: Arc<[u8]>,
    cancel: Arc<AtomicBool>,
    ledger: Mutex<Ledger>,
    release: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}
pub(crate) fn enabled() -> bool {
    cfg!(target_os = "windows")
        && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok(ALIAS)
        && std::env::var("SUPER_PLATINUM_CACHED_SAVE_CHECK").as_deref() == Ok("1")
}
fn refused() -> Error {
    Error::Api("cached_save_fixture_refused".into())
}
fn current(shell: &ShellState) -> Result<&'static Arc<Capability>, String> {
    let cap = CAP.get().ok_or("Cached capability not installed")?;
    if !enabled() || !cap.owner.current(shell) {
        return Err("Cached capability retired".into());
    }
    Ok(cap)
}
pub(crate) fn start(state: Signal<ShellState>) {
    if enabled() {
        dioxus::core::spawn_forever(async move {
            check::run(state).await;
        });
    }
}
pub(in crate::download) struct Input {
    cap: Arc<Capability>,
    attempt: usize,
}
pub(in crate::download) fn capture(
    shell: &ShellState,
    owner: &owner::Owner,
    bytes: Option<&Arc<[u8]>>,
    canceled: &Arc<AtomicBool>,
) -> Result<Option<Input>, String> {
    if !enabled() {
        return Ok(None);
    }
    let cap = current(shell)?;
    if !owner.current(shell)
        || !bytes.is_some_and(|b| Arc::ptr_eq(b, &cap.bytes))
        || !Arc::ptr_eq(canceled, &cap.cancel)
        || cap.cancel.load(Ordering::Acquire)
        || shell.viewer.as_ref().is_none_or(|v| v.id != cap.old)
    {
        return Err("Wrong captured cached owner/bytes/latch".into());
    }
    let attempt = cap
        .ledger
        .lock()
        .map_err(|_| "Cached ledger lock")?
        .begin()?;
    let input = Input {
        cap: cap.clone(),
        attempt,
    };
    Ok(Some(input))
}
impl Input {
    pub async fn requested(&self) {
        self.emit(
            "requested",
            json!({"actualRfd":true,"cached":true,"mode":"ConfirmedCachedOverwrite"}),
        )
        .await;
    }
    pub fn directory(&self) -> PathBuf {
        self.cap.root.join("downloads")
    }
    async fn emit(&self, kind: &str, fields: Value) {
        let mut value = fields;
        value["identity"] = self.cap.identity();
        value["attempt"] = json!(self.attempt);
        emit(kind, value).await;
    }
    pub async fn returned(&self, path: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
        let cap = self.cap.clone();
        let actual = path.clone();
        let attempt = self.attempt;
        let kind = tokio::task::spawn_blocking(move || match actual {
            None => Ok("none"),
            Some(p) => cap.directory.validate(&p, attempt),
        })
        .await
        .map_err(|_| "Cached path worker")??;
        self.cap
            .ledger
            .lock()
            .map_err(|_| "Cached ledger")?
            .returned(self.attempt, kind)?;
        self.emit(
            "returned",
            json!({"kind":kind,"actualRfd":true,"path":path,"accepted":true}),
        )
        .await;
        Ok(path)
    }
    pub async fn before_current(&self) -> Result<(), String> {
        if self.attempt != 3 {
            return Ok(());
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut slot = self.cap.release.lock().map_err(|_| "Cached release lock")?;
            if slot.is_some() {
                return Err("Repeated cached hold".into());
            }
            self.cap
                .ledger
                .lock()
                .map_err(|_| "Cached ledger")?
                .hold(self.attempt)?;
            *slot = Some(tx);
        }
        self.emit(
            "held",
            json!({"actualReturnedPath":true,"beforeCurrentCheck":true}),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(120), rx)
            .await
            .map_err(|_| "Cached return hold expired")?
            .map_err(|_| "Cached return hold canceled")?;
        Ok(())
    }
    pub async fn transfer(&self) -> Result<(), String> {
        self.cap
            .ledger
            .lock()
            .map_err(|_| "Cached ledger")?
            .transfer(self.attempt)?;
        self.emit("transfer", json!({"cached":true})).await;
        Ok(())
    }
    pub async fn retired(&self, canceled: bool) {
        let result = self
            .cap
            .ledger
            .lock()
            .map_err(|_| "Cached ledger".to_owned())
            .and_then(|mut l| l.retired(self.attempt, canceled));
        self.emit(
            "outcome",
            json!({"kind":"retired-before-transfer","ok":result.is_ok(),"canceled":canceled}),
        )
        .await;
    }
    pub async fn completed(&self, result: &Result<(), DownloadError>) {
        let saved = result.is_ok();
        let accepted = self
            .cap
            .ledger
            .lock()
            .ok()
            .is_some_and(|mut l| l.saved(self.attempt, saved).is_ok());
        self.emit(
            "outcome",
            json!({"kind":if saved{"saved"}else{"unexpected-error"},"ok":accepted}),
        )
        .await;
    }
}
impl Capability {
    fn identity(&self) -> Value {
        json!({"version":1,"nonce":self.nonce,"fixture":ALIAS,"pid":std::process::id()})
    }
    fn snapshot(&self) -> Result<Value, String> {
        Ok(
            serde_json::to_value(&*self.ledger.lock().map_err(|_| "Cached ledger")?)
                .map_err(|_| "Cached ledger JSON")?,
        )
    }
    fn release(&self, shell: &ShellState) -> Result<(), String> {
        current(shell)?;
        if !self.cancel.load(Ordering::Acquire)
            || shell.viewer.as_ref().is_none_or(|v| v.id == self.old)
            || shell
                .viewer
                .as_ref()
                .and_then(|v| shell.media.ready_download_bytes(&v.id))
                .as_deref()
                != Some(NEW.as_bytes())
        {
            return Err("Actual successful refresh must retire old save first".into());
        }
        let mut slot = self.release.lock().map_err(|_| "Cached release")?;
        self.ledger.lock().map_err(|_| "Cached ledger")?.release()?;
        slot.take()
            .ok_or("Missing held save")?
            .send(())
            .map_err(|_| "Held save ended".into())
    }
}
pub(crate) fn metadata(shell: &ShellState) -> Result<File, Error> {
    let cap = current(shell).map_err(|_| refused())?;
    cap.ledger
        .lock()
        .map_err(|_| refused())?
        .metadata()
        .map_err(|_| refused())?;
    Ok(File {
        id: Some(FILE.into()),
        name: Some("release-notes.txt".into()),
        title: Some("Release notes".into()),
        mimetype: Some("text/plain".into()),
        filetype: Some("text".into()),
        is_external: Some(false),
        size: Some(NEW.len() as u64),
        url_private: Some(URL.into()),
        extra: std::collections::BTreeMap::from([("mode".into(), json!("hosted"))]),
        ..Default::default()
    })
}
pub(crate) async fn body(state: Signal<ShellState>, url: &str) -> Result<Vec<u8>, Error> {
    let cap = current(&state.peek()).map_err(|_| refused())?;
    if url != URL {
        return Err(refused());
    }
    cap.ledger
        .lock()
        .map_err(|_| refused())?
        .body()
        .map_err(|_| refused())?;
    Ok(NEW.as_bytes().to_vec())
}
async fn install(mut state: Signal<ShellState>) -> Result<Arc<Capability>, String> {
    if !enabled() {
        return Err("Exact cached gate required".into());
    }
    let root =
        PathBuf::from(std::env::var("SUPER_PLATINUM_CACHED_SAVE_ROOT").map_err(|_| "Cached root")?);
    let nonce = std::env::var("SUPER_PLATINUM_CACHED_SAVE_NONCE").map_err(|_| "Cached nonce")?;
    let r = root.clone();
    let n = nonce.clone();
    let directory = tokio::task::spawn_blocking(move || paths::Directory::capture(r, n))
        .await
        .map_err(|_| "Cached directory worker")??;
    let mut shell = state.write();
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_account.is_some()
        || shell.core.active_team.as_deref() != Some("T1")
    {
        return Err("Disconnected cached seed required".into());
    }
    refresh::fixture::install(&mut shell, "file-preview-refresh");
    shell.core.active_account = Some("cached-save-fixture".into());
    shell.core.session = Some(Session {
        d_cookie: String::new(),
        workspaces: std::collections::BTreeMap::from([(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U0".into(),
                enterprise_id: None,
                name: "Cached Save".into(),
                url: "https://fixture.invalid".into(),
                token: String::new(),
            },
        )]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("")));
    let old = shell.viewer.as_ref().ok_or("Cached viewer")?.id.clone();
    shell
        .media
        .insert(old.clone(), "text/plain", OLD.as_bytes());
    refresh::opened(&mut shell);
    let cancel = refresh::lease(&shell, &old)
        .ok_or("Cached lease")?
        .cancellation();
    let (owner, bytes) = owner::Owner::capture(&shell, old.clone()).ok_or("Cached owner")?;
    let cap = Arc::new(Capability {
        root,
        nonce,
        directory,
        owner,
        old,
        bytes: bytes.ok_or("Cached bytes")?,
        cancel,
        ledger: Mutex::new(Ledger::default()),
        release: Mutex::new(None),
    });
    CAP.set(cap.clone())
        .map_err(|_| "Repeated cached capability")?;
    Ok(cap)
}

async fn emit(kind: &str, value: Value) {
    let kind = kind.to_owned();
    let _ = tokio::task::spawn_blocking(move || eprintln!("cached-save-{kind}: {value}")).await;
}
