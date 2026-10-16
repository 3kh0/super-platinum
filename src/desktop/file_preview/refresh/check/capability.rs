//! Named synthetic read boundary; the normal parser/decoder/publication stay shared.
use super::*;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::config::{Session, WorkspaceSession};
use super_platinum_core::slack::Transport;
static CAP: OnceLock<Capability> = OnceLock::new();
pub(super) const FILE: &str = "F_REFRESH";
pub(super) const URL: &str = "https://files.slack.com/refresh-fixture";
pub(super) const OLD: &str =
    "Release notes\nKeep this text selected while refreshing.\nOriginal snapshot A.\n";
pub(super) const NEW: &str =
    "Release notes\nKeep this text selected while refreshing.\nOriginal snapshot B.\n";
pub(super) struct Capability {
    scope: source::Scope,
    registry: MediaRegistry,
    pub old: MediaAssetId,
    pub cancel: Arc<AtomicBool>,
    pub old_bytes: Arc<[u8]>,
    ledger: Mutex<Ledger>,
}
#[derive(Default)]
struct Ledger {
    metadata: Vec<u64>,
    bodies: Vec<u64>,
    releases: Vec<(u64, &'static str)>,
    pending: Option<(u64, tokio::sync::oneshot::Sender<Result<Vec<u8>, Error>>)>,
}
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_FILE_REFRESH_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("file-preview-refresh-check" | "file-preview-refresh-check-narrow")
        )
}
fn file() -> File {
    File {
        id: Some(FILE.into()),
        title: Some("Release notes".into()),
        name: Some("release-notes.txt".into()),
        mimetype: Some("text/plain".into()),
        filetype: Some("text".into()),
        is_external: Some(false),
        size: Some(OLD.len() as u64),
        url_private: Some(URL.into()),
        extra: std::collections::BTreeMap::from([("mode".into(), json!("hosted"))]),
        ..Default::default()
    }
}
pub(crate) fn install(shell: &mut ShellState) -> Result<(), String> {
    if !enabled() || shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Exact disconnected refresh seed required".into());
    }
    // No disk/network here: startup only parses the private runner address.
    harness::configuration()?;
    super::super::fixture::install(shell, "file-preview-refresh");
    shell.core.active_account = Some("refresh-native-fixture".into());
    shell.core.session = Some(Session {
        d_cookie: String::new(),
        workspaces: std::collections::BTreeMap::from([(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U0".into(),
                enterprise_id: None,
                name: "Refresh fixture".into(),
                url: "https://fixture.invalid".into(),
                token: String::new(),
            },
        )]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("")));
    let old = shell
        .viewer
        .as_ref()
        .ok_or("Missing seeded viewer")?
        .id
        .clone();
    shell
        .media
        .insert(old.clone(), "text/plain", OLD.as_bytes());
    opened(shell);
    let old_lease = lease(shell, &old).ok_or("Missing fixture lease")?;
    let cap = Capability {
        scope: source::Scope::capture(shell).ok_or("Missing fixture scope")?,
        registry: shell.media.clone(),
        old: old.clone(),
        cancel: old_lease.cancellation(),
        old_bytes: shell
            .media
            .ready_bytes(&old)
            .ok_or("Missing seeded bytes")?,
        ledger: Mutex::new(Ledger::default()),
    };
    CAP.set(cap)
        .map_err(|_| "Repeated refresh capability".into())
}
pub(super) fn get(shell: &ShellState) -> Result<&'static Capability, String> {
    let cap = CAP.get().ok_or("Missing refresh capability")?;
    if !enabled()
        || !cap.scope.current(shell)
        || !cap.registry.same_registry(&shell.media)
        || shell.core.active_account.as_deref() != Some("refresh-native-fixture")
    {
        return Err("Retired refresh capability".into());
    }
    Ok(cap)
}
fn refused() -> Error {
    Error::Api("refresh_fixture_refused".into())
}
pub(in crate::file_preview::refresh) fn metadata(
    ticket: &Ticket,
    shell: &ShellState,
) -> Result<File, Error> {
    let cap = get(shell).map_err(|_| refused())?;
    if !ticket.current(shell) || ticket.lease.0.file != FILE {
        return Err(refused());
    }
    let mut l = cap.ledger.lock().map_err(|_| refused())?;
    if ticket.attempt != l.metadata.len() as u64 + 1 || ticket.attempt > 3 || l.pending.is_some() {
        return Err(refused());
    }
    l.metadata.push(ticket.attempt);
    Ok(file())
}
pub(in crate::file_preview::refresh) async fn body(
    ticket: &Ticket,
    state: Signal<ShellState>,
    url: &str,
) -> Result<Vec<u8>, Error> {
    let rx = {
        let shell = state.peek();
        let cap = get(&shell).map_err(|_| refused())?;
        if !ticket.current(&shell) || url != URL {
            return Err(refused());
        }
        let mut l = cap.ledger.lock().map_err(|_| refused())?;
        if l.metadata.last() != Some(&ticket.attempt)
            || ticket.attempt != l.bodies.len() as u64 + 1
            || l.pending.is_some()
        {
            return Err(refused());
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        l.bodies.push(ticket.attempt);
        l.pending = Some((ticket.attempt, tx));
        rx
    };
    let result = tokio::time::timeout(Duration::from_secs(40), rx)
        .await
        .map_err(|_| refused())?
        .map_err(|_| refused())?;
    get(&state.peek()).map_err(|_| refused())?;
    // Ticket::run checks the actual revision after this await, before decode/publish.
    result
}
impl Capability {
    pub fn snapshot(&self) -> Result<Value, String> {
        let l = self.ledger.lock().map_err(|_| "Refresh ledger poisoned")?;
        Ok(
            json!({"metadataAttempts":l.metadata,"bodyAttempts":l.bodies,
            "released":l.releases,"pending":l.pending.as_ref().map(|(n,_)|*n),
            "file":FILE,"url":URL,"size":OLD.len(),"syntheticReads":true}),
        )
    }
    pub fn release(&self, attempt: u64) -> Result<(), String> {
        let mut l = self.ledger.lock().map_err(|_| "Refresh ledger poisoned")?;
        if l.pending.as_ref().map(|(n, _)| *n) != Some(attempt) {
            return Err("Wrong held refresh body".into());
        }
        let (_, tx) = l.pending.take().unwrap();
        l.releases.push((
            attempt,
            if attempt == 1 {
                "failure"
            } else if attempt == 2 {
                "stale-body"
            } else {
                "fresh-body"
            },
        ));
        tx.send(if attempt == 1 {
            Err(Error::Offline("synthetic failure".into()))
        } else {
            Ok(NEW.as_bytes().to_vec())
        })
        .map_err(|_| "Refresh body waiter retired".into())
    }
}
