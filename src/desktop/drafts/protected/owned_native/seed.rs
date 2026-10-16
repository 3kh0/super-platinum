//! Actual isolated route and captured synthetic session, with no config/keyring.
use super::super::owned_route::{Held, o};
use super::*;
use super_platinum_core::{
    config::{Session as Login, WorkspaceSession},
    slack::{
        Transport,
        realtime::{ConnectParams, flannel_url},
    },
};
const COOKIE: &str = "owned-send-fixture-cookie";
const TOKEN: &str = "owned-send-fixture-token";
struct Witness {
    account: Arc<Account>,
    transport: Arc<Transport>,
    epoch: u64,
    view_id: u64,
    user: String,
}
static WITNESS: OnceLock<Witness> = OnceLock::new();
pub(super) fn self_user() -> Option<String> {
    WITNESS.get().map(|v| v.user.clone())
}
pub(super) fn permits_socket(s: &ShellState) -> bool {
    let Some(w) = WITNESS.get() else {
        return false;
    };
    enabled()
        && w.account.owned.is_some()
        && s.core.cache.is_none()
        && s.core.active_account.as_deref() == Some(w.account.id())
        && s.core.active_team.as_deref() == Some("T1")
        && s.core.account_epoch == w.epoch
        && s.draft_writes.view_id == w.view_id
        && s.core
            .transport
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, &w.transport))
        && s.core
            .workspaces
            .get("T1")
            .is_some_and(|v| v.self_user_id == w.user)
        && s.core.session.as_ref().is_some_and(|v| {
            v.d_cookie == COOKIE
                && v.workspaces.len() == 1
                && v.workspaces.get("T1").is_some_and(|t| {
                    t.team_id == "T1"
                        && t.user_id == w.user
                        && t.token == TOKEN
                        && t.url == "https://owned-send.slack.com"
                        && t.enterprise_id.is_none()
                })
        })
}
fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(
    v: Result<o::StorageJob<I, T>, o::AdmissionRefused<I>>,
) -> Result<T, String> {
    let job = v.map_err(|v| format!("Fixture admission {:?}", v.reason))?;
    job.submit().map_err(|e| format!("Fixture submit {e:?}"))?;
    job.wait().map_err(|e| format!("Fixture wait {e:?}"))?;
    job.result().ok_or("Fixture result absent")?.map_err(err)
}
pub(crate) async fn install(mut state: Signal<ShellState>) -> Result<Arc<Run>, String> {
    let config = isolation::configuration()?;
    {
        let shell = state.peek();
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.core.cache.is_some()
            || shell.core.active_account.is_some()
        {
            return Err("Owned fixture seed isolation missing".into());
        }
    }
    let checked = config.clone();
    let (initialized, ticket, stream) = tokio::task::spawn_blocking(move || {
        isolation::validate(&checked)?;
        let route = o::OwnedRoute::fresh_fixture().map_err(err)?;
        let before = route.inspect().map_err(err)?.wait().map_err(err)?;
        if before.metadata().kind != o::StoreKind::Missing {
            return Err("Fresh Owned store was not missing".into());
        }
        route.commit(&before).map_err(err)?.wait().map_err(err)?;
        let opened = route
            .inspect()
            .map_err(err)?
            .wait()
            .map_err(err)?
            .open()
            .map_err(err)?;
        let ticket = opened.fixture_ticket().map_err(err)?;
        let initialized = opened
            .initialize_cold(&o::FactoryIdentity::default())
            .map_err(err)?
            .wait()
            .map_err(err)?;
        let ops = initialized.operations();
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let scope = Scope {
                account_id: ops.account().into(),
                key: key(target),
            };
            let inventory = done(ops.local_inventory(scope))?;
            done(ops.enroll(inventory))?;
        }
        let scope = Scope {
            account_id: ops.account().into(),
            key: key(ComposerTarget::Channel),
        };
        let expected = done(ops.stream_state(o::TeamInput {
            scope: scope.clone(),
        }))?;
        let stream = done(ops.open_stream(o::OpenInput { scope, expected }))?;
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let scope = Scope {
                account_id: ops.account().into(),
                key: key(target),
            };
            let expected = done(ops.review(scope))?;
            done(ops.verify_local(o::MutationInput {
                stream: stream.clone(),
                expected,
            }))?;
        }
        Ok::<_, String>((initialized, ticket, stream))
    })
    .await
    .map_err(|_| "Owned seed task failed")??;
    let account = Account::from_retained(initialized);
    let (epoch, view_id, user) = {
        let mut shell = state.write();
        shell.core.active_account = Some(account.id().into());
        shell.core.account_epoch = shell
            .core
            .account_epoch
            .checked_add(1)
            .ok_or("Epoch exhausted")?;
        (
            shell.core.account_epoch,
            shell.draft_writes.view_id,
            shell
                .core
                .workspaces
                .get("T1")
                .ok_or("Fixture team absent")?
                .self_user_id
                .clone(),
        )
    };
    let registry = Arc::new(registry::Registry::default());
    registry.select(account.id().into(), epoch);
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let editor = editor::Editor::open(account.owner(key(target)).map_err(err)?)
            .await
            .map_err(err)?;
        editor.submit(&local(INITIAL, target)).map_err(err)?;
        editor.flush().await.map_err(err)?;
        registry
            .replace(account.id(), epoch, &key(target), None, editor)
            .map_err(err)?;
    }
    let loaded = account
        .owned
        .as_ref()
        .ok_or("Owned route absent")?
        .inventory()
        .await
        .map_err(err)?;
    let mut session = Session::default();
    session.select(account.id().into(), epoch);
    let load = session.begin_load().ok_or("Protected load ticket absent")?;
    session.accept_load(load, &loaded).map_err(err)?;
    // Captured identities stay real; this fixture owns no HTTP client.
    let transport = Arc::new(Transport::disconnected_fixture(COOKIE));
    WITNESS
        .set(Witness {
            account: account.clone(),
            transport: transport.clone(),
            epoch,
            view_id,
            user: user.clone(),
        })
        .map_err(|_| "Owned synthetic witness already set")?;
    let (worker, source, network) = {
        let mut shell = state.write();
        if shell.core.active_account.as_deref() != Some(account.id())
            || shell.core.account_epoch != epoch
            || shell.draft_writes.view_id != view_id
        {
            return Err("Owned seed owner changed".into());
        }
        shell.core.transport = Some(transport);
        shell.core.session = Some(Login {
            d_cookie: COOKIE.into(),
            workspaces: [(
                "T1".into(),
                WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: user,
                    name: "Owned fixture".into(),
                    url: "https://owned-send.slack.com".into(),
                    token: TOKEN.into(),
                },
            )]
            .into(),
        });
        let params = ConnectParams {
            team: "T1".into(),
            ws_url: flannel_url(TOKEN, "T1"),
            d_cookie: COOKIE.into(),
            user_agent: "owned-fixture".into(),
        };
        let mut worker =
            crate::drafts::observation::WorkerSource::capture(&mut shell, params.clone())
                .map_err(|_| "Actual socket capture refused")?;
        if worker
            .take_connect_params(&shell)
            .map_err(|_| "Actual socket launch refused")?
            != params
        {
            return Err("Socket params changed".into());
        }
        // No connect is performed. The synthetic Connected event traverses the
        // actual captured-worker generation admission and Source constructors.
        if !worker.can_accept(&shell, "T1", 1) {
            return Err("Actual socket generation refused".into());
        }
        shell
            .core
            .workspaces
            .get_mut("T1")
            .ok_or("Fixture team absent")?
            .rt_generation = 1;
        let socket = Arc::new(
            worker
                .accept(&mut shell, "T1", 1)
                .map_err(|_| "Actual socket acceptance refused")?,
        );
        let source = Source::captured(account.clone(), socket, 1, stream, &shell).map_err(err)?;
        let network = crate::drafts::network::Binding::capture(&shell, "T1")
            .map_err(|_| "Actual network binding refused")?;
        (worker, source, network)
    };
    let b = Arc::new(Run {
        account,
        registry,
        source,
        worker: Mutex::new(worker),
        network,
        root: config.root,
        captures: config.captures,
        nonce: config.nonce,
        scenario: config.scenario,
        ticket,
        epoch,
        view_id,
        routed: AtomicU64::new(0),
        refused: AtomicU64::new(0),
        actions: Mutex::new(vec![]),
        native: Mutex::new(vec![]),
        attempts: Mutex::new(vec![]),
        parent: Mutex::new(None),
        held: AtomicBool::new(false),
        released: AtomicBool::new(false),
        changed: tokio::sync::Notify::new(),
    });
    if !b.current(&state.peek()) {
        return Err("Owned fixture binding changed".into());
    }
    SESSION.with_borrow_mut(|slot| *slot = Some(Rc::new(RefCell::new(session))));
    RUN.set(b.clone())
        .map_err(|_| "Owned fixture already installed")?;
    Ok(b)
}
fn err(e: durable::Error) -> String {
    format!("{e:?}")
}
