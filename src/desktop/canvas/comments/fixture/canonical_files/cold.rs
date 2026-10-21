//! Disconnected absent-file metadata receipts, separate from click resolution.
use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        SlackClient,
        client::{PreparedRequest, RequestBody},
    },
};
use tokio::sync::oneshot;
mod protocol;
#[cfg(test)]
mod tests;

const FLAG: &str = "SUPER_PLATINUM_CANVAS_COLD_FILE_CHECK";
const ACCOUNT: &str = "147b5480-9f1a-4443-89da-836472cbb209";

#[derive(Clone, Copy, Default)]
pub(in crate::canvas) enum Mode {
    #[default]
    Offline,
    RateLimited,
    Held,
    Malformed,
    WrongId,
    Error,
    Success,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::RateLimited => "rate-limited",
            Self::Held => "held",
            Self::Malformed => "malformed",
            Self::WrongId => "wrong-id",
            Self::Error => "error",
            Self::Success => "success",
        }
    }
}
struct Held {
    serial: usize,
    id: String,
    response: Value,
    sender: oneshot::Sender<Value>,
}
#[derive(Default)]
struct Ledger {
    mode: Mode,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Vec<Held>,
    rate_started_at: Option<std::time::Instant>,
}
static READS: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn named(name: &str) -> bool {
    matches!(
        name,
        "canvas-canonical-file-custody"
            | "canvas-canonical-file-custody-narrow"
            | "canvas-canonical-native-canvas"
            | "canvas-canonical-native-canvas-narrow"
            | "canvas-canonical-native-list"
            | "canvas-canonical-native-list-narrow"
    )
}
pub(in crate::canvas) fn requested() -> bool {
    permitted(
        cfg!(any(test, feature = "protected-native-fixture")),
        crate::fixture::is_fixture(),
        std::env::var(super::FLAG).as_deref().ok(),
        std::env::var(FLAG).as_deref().ok(),
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref().ok(),
    )
}
fn permitted(
    protected: bool,
    fixture: bool,
    base: Option<&str>,
    cold: Option<&str>,
    name: Option<&str>,
) -> bool {
    protected && fixture && base == Some("1") && cold == Some("1") && name.is_some_and(named)
}
fn identity(
    account: Option<&str>,
    team: Option<&str>,
    user: &str,
    session: bool,
    transport: bool,
    canvas: Option<&str>,
) -> bool {
    account == Some(ACCOUNT)
        && team == Some("T1")
        && user == "U0"
        && !session
        && !transport
        && canvas == Some("Ffixture")
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && super::enabled(shell)
        && identity(
            shell.core.active_account.as_deref(),
            shell.core.active_team.as_deref(),
            &shell.self_account.user_id,
            shell.core.session.is_some(),
            shell.core.transport.is_some(),
            shell.canvas.as_ref().map(|f| f.id.as_str()),
        )
}
pub(in crate::canvas) fn target_id() -> &'static str {
    destination_id().unwrap_or(HOSTED)
}
fn refused() -> Error {
    Error::Offline("Closed Canvas cold-file reader unavailable".into())
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U0".into(),
        enterprise_id: None,
        name: "Cold Canvas fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
pub(super) fn decorate(rows: &mut [Value]) {
    if requested() {
        decorate_for(rows, target_id());
    }
}
fn decorate_for(rows: &mut [Value], id: &str) {
    for row in rows {
        if row["ts"] == ROOT {
            row["files"] = json!([{"id":id,"file_access":"check_file_info"}]);
        } else if let Some(files) = row.get_mut("files").and_then(Value::as_array_mut) {
            for value in files {
                if value["id"] == id {
                    // Other roots remain warm controls, never another same-ID
                    // source which could accidentally satisfy the cold demand.
                    *value = file(SECOND);
                }
            }
        }
    }
}
pub(in crate::canvas) fn configure(shell: &ShellState, id: &str, mode: Mode) -> Result<(), Error> {
    if !enabled(shell) || id != target_id() {
        return Err(refused());
    }
    READS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| refused())?
        .mode = mode;
    Ok(())
}
pub(in crate::canvas) fn request(shell: &ShellState, id: &str) -> Result<PreparedRequest, Error> {
    if !enabled(shell) || id != target_id() {
        return Err(refused());
    }
    Ok(super::super::super::file_acquisition::request(
        &shell.core.client,
        &session(),
        id,
    ))
}
fn response(mode: Mode, id: &str) -> Value {
    match mode {
        Mode::Offline | Mode::RateLimited => Value::Null,
        Mode::Malformed => json!({"ok":true,"file":false}),
        Mode::WrongId => json!({"ok":true,"file":file(SECOND)}),
        Mode::Error => json!({"ok":false,"error":"file_not_found"}),
        Mode::Held | Mode::Success => {
            let mut value = file(id);
            value["name"] = json!(format!("Cold canonical {id} & \u{4e2d}\u{1f98a}"));
            value["title"] = json!("Different cold title must not label root");
            json!({"ok":true,"file":value})
        }
    }
}
fn outcome(mode: Mode, raw: Value) -> Result<Value, Error> {
    match mode {
        Mode::Offline => Err(refused()),
        Mode::RateLimited => Err(Error::RateLimited {
            retry_after_secs: Some(15),
        }),
        _ => Ok(raw),
    }
}
fn record_rate(ledger: &mut Ledger, mode: Mode, now: std::time::Instant) {
    if matches!(mode, Mode::RateLimited) {
        ledger.rate_started_at.get_or_insert(now);
    }
}
/// Read separately from stable screenshot evidence; never resets on retry.
pub(in crate::canvas) fn rate_elapsed_ms() -> Option<u128> {
    READS
        .get_or_init(Default::default)
        .lock()
        .ok()?
        .rate_started_at
        .map(|started| started.elapsed().as_millis())
}
struct Finish {
    serial: usize,
    completed: bool,
}
impl Drop for Finish {
    fn drop(&mut self) {
        if let Ok(mut ledger) = READS.get_or_init(Default::default).lock() {
            ledger
                .settlements
                .push(json!({"serial":self.serial,"cancelled":!self.completed}));
        }
    }
}
pub(in crate::canvas) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let (serial, mode, raw, receiver) = {
        let shell = state.peek();
        if !enabled(&shell) {
            return Err(refused());
        }
        let id = target_id();
        let args = protocol::validate(&shell.core.client, &session(), request, id)?;
        let mut ledger = READS
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| refused())?;
        if ledger.requests.len() >= 32 {
            return Err(refused());
        }
        let serial = ledger.requests.len() + 1;
        let mode = ledger.mode;
        let raw = response(mode, id);
        record_rate(&mut ledger, mode, std::time::Instant::now());
        ledger.requests.push(json!({"serial":serial,"id":id,"method":"files.info",
            "path":"/api/files.info","args":args,"mode":mode.label(),"reason":"files-store-unknown-fetch"}));
        ledger
            .responses
            .push(json!({"serial":serial,"id":id,"disposition":mode.label(),"raw":raw}));
        let receiver = if matches!(mode, Mode::Held) {
            let (sender, receiver) = oneshot::channel();
            ledger.held.push(Held {
                serial,
                id: id.into(),
                response: raw.clone(),
                sender,
            });
            Some(receiver)
        } else {
            None
        };
        (serial, mode, raw, receiver)
    };
    let mut finish = Finish {
        serial,
        completed: false,
    };
    let raw = if let Some(receiver) = receiver {
        tokio::time::timeout(std::time::Duration::from_secs(45), receiver)
            .await
            .map_err(|_| refused())?
            .map_err(|_| refused())?
    } else {
        raw
    };
    finish.completed = true;
    outcome(mode, raw)
}
pub(in crate::canvas) fn release(shell: &ShellState, serial: usize) -> Result<Value, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let mut ledger = READS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| refused())?;
    let at = ledger
        .held
        .iter()
        .position(|h| h.serial == serial)
        .ok_or_else(refused)?;
    let held = ledger.held.remove(at);
    let cancelled = held.sender.is_closed();
    let delivered = held.sender.send(held.response).is_ok();
    let proof = json!({"serial":serial,"id":held.id,"delivered":delivered,"cancelledBeforeReply":cancelled});
    ledger.releases.push(proof.clone());
    Ok(proof)
}
pub(in crate::canvas) fn snapshot() -> Value {
    let ledger = READS.get_or_init(Default::default).lock().unwrap();
    let held = ledger
        .held
        .iter()
        .map(|h| json!({"serial":h.serial,"id":h.id,"receiverClosed":h.sender.is_closed()}))
        .collect::<Vec<_>>();
    json!({"requests":ledger.requests,"responses":ledger.responses,
        "settlements":ledger.settlements,"releases":ledger.releases,"held":held,
        "completed":ledger.settlements.len(),
        "cancelled":ledger.settlements.iter().filter(|v|v["cancelled"]==true).count(),
        "networkCalls":0,"httpCapability":false})
}
