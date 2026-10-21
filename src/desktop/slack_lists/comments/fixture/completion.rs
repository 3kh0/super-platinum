//! Named List channel-completion fixture. The identity carrier has no HTTP client.
use super::*;
use std::time::Instant;
use super_platinum_core::{
    config::Session,
    slack::{Transport, transport::Health},
};
pub(crate) mod files;
pub(crate) mod groups;
pub(crate) mod members;
pub(super) mod protocol;
#[cfg(test)]
mod tests;

pub(crate) const LOCAL: &str = "CLISTLOCAL";
pub(crate) const PRIVATE: &str = "GLISTPRIVATE";
pub(crate) const REMOTE: &str = "CLISTREMOTE";
pub(crate) const SELF: &str = "ULIST0001";
const COOKIE: &str = "synthetic-list-completion-cookie";
const TOKEN: &str = "synthetic-list-completion-token";
type Reply = (Value, Option<Vec<String>>);

#[derive(Clone)]
pub(crate) struct Model {
    transport: Arc<Transport>,
    session: WorkspaceSession,
    account: Option<String>,
    epoch: u64,
    ledger: Arc<Mutex<Ledger>>,
}
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    responses: Vec<Value>,
    releases: Vec<Value>,
    seed: Option<Value>,
    mode: Mode,
    held: Vec<(usize, oneshot::Sender<Result<Reply, Error>>, Reply)>,
    completed: usize,
    cancelled: usize,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Mode {
    #[default]
    Success,
    Empty,
    Malformed,
    Offline,
    Held,
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && super::compose::check_enabled()
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("channel-completion"
                | "channel-completion-custody"
                | "member-completion"
                | "member-completion-custody"
                | "group-completion"
                | "group-completion-custody"
                | "group-completion-restricted"
                | "file-completion"
                | "file-completion-custody"
                | "file-completion-off"
                | "file-navigation"
                | "file-navigation-custody"
                | "cold-files"
                | "cold-files-recovery"
                | "own-edit"
                | "own-delete"
                | "own-delete-failure"
                | "own-delete-storage"
                | "own-delete-guards"
                | "own-delete-rate-limit"
                | "own-delete-restart-pending-exercise"
                | "own-delete-restart-pending-restore"
                | "own-delete-restart-unknown-exercise"
                | "own-delete-restart-unknown-restore"
                | "own-edit-custody"
                | "own-edit-recovery"
                | "own-edit-storage"
                | "own-edit-restart-unsent-exercise"
                | "own-edit-restart-unsent-restore"
                | "own-edit-restart-unknown-exercise"
                | "own-edit-restart-unknown-restore")
        )
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named()
        && shell
            .slack_lists
            .fixture
            .as_ref()
            .and_then(|f| f.completion.as_ref())
            .is_some_and(|m| m.current(shell))
}
impl Model {
    fn current(&self, shell: &ShellState) -> bool {
        self.account == shell.core.active_account
            && shell.core.cache.is_none()
            && self.epoch == shell.core.account_epoch
            && shell.core.active_team.as_deref() == Some(self.session.team_id.as_str())
            && shell.self_account.user_id == self.session.user_id
            && shell
                .core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| {
                    w.team_id == self.session.team_id && w.self_user_id == self.session.user_id
                })
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && shell.core.session.as_ref().is_some_and(|s| {
                s.d_cookie == COOKIE
                    && s.workspaces.len() == 1
                    && s.workspaces
                        .get(&self.session.team_id)
                        .is_some_and(|v| crate::emoji_preferences::same_session(v, &self.session))
            })
    }
    pub(crate) fn release(&self) -> Result<Value, Error> {
        let (serial, sender, reply) = {
            let mut l = self.ledger.lock().unwrap();
            if l.held.is_empty() {
                return Err(refused());
            }
            l.held.remove(0)
        };
        let delivered = sender.send(Ok(reply)).is_ok();
        let proof =
            json!({"serial":serial,"delivered":delivered,"cancelledBeforeReply":!delivered});
        self.ledger.lock().unwrap().releases.push(proof.clone());
        Ok(proof)
    }
    pub(crate) fn snapshot(&self) -> Value {
        let l = self.ledger.lock().unwrap();
        json!({"requests":l.requests,"responses":l.responses,"releases":l.releases,"seed":l.seed,"completed":l.completed,"cancelled":l.cancelled,
            "held":l.held.len(),"networkCalls":0,"httpCapability":false})
    }
}
fn refused() -> Error {
    Error::Offline("Closed List completion fixture refused request".into())
}
pub(crate) fn handle(shell: &ShellState) -> Result<Model, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    shell
        .slack_lists
        .fixture
        .as_ref()
        .and_then(|f| f.completion.clone())
        .ok_or_else(refused)
}
/// Called before open_shell captures its List scope. No existing mount is re-owned.
pub(crate) fn install(shell: &mut ShellState) {
    if !named() {
        return;
    }
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    assert!(shell.core.cache.is_none());
    #[cfg(any(test, feature = "protected-native-fixture"))]
    {
        let team = shell.core.active_team.clone().expect("fixture team");
        // The legacy layout fixture has short U0/U1 identities. Canonical raw
        // member intake deliberately rejects those; start this named fixture
        // with a valid synthetic member before any List scope is captured.
        let user = SELF.to_owned();
        let workspace = shell.core.workspaces.get_mut(&team).unwrap();
        workspace.users.clear();
        // Every retained native channel needs its own accepted raw observation.
        // Legacy layout-only C1/C2 rows cannot join this canonical source.
        workspace.channels.clear();
        workspace.self_user_id = user.clone();
        shell.self_account.user_id = user.clone();
        let session = WorkspaceSession {
            team_id: team.clone(),
            user_id: user,
            enterprise_id: None,
            name: "List completion".into(),
            url: if super::file_navigation::named() {
                "https://list-navigation-fixture.slack.com"
            } else {
                "https://example.test"
            }
            .into(),
            token: TOKEN.into(),
        };
        let transport = Arc::new(Transport::disconnected_fixture(COOKIE));
        shell.core.session = Some(Session {
            d_cookie: COOKIE.into(),
            unified_user_client_enabled: if super::own_edit::named() {
                [(team.clone(), false)].into()
            } else {
                Default::default()
            },
            workspaces: [(team.clone(), session.clone())].into(),
        });
        shell.core.transport = Some(transport.clone());
        let model = Model {
            transport: transport.clone(),
            session,
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            ledger: Default::default(),
        };
        shell
            .slack_lists
            .fixture
            .as_mut()
            .expect("List fixture")
            .completion = Some(model);
        let (sender, _) = tokio::sync::mpsc::channel(1);
        shell.core.workspaces.get_mut(&team).unwrap().rt =
            super_platinum_core::state::RealtimeStatus::Connected(
                super_platinum_core::slack::realtime::Connection::from_sender(sender),
            );
        transport.set_health(Health::Online);
        shell.connection.routable = true;
        shell.connection.routed_at = Some(Instant::now());
    }
}
pub(crate) fn mode(shell: &ShellState, mode: Mode) -> Result<(), Error> {
    handle(shell)?.ledger.lock().unwrap().mode = mode;
    Ok(())
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    let model = handle(&state.peek()).map_err(|e| e.to_string())?;
    let (boot, init) = seed_payloads(&model.session);
    model.ledger.lock().unwrap().seed = Some(json!({"boot":boot,"init":init,"accepted":false}));
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    // Wait for the actual queued Init sink; acceptance cannot be inferred from
    // raw boot success while unrelated native rows still invalidate the source.
    for _ in 0..100 {
        if crate::app_home::channel_fetches::canonical::capture(
            &mut state.write(),
            &model.session.team_id,
        )
        .is_ok()
        {
            if groups::named() {
                let proof = groups::seed(state).await.map_err(|e| e.to_string())?;
                model.ledger.lock().unwrap().seed.as_mut().unwrap()["groups"] = proof;
            }
            model.ledger.lock().unwrap().seed.as_mut().unwrap()["accepted"] = json!(true);
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Err("Named List fixture canonical channel baseline unavailable".into())
}
fn seed_payloads(owner: &WorkspaceSession) -> (Value, Value) {
    if members::named() {
        let user = members::self_row(owner);
        let boot = members::boot(owner);
        let init = json!({"ok":true,"self":user,"team":{"id":owner.team_id,"name":"List member completion"},
            "workspaces":[{"id":owner.team_id}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
        return (boot, init);
    }
    let user = json!({"id":owner.user_id,"name":"fixture-self","team_id":owner.team_id,"is_admin":true,"profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team_id,"name":"List completion"},"self":user,"users":[user],
        "channels":[{"id":LOCAL,"name":"project-local","team_id":owner.team_id,"is_channel":true,"is_member":true,"updated":100,"previous_names":[]}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team_id,"name":"List completion"},
        "workspaces":[{"id":owner.team_id},{"id":"TOTHER001"}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    (boot, init)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    match handle(shell) {
        Ok(m) => {
            let mut value = m.snapshot();
            value["enabled"] = json!(true);
            value["named"] = json!(named());
            value["protectedFixture"] =
                json!(cfg!(any(test, feature = "protected-native-fixture")));
            value["identityCurrent"] = json!(m.current(shell));
            value["account"] = json!(m.account);
            value["epoch"] = json!(m.epoch);
            value["team"] = json!(m.session.team_id);
            value["user"] = json!(m.session.user_id);
            if groups::named() {
                value["groups"] =
                    groups::snapshot(shell).unwrap_or_else(|_| json!({"unavailable":true}));
            }
            if files::named() {
                value["files"] = files::snapshot(shell);
            }
            value
        }
        Err(_) => json!({"enabled":false}),
    }
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Reply, Error> {
    let (model, receiver) = {
        let shell = state.peek();
        let model = handle(&shell)?;
        let accepted = protocol::validate(&shell.core.client, &model.session, request)?;
        let mut l = model.ledger.lock().unwrap();
        let serial = l.requests.len() + 1;
        l.requests
            .push(json!({"serial":serial,"method":accepted.method,"args":accepted.args}));
        if !accepted.search {
            l.responses
                .push(json!({"serial":serial,"disposition":"response","raw":accepted.reply.0}));
            return Ok(accepted.reply);
        }
        let mode = std::mem::take(&mut l.mode);
        if !matches!(mode, Mode::Held) {
            l.completed += 1;
            let reply = if accepted.method == "search.autocomplete.files" {
                files::reply(&model.session, mode)
            } else if accepted.method == "users/search" {
                members::reply(&model.session, mode)
            } else if accepted.method == "usergroups/search" {
                groups::reply(&model.session, mode)
            } else {
                protocol::search_reply(&model.session, mode)
            };
            l.responses.push(match &reply {
                Ok((raw, _)) => json!({"serial":serial,"disposition":"response","raw":raw}),
                Err(_) => json!({"serial":serial,"disposition":"offline","raw":null}),
            });
            return reply;
        }
        let (sender, receiver) = oneshot::channel();
        l.responses
            .push(json!({"serial":serial,"disposition":"held","raw":accepted.reply.0}));
        l.held.push((serial, sender, accepted.reply));
        drop(l);
        (model, receiver)
    };
    struct Finish {
        model: Model,
        completed: bool,
    }
    impl Drop for Finish {
        fn drop(&mut self) {
            let mut l = self.model.ledger.lock().unwrap();
            l.completed += 1;
            if !self.completed {
                l.cancelled += 1;
            }
        }
    }
    let mut finish = Finish {
        model,
        completed: false,
    };
    let result = receiver.await.map_err(|_| refused())?;
    finish.completed = true;
    result
}
pub(crate) fn ranking_request(
    shell: &ShellState,
    request: &PreparedRequest,
) -> Result<Reply, Error> {
    let model = handle(shell)?;
    let accepted = protocol::validate(&shell.core.client, &model.session, request)?;
    if !matches!(
        accepted.method.as_str(),
        "search.autocomplete.model" | "search.autocomplete.offlineFeatures"
    ) {
        return Err(refused());
    }
    let mut l = model.ledger.lock().unwrap();
    let serial = l.requests.len() + 1;
    l.requests
        .push(json!({"serial":serial,"method":accepted.method,"args":accepted.args}));
    l.responses
        .push(json!({"serial":serial,"disposition":"response","raw":accepted.reply.0}));
    Ok(accepted.reply)
}
