use super::super::super::operation::tests::Fixture as Store;
use super::*;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{
        Transport,
        realtime::{ConnectParams, flannel_url},
    },
};

pub(in super::super::super) struct Fixture {
    pub(in super::super::super) store: Store,
    pub(in super::super::super) shell: ShellState,
    pub(in super::super::super) source: Arc<Source>,
    _worker: observation::WorkerSource,
}
impl Fixture {
    pub(in super::super::super) async fn new() -> Self {
        let store = Store::new().await;
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let team = "T1".to_owned();
        shell.core.active_account = Some(store.account.id().into());
        shell.core.account_epoch = 7;
        shell.core.transport = Some(Arc::new(Transport::new("fixture-only-cookie").unwrap()));
        shell.core.session = Some(Session {
            unified_user_client_enabled: Default::default(),
            d_cookie: "fixture-only-cookie".into(),
            workspaces: [(
                team.clone(),
                WorkspaceSession {
                    team_id: team.clone(),
                    enterprise_id: None,
                    user_id: shell.core.workspaces[&team].self_user_id.clone(),
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "fixture-only-token".into(),
                },
            )]
            .into(),
        });
        let params = ConnectParams {
            team: team.clone(),
            ws_url: flannel_url("fixture-only-token", &team),
            d_cookie: "fixture-only-cookie".into(),
            user_agent: "fixture-only-agent".into(),
        };
        let mut worker = observation::WorkerSource::capture(&mut shell, params).unwrap();
        worker.take_connect_params(&shell).unwrap();
        assert!(worker.can_accept(&shell, &team, 9));
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation = 9;
        let socket = Arc::new(worker.accept(&mut shell, &team, 9).unwrap());
        let stream = store.verified_stream().await;
        let source = Source::captured(store.account.clone(), socket, 9, stream, &shell).unwrap();
        Self {
            store,
            shell,
            source,
            _worker: worker,
        }
    }
}

#[tokio::test]
async fn source_drop_invalidates_without_inventory_keeping_source_alive() {
    let Fixture {
        store,
        shell,
        source,
        _worker,
    } = Fixture::new().await;
    let handle = source.ownership();
    let before = handle.snapshot(&store.account, &shell).unwrap();
    let weak = Arc::downgrade(&source);
    drop(source);
    assert!(weak.upgrade().is_none());
    assert_eq!(
        before.validate(&handle, &store.account, &shell),
        Err(Refused::Changed)
    );
    let after = handle.snapshot(&store.account, &shell).unwrap();
    assert_eq!(after.phase, Phase::Dropped);
    assert_eq!(after.gap, Some(durable::GapReason::InterruptedAdmission));
    assert!(!after.stream_live);
    // No claim that the asynchronous gap write has completed.
}

#[tokio::test]
async fn same_textual_account_reallocation_cannot_validate_component_stamp() {
    let f = Fixture::new().await;
    let replacement = Store::new().await;
    assert_eq!(f.store.account.id(), replacement.account.id());
    let h = f.source.ownership();
    let before = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert_eq!(
        before.validate(&h, &replacement.account, &f.shell),
        Err(Refused::Account)
    );
    assert!(matches!(
        h.snapshot(&replacement.account, &f.shell),
        Err(Refused::Account)
    ));
}

#[tokio::test]
async fn gap_and_socket_owner_changes_refuse_old_observations() {
    let f = Fixture::new().await;
    let h = f.source.ownership();
    let before = h.snapshot(&f.store.account, &f.shell).unwrap();
    drop(f._worker);
    assert_eq!(
        before.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Changed)
    );
    f.source.gap(durable::GapReason::Disconnect);
    let after = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert_eq!(
        after.phase,
        Phase::GapRequested(durable::GapReason::Disconnect)
    );
    assert!(!after.stream_live);
    assert!(!after.socket_current);
}

#[tokio::test]
async fn queue_registration_after_enumeration_and_drop_after_gap_are_not_invisible() {
    let Fixture {
        store,
        shell,
        source,
        _worker,
    } = Fixture::new().await;
    let h = source.ownership();
    let before = h.snapshot(&store.account, &shell).unwrap();
    let q = ObservationQueue::new(store.account.clone(), source.clone());
    assert_eq!(
        before.validate(&h, &store.account, &shell),
        Err(Refused::Changed)
    );
    source.gap(durable::GapReason::Disconnect);
    // Drop the queue before capturing a handle-only Source case separately:
    // disappeared registrations must remain unknown, not silently be omitted.
    drop(q);
    drop(source);
    assert!(matches!(
        h.snapshot(&store.account, &shell),
        Err(Refused::Unknown)
    ));
    let state = h.0.state.lock().unwrap();
    assert_eq!(state.phase, Phase::Dropped);
    assert_eq!(state.gap, Some(durable::GapReason::Disconnect));
}

#[test]
fn clock_exhaustion_is_permanently_unknown() {
    let c = Clock::default();
    c.generation.store(u64::MAX, Ordering::Release);
    c.change();
    assert_eq!(c.read(), Err(Refused::Unknown));
    c.change();
    assert_eq!(c.read(), Err(Refused::Unknown));
    assert_eq!(c.generation.load(Ordering::Acquire), u64::MAX);
}
