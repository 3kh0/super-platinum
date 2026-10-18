//! Explicit disconnected capability. No external request, journal or response program.
use super::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{OnceLock, Weak},
    time::Duration,
};
#[cfg(any(test, feature = "protected-native-fixture"))]
use super_platinum_core::config::{Session, WorkspaceSession};
use super_platinum_core::slack::{Error, PreparedRequest, Transport};
pub(crate) mod check;
mod harness;
mod keyboard;
mod server;
pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000191";
const TOKEN: &str = "scheduled-forward-fixture-only";
pub(crate) const SCENARIOS: &[&str] = &[
    "ordered-rich",
    "person-rich",
    "lost-readback",
    "lost-edit",
    "conflict-review",
    "owner-retirement",
];
fn installed() -> &'static Mutex<Weak<State>> {
    static VALUE: OnceLock<Mutex<Weak<State>>> = OnceLock::new();
    VALUE.get_or_init(Mutex::default)
}
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("forward-scheduled-dynamic" | "forward-scheduled-dynamic-narrow")
        )
}
#[derive(Default)]
struct Hold {
    armed: bool,
    reached: bool,
    released: bool,
}
pub(crate) struct State {
    pub nonce: String,
    pub scenario: String,
    pub phase: String,
    pub directory: PathBuf,
    transport: Arc<Transport>,
    server: Mutex<server::Server>,
    hold: Mutex<Hold>,
}
impl State {
    fn configured(&self) -> bool {
        enabled()
            && std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_NONCE").as_deref()
                == Ok(self.nonce.as_str())
            && std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_SCENARIO").as_deref()
                == Ok(self.scenario.as_str())
            && std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_PHASE").as_deref()
                == Ok(self.phase.as_str())
            && std::env::var_os("SUPER_PLATINUM_SCHEDULED_FORWARD_READY_DIR")
                .is_some_and(|p| PathBuf::from(p) == self.directory)
    }
    pub fn admits(&self, shell: &ShellState) -> bool {
        self.configured()
            && shell.core.active_account.as_deref() == Some(ACCOUNT)
            && shell.core.active_team.as_deref() == Some("T1")
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && shell.core.session.as_ref().is_some_and(|s| {
                s.d_cookie == TOKEN
                    && s.workspaces.len() == 1
                    && s.workspaces.get("T1").is_some_and(|w| {
                        w.token == TOKEN
                            && w.user_id == "U00000001"
                            && w.url == "https://fixture.slack.com"
                    })
            })
    }
    pub fn evidence(&self) -> Value {
        let s = self.server.lock().unwrap();
        json!({"rows":s.rows,"ledger":s.ledger,"writes":s.writes,"external":s.external})
    }
    pub async fn inject_conflict(self: &Arc<Self>) -> Result<(), String> {
        if !self.configured() || self.scenario != "conflict-review" || self.phase != "exercise" {
            return Err("Conflict injection not admitted".into());
        }
        let owner = self.clone();
        tokio::task::spawn_blocking(move || {
            harness::validate_owned(&owner.directory, &owner.nonce, &owner.scenario)
                .map_err(|_| "Conflict owner changed".to_owned())?;
            let mut server = owner
                .server
                .lock()
                .map_err(|_| "Conflict server unavailable")?;
            server.inject_conflict()?;
            harness::save_server(&owner.directory, &server)
                .map_err(|_| "Conflict server persistence failed".to_owned())
        })
        .await
        .map_err(|_| "Conflict task failed")?
    }
    pub fn arm_read(&self) -> Result<(), String> {
        let mut h = self.hold.lock().unwrap();
        if h.armed {
            return Err("Read already held".into());
        }
        *h = Hold {
            armed: true,
            ..Default::default()
        };
        Ok(())
    }
    pub fn release_read(&self) {
        self.hold.lock().unwrap().released = true;
    }
    pub async fn wait_read(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.hold.lock().unwrap().reached {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err("Held read was not reached".into())
    }
    async fn hold_read(&self) -> Result<(), Error> {
        {
            let mut h = self.hold.lock().unwrap();
            if !h.armed {
                return Ok(());
            }
            h.reached = true;
        }
        for _ in 0..500 {
            {
                let mut h = self.hold.lock().unwrap();
                if h.released {
                    *h = Hold::default();
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(server::refused("hold_timeout"))
    }
}
pub(crate) fn journal_path(shell: &ShellState) -> Option<PathBuf> {
    let value = installed().lock().ok()?.upgrade()?;
    value
        .admits(shell)
        .then(|| value.directory.join("journal.sqlite"))
}
pub(super) fn validate_storage(
    account: &str,
    owner: &model::Owner,
    path: Option<&std::path::Path>,
) -> Result<(), Error> {
    let value = installed()
        .lock()
        .map_err(|_| server::refused("capability"))?
        .upgrade()
        .ok_or_else(|| server::refused("capability"))?;
    if !value.configured()
        || account != ACCOUNT
        || owner.team != "T1"
        || owner.user != "U00000001"
        || path != Some(value.directory.join("journal.sqlite").as_path())
    {
        return Err(server::refused("journal_owner"));
    }
    // The captured original owner may persist a late result after visible retirement.
    // This capability grants only its private journal, never the default store.
    harness::validate_owned(&value.directory, &value.nonce, &value.scenario)
}
pub(crate) fn owns_draft(shell: &ShellState, draft: &str) -> bool {
    let Some(value) = installed().lock().ok().and_then(|w| w.upgrade()) else {
        return false;
    };
    value.admits(shell)
        && value
            .server
            .lock()
            .is_ok_and(|s| s.rows.contains_key(draft))
}
/// The same captured Context still controls preflight and shared mutation fences.
/// A fixture cannot fall through to an ordinary transport even if misconfigured.
pub(crate) async fn execute(
    context: &crate::scheduled::Context,
    request: PreparedRequest,
    mutation: bool,
) -> Result<Value, Error> {
    if !crate::fixture::is_fixture() {
        return if mutation {
            context.transport.execute_draft_mutation(request).await
        } else {
            context.transport.execute(request).await
        };
    }
    let value = installed()
        .lock()
        .map_err(|_| server::refused("capability"))?
        .upgrade()
        .ok_or_else(|| server::refused("capability"))?;
    if !value.configured()
        || !Arc::ptr_eq(&context.transport, &value.transport)
        || context.session.token != TOKEN
        || context.session.team_id != "T1"
        || context.session.user_id != "U00000001"
    {
        return Err(server::refused("owner"));
    }
    let endpoint = request.url.split('?').next().unwrap_or("");
    if mutation
        != matches!(
            endpoint.rsplit('/').next(),
            Some("drafts.create" | "drafts.update" | "drafts.delete")
        )
    {
        return Err(server::refused("request_kind"));
    }
    if !mutation {
        value.hold_read().await?;
    }
    tokio::task::spawn_blocking(move || {
        harness::validate_owned(&value.directory, &value.nonce, &value.scenario)?;
        let mut server = value
            .server
            .lock()
            .map_err(|_| server::refused("server_lock"))?;
        let result = server.execute(request, &value.scenario, &value.phase);
        // Persist an applied mutation before a lost reply is returned to the host.
        harness::save_server(&value.directory, &server)?;
        result
    })
    .await
    .map_err(|_| server::refused("server_worker"))?
}

#[cfg(not(any(test, feature = "protected-native-fixture")))]
pub(crate) async fn install(_state: Signal<ShellState>) -> Result<Arc<State>, String> {
    Err("Scheduled forward fixture requires protected-native-fixture".into())
}

#[cfg(any(test, feature = "protected-native-fixture"))]
pub(crate) async fn install(mut state: Signal<ShellState>) -> Result<Arc<State>, String> {
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Fresh disconnected shell required".into());
    }
    let (directory, owner) = harness::wait_runner(state).await?;
    let scenario = owner["scenario"]
        .as_str()
        .ok_or("Missing scenario")?
        .to_owned();
    let phase = owner["phase"].as_str().ok_or("Missing phase")?.to_owned();
    let root = directory.clone();
    let restart = phase == "restart";
    let nonce = owner["nonce"].as_str().ok_or("Missing nonce")?.to_owned();
    let owner_scenario = scenario.clone();
    let server = tokio::task::spawn_blocking(move || {
        harness::validate_owned(&root, &nonce, &owner_scenario)
            .map_err(|_| "Private files failed ownership validation".to_owned())?;
        harness::load_server(&root, restart)
    })
    .await
    .map_err(|_| "Server restore task failed")??;
    let transport = Arc::new(Transport::disconnected_fixture(TOKEN));
    let value = Arc::new(State {
        nonce: owner["nonce"].as_str().unwrap().into(),
        scenario,
        phase,
        directory,
        transport: transport.clone(),
        server: Mutex::new(server),
        hold: Mutex::default(),
    });
    if installed()
        .lock()
        .map_err(|_| "Fixture registry unavailable")?
        .upgrade()
        .is_some()
    {
        return Err("Fixture already installed".into());
    }
    {
        let mut shell = state.write();
        if shell.core.session.is_some() || shell.core.transport.is_some() {
            return Err("Shell acquired a live connection".into());
        }
        shell.core.active_account = Some(ACCOUNT.into());
        shell.core.active_team = Some("T1".into());
        shell.self_account.user_id = "U00000001".into();
        let workspace = shell
            .core
            .workspaces
            .get_mut("T1")
            .ok_or("Fixture workspace missing")?;
        workspace.self_user_id = "U00000001".into();
        workspace.users.insert(
            "U00000991".into(),
            super_platinum_core::slack::models::User {
                id: "U00000991".into(),
                name: Some("nativeforwardrecipient".into()),
                real_name: Some("Native Forward Recipient".into()),
                profile: Some(super_platinum_core::slack::models::UserProfile {
                    display_name: Some("Native Forward Recipient".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        shell.core.session = Some(Session {
            d_cookie: TOKEN.into(),
            unified_user_client_enabled: BTreeMap::new(),
            workspaces: BTreeMap::from([(
                "T1".into(),
                WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: "U00000001".into(),
                    name: "Isolated scheduled forward".into(),
                    url: "https://fixture.slack.com".into(),
                    token: TOKEN.into(),
                },
            )]),
        });
        shell.core.transport = Some(transport);
        let epoch = shell.core.account_epoch;
        shell.core.scheduled.ensure_scope(epoch, "T1");
    }
    *installed()
        .lock()
        .map_err(|_| "Fixture registry unavailable")? = Arc::downgrade(&value);
    Ok(value)
}
