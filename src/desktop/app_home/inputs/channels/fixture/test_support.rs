use super::*;
use directory::read_scope::ReadScope;
use tokio::sync::oneshot;

pub(super) fn setup() -> (
    crate::bootstrap::test_support::Harness,
    Ticket,
    u64,
    u64,
    ReadScope,
) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("EHOME0001".into());
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"destination","optional":true,"label":{"type":"plain_text","text":"Channel"},"element":{"type":"channels_select","action_id":"pick","initial_channel":"CINITIAL1"}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    crate::app_home::inputs::hydrate(&mut s);
    s.app_home.channels.fixture = Some(Harness {
        reply: Reply::Conversations,
        hold: true,
        pending: None,
        pending_owner: None,
        lane: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        requests: vec![],
        releases: vec![],
    });
    let key = InputKey {
        block_id: "destination".into(),
        action_id: "pick".into(),
    };
    let ticket = Ticket::capture(&s, &key).unwrap();
    assert!(open_local(&mut s, ticket.clone()));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    let (serial, revision) = (p.serial, p.query_revision);
    let harness = directory::read_scope::test_support::mount(s);
    let scope = super::super::search::fixture_scope(harness.state, &ticket, serial, revision);
    (harness, ticket, serial, revision, scope)
}
pub(super) fn register(
    s: &mut ShellState,
    _: &Ticket,
) -> Option<oneshot::Receiver<Result<Value, Error>>> {
    let (send, receive) = oneshot::channel();
    let f = s.app_home.channels.fixture.as_mut().unwrap();
    assert!(f.pending.as_ref().is_none_or(|old| old.is_closed()));
    f.pending = Some(send);
    f.requests.push(json!({"closedInjection":true}));
    Some(receive)
}
pub(super) fn deliver(mut state: Signal<ShellState>, value: Result<Value, Error>) {
    state
        .write()
        .app_home
        .channels
        .fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
        .unwrap()
        .send(value)
        .unwrap();
}
