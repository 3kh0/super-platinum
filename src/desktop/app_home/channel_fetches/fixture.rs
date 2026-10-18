//! Closed offline adapter: real source ingress, dispatch, DOM transforms and drain.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::Session,
    slack::{
        PreparedRequest,
        events::{RawEvent, RtEvent},
    },
    state::{
        Workspace,
        cfm_payload::PayloadKind,
        directory_channels::freshness::refresh::{Branch, request},
    },
};
pub(crate) mod check;
const TEAM: &str = "TINIT0001";
const USER: &str = "UINIT0001";
const CHANNEL: &str = "CINIT0001";
#[derive(Default)]
pub(super) struct Model {
    requests: Vec<PreparedRequest>,
    proof: Value,
    focus: String,
}
pub(crate) fn install(shell: &mut ShellState) {
    let Some(transport) = disconnected() else {
        return;
    };
    let session: WorkspaceSession = serde_json::from_value(json!({"team_id":TEAM,"user_id":USER,
        "name":"Channel refresh","url":"https://example.test","token":"synthetic-channel-token"}))
    .unwrap();
    shell.core.workspaces.clear();
    shell
        .core
        .workspaces
        .insert(TEAM.into(), Workspace::from_session(&session));
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000081".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.active_channel = Some(CHANNEL.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-channel-cookie".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [(TEAM.into(), session.clone())].into(),
    });
    shell.core.transport = Some(Arc::new(transport));
    let transport = shell.core.transport.clone().unwrap();
    let boot = super_platinum_core::state::posting_metadata::authority::BootRead::begin_full(
        &mut shell.core,
        &transport,
        &session,
    )
    .unwrap();
    let raw = json!({"ok":true,"team":{"id":TEAM,"name":"Channel refresh"},"self":{"id":USER,"name":"fixture-self","team_id":TEAM,"profile":{}},
        "users":[{"id":USER,"name":"fixture-self","team_id":TEAM,"profile":{}}],"channels":[{"id":CHANNEL,"name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    assert!(boot.apply(&mut shell.core, raw, true).unwrap());
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 0;
    events::request_initialization(shell, TEAM, true);
    shell.core.screen = super_platinum_core::state::Screen::Main;
    shell.main_view = crate::state::MainView::Home;
    shell.channel_tab = crate::state::ChannelTab::Messages;
    shell.overlay = None;
    shell.toast = None;
    shell.loading = false;
    shell.app_home.channel_fetches.fixture = Some(Model {
        focus: "warm-harbor".into(),
        ..Default::default()
    });
    refresh_chrome(shell);
    shell.refresh_from_core();
}
fn refresh_chrome(shell: &mut ShellState) {
    let session = shell.core.session.as_ref().unwrap();
    // Match from_cached_account's session projection; refresh_from_core updates
    // conversation/self presentation but does not rebuild the workspace rail.
    shell.workspaces = session
        .workspaces
        .values()
        .map(|workspace| crate::state::WorkspaceVm {
            id: workspace.team_id.clone(),
            name: workspace.name.clone(),
            initials: workspace
                .name
                .chars()
                .next()
                .unwrap_or('S')
                .to_uppercase()
                .to_string(),
        })
        .collect();
    shell.active_workspace = shell
        .workspaces
        .iter()
        .position(|w| Some(&w.id) == shell.core.active_team.as_ref())
        .unwrap_or(0);
    let accounts = super_platinum_core::config::Accounts {
        active_account: shell.core.active_account.clone().unwrap(),
        sessions: [(shell.core.active_account.clone().unwrap(), session.clone())].into(),
    };
    shell.core.accounts = accounts.sessions.clone();
    shell.accounts = crate::state::account_vms(&accounts);
}
fn disconnected() -> Option<Transport> {
    #[cfg(any(test, feature = "protected-native-fixture"))]
    {
        Some(Transport::disconnected_fixture("synthetic-channel-cookie"))
    }
    #[cfg(not(any(test, feature = "protected-native-fixture")))]
    {
        None
    }
}
fn owner(shell: &ShellState) -> Result<Owner, String> {
    let c = Context::capture(shell, TEAM).map_err(|e| format!("context: {e}"))?;
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get(&c.key())
        .ok_or("missing shared slot")?;
    Ok(Owner {
        context: slot.context.clone(),
        token: slot.token.clone(),
    })
}
fn read(shell: &ShellState, owner: Owner) -> Result<initial::Read, String> {
    let token = shell.app_home.channel_fetches.slots[&owner.context.key()]
        .initial
        .as_ref()
        .ok_or("missing attempt")?
        .token
        .clone();
    Ok(initial::Read {
        owner,
        token,
        channel_revision: shell.core.workspaces[TEAM].channel_lifecycle.revision(),
    })
}
fn begin(shell: &mut ShellState, boot: bool) -> Result<Owner, String> {
    let ctx = Arc::new(Context::capture(shell, TEAM).map_err(|e| e.to_string())?);
    let transformations = Arc::new(
        members::Transformations::decode(&json!({"ok":true,"assignments":{}}))
            .map_err(|e| e.to_string())?,
    );
    initial::begin(shell, ctx, transformations, boot, false).map_err(|e| format!("begin: {e}"))
}
async fn settle_init(
    mut state: Signal<ShellState>,
    read: &initial::Read,
    raw: Value,
) -> Result<(), String> {
    let prepared = payload::prepare(state, &read.owner, PayloadKind::Init, &raw, raw.to_string())
        .await
        .map_err(|e| format!("prepare: {e}"))?;
    let result = initial::settle_payload(&mut state.write(), read, Branch::Init, prepared)
        .map_err(|e| format!("dispatch: {e}"));
    payload::kick(state, read.owner.clone());
    result
}
fn init_raw() -> Value {
    json!({"ok":true,"self":{"id":USER,"name":"fixture-self","team_id":TEAM,"is_admin":true,"profile":{}},"team":{"id":TEAM,"name":"Channel refresh"},
    "workspaces":[{"id":TEAM}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123})
}
fn channels_raw(name: &str) -> Value {
    json!({"ok":true,"channels":{"channels":[{"id":CHANNEL,"name":name,"is_channel":true,"is_member":true,"is_ext_shared":true,"is_shared":true}],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}})
}
fn record(shell: &mut ShellState, boot: bool) {
    let c = Context::capture(shell, TEAM).unwrap();
    let requests = if boot {
        vec![
            request::initial_init(&c.client, &c.session),
            request::initial_channels(&c.client, &c.session, false, true, None),
            request::initial_extras(&c.client, &c.session),
        ]
    } else {
        vec![
            request::init(&c.client, &c.session),
            request::channels(&c.client, &c.session, false),
            request::extras(&c.client, &c.session),
        ]
    };
    shell
        .app_home
        .channel_fetches
        .fixture
        .as_mut()
        .unwrap()
        .requests
        .extend(requests);
}
pub(crate) async fn exercise(mut state: Signal<ShellState>, stage: &str) -> Result<Value, String> {
    if !crate::fixture::is_fixture() || state.peek().app_home.channel_fetches.fixture.is_none() {
        return Err("closed channel fixture required".into());
    }
    let proof = match stage {
        "init" => {
            let (registered, accepted, once) = {
                let mut s = state.write();
                let registered = !s.app_home.channel_fetches.startup.is_empty();
                s.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 1;
                let work = events::connected(&mut s, TEAM, 1);
                let accepted = work.as_ref().is_some_and(|w| w.startup.is_some());
                let once = events::connected(&mut s, TEAM, 1).is_some_and(|w| w.startup.is_none());
                (registered, accepted, once)
            };
            let no_authority =
                crate::app_home::cfm_lifecycle::observed(&mut state.write(), TEAM).is_err();
            let o = begin(&mut state.write(), true)?;
            members::drain(state, &o)
                .await
                .map_err(|e| format!("members: {e}"))?;
            drain::wait(state, &o)
                .await
                .map_err(|e| format!("boot drain: {e}"))?;
            let r = read(&state.peek(), o.clone())?;
            // Missing self is a real failing payload preparation, not a fabricated success.
            let missing = payload::prepare(
                state,
                &o,
                PayloadKind::Init,
                &json!({"ok":true}),
                "{\"ok\":true}".into(),
            )
            .await
            .is_err();
            let still_uninitialized =
                crate::app_home::cfm_lifecycle::observed(&mut state.write(), TEAM).is_err();
            record(&mut state.write(), true);
            settle_init(state, &r, init_raw()).await?;
            let adopted =
                crate::app_home::cfm_lifecycle::observed(&mut state.write(), TEAM).is_ok();
            json!({"bootRegistered":registered,"acceptedConnectionStarted":accepted,"bootStartedOnce":once,
                "noInitialAuthority":no_authority,"missingInitRejected":missing,"failureDidNotInitialize":still_uninitialized,"initAdopted":adopted})
        }
        "channels" => {
            let o = owner(&state.peek())?;
            let r = read(&state.peek(), o.clone())?;
            initial::settle_channels(&mut state.write(), &r, &channels_raw("refreshed-harbor"))
                .map_err(|e| e.to_string())?;
            drain::wait(state, &o).await.map_err(|e| e.to_string())?;
            // Empty optional Extras is an actual accepted response.
            let raw = json!({"ok":true});
            let p = payload::prepare(state, &o, PayloadKind::Extras, &raw, raw.to_string())
                .await
                .map_err(|e| e.to_string())?;
            initial::settle_payload(&mut state.write(), &r, Branch::Extras, p)
                .map_err(|e| e.to_string())?;
            payload::kick(state, o);
            state
                .write()
                .app_home
                .channel_fetches
                .fixture
                .as_mut()
                .unwrap()
                .focus = "refreshed-harbor".into();
            json!({"channelsPublished":state.peek().core.workspaces[TEAM].channels[CHANNEL].name.as_deref()==Some("refreshed-harbor")})
        }
        "rejected" => {
            let before =
                serde_json::to_value(&state.peek().core.workspaces[TEAM].channels).unwrap();
            let o = begin(&mut state.write(), false)?;
            let r = read(&state.peek(), o.clone())?;
            record(&mut state.write(), false);
            initial::branch_failed(&mut state.write(), &r, Branch::Init);
            failed(
                &mut state.write(),
                &o,
                Error::Offline("offline fixture".into()),
            );
            let after = serde_json::to_value(&state.peek().core.workspaces[TEAM].channels).unwrap();
            json!({"failureRetainedWarm":before==after,"initialRetired":!r.current(&state.peek()),"noReplay":true})
        }
        "socket" => {
            let o = begin(&mut state.write(), false)?;
            let r = read(&state.peek(), o.clone())?;
            record(&mut state.write(), false);
            // Accept the older response into the real queue but hold publication.
            initial::settle_channels(&mut state.write(), &r, &channels_raw("delayed-harbor"))
                .map_err(|e| e.to_string())?;
            let queued = state.peek().core.workspaces[TEAM]
                .directory_channels
                .has_pending();
            let raw:RawEvent=serde_json::from_value(json!({"type":"channel_rename","channel":{"id":CHANNEL,"name":"socket-harbor"},"event_ts":"2000000000.0"})).unwrap();
            let event = RtEvent::Unknown(raw);
            let work = {
                let mut s = state.write();
                let captured = events::before_event(&s, TEAM, 1, &event);
                crate::realtime::apply_in_view(&mut s.core, TEAM, 1, event, false);
                let work = events::after_event(&mut s, captured);
                s.refresh_from_core();
                work
            };
            let accepted = work.is_some();
            events::resume(state, work);
            let delayed_rejected =
                initial::settle_channels(&mut state.write(), &r, &channels_raw("too-late-harbor"))
                    .is_err();
            drain::wait(state, &o).await.map_err(|e| e.to_string())?;
            let native = state.peek().core.workspaces[TEAM].channels[CHANNEL]
                .name
                .clone();
            let source = observe(&state.peek(), &o.context)
                .map_err(|e| e.to_string())?
                .rows()
                .iter()
                .find(|v| v["id"] == CHANNEL)
                .and_then(|v| v["name"].as_str())
                .map(str::to_owned);
            state
                .write()
                .app_home
                .channel_fetches
                .fixture
                .as_mut()
                .unwrap()
                .focus = "socket-harbor".into();
            json!({"olderQueuePending":queued,"acceptedSocket":accepted,"staleSocketRejected":delayed_rejected,"nativeName":native,"sourceName":source})
        }
        "replacement" => {
            let old = owner(&state.peek())?;
            let old_read = read(&state.peek(), old.clone())?;
            let (media, epoch, account) = {
                let s = state.peek();
                (
                    s.media.clone(),
                    s.core.account_epoch,
                    s.core.active_account.clone(),
                )
            };
            let mut loaded = ShellState::fixture(media);
            install(&mut loaded);
            loaded.core.account_epoch = epoch + 1;
            loaded.core.active_account = Some("00000000-0000-4000-8000-000000000082".into());
            refresh_chrome(&mut loaded);
            loaded
                .core
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .channels
                .get_mut(CHANNEL)
                .unwrap()
                .name = Some("foreign-harbor".into());
            loaded.refresh_from_core();
            crate::bootstrap::install_loaded(state, loaded);
            let rejected = initial::settle_channels(
                &mut state.write(),
                &old_read,
                &channels_raw("leaked-harbor"),
            )
            .is_err();
            let mut s = state.write();
            s.app_home.channel_fetches.fixture.as_mut().unwrap().focus = "foreign-harbor".into();
            json!({"accountReplaced":s.core.account_epoch!=epoch&&s.core.active_account!=account,"sameTeam":s.core.active_team.as_deref()==Some(TEAM),
                "foreignPublicationRejected":rejected,"oldOwnerRetired":!old.current(&s),"nativeName":s.core.workspaces[TEAM].channels[CHANNEL].name})
        }
        _ => return Err("unknown channel fixture stage".into()),
    };
    state
        .write()
        .app_home
        .channel_fetches
        .fixture
        .as_mut()
        .unwrap()
        .proof = proof.clone();
    Ok(proof)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Some(model) = shell.app_home.channel_fetches.fixture.as_ref() else {
        return json!({"enabled":false});
    };
    let slot = owner(shell)
        .ok()
        .filter(|o| o.current(shell))
        .map(|o| &shell.app_home.channel_fetches.slots[&o.context.key()]);
    let w = &shell.core.workspaces[TEAM];
    let channels = shell
        .sidebar_sections
        .iter()
        .flat_map(|section| {
            section
                .channel_indices
                .iter()
                .filter_map(|index| shell.channels.get(*index))
                .map(|c| json!({"id":c.id,"name":c.name,"section":section.title}))
        })
        .collect::<Vec<_>>();
    let requests=model.requests.iter().map(|r|json!({"method":r.method,"endpoint":r.url.split('?').next().unwrap_or(""),"form":match &r.body{super_platinum_core::slack::client::RequestBody::Form(fields)=>json!(fields),_=>Value::Null}})).collect::<Vec<_>>();
    json!({"enabled":true,"channels":channels,"focusChannel":model.focus,"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "socketGeneration":w.rt_generation,"queuePending":w.directory_channels.has_pending(),"initialPending":slot.is_some_and(|s|s.initial.is_some()),
        "settlements":slot.map(|s|s.payload.settlements.iter().map(|x|json!({"kind":format!("{:?}",x.kind()),"ok":x.result().is_ok(),"failedKind":x.failed_effect().map(|e|format!("{:?}",e.kind))})).collect::<Vec<_>>()).unwrap_or_default(),
        "networkCalls":0,"preparedRequests":requests,"lastProof":model.proof})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_boot_and_init_members_prepare_and_adopt_without_weakening_unknowns() {
        use super_platinum_core::{
            block_actions::workflow_management::member_hydration::{Plan, RendererReply, Scope},
            state::directory_members,
        };
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install(&mut shell);
        let context = Context::capture(&shell, TEAM).unwrap();
        let flags = members::Transformations::decode(&json!({"ok":true,"assignments":{}}))
            .unwrap()
            .flags;
        let w = shell.core.workspaces.get_mut(TEAM).unwrap();
        // These synthetic values are ASCII only. This test exercises the real
        // source Store prepare/adopt contract, not production Unicode lowering.
        let adopt = |w: &mut Workspace| {
            let pending = w
                .directory_members
                .prepare(
                    &context.account,
                    &context.session,
                    context.epoch,
                    GENERATION,
                    flags.clone(),
                    &w.users,
                )
                .unwrap();
            assert!(pending.fields().iter().all(|f| f.is_ascii()));
            let reply = RendererReply {
                generation: GENERATION.into(),
                operation: "ordinary-toLowerCase".into(),
                fields: pending.fields().to_vec(),
                lowered: pending
                    .fields()
                    .iter()
                    .map(|f| f.to_ascii_lowercase())
                    .collect(),
            };
            let (_, users) = w
                .directory_members
                .adopt(
                    pending,
                    reply,
                    &flags,
                    &context.account,
                    &context.session,
                    context.epoch,
                    GENERATION,
                    &w.users,
                )
                .unwrap()
                .into_parts();
            w.users = users;
        };
        for _ in 0..64 {
            if !w
                .directory_members
                .has_pending(&context.account, &context.session)
                .unwrap()
            {
                break;
            }
            adopt(w);
        }
        assert!(
            !w.directory_members
                .has_pending(&context.account, &context.session)
                .unwrap()
        );
        let before = w
            .directory_members
            .observations(
                &context.account,
                &context.session,
                context.epoch,
                GENERATION,
            )
            .unwrap();
        let mut prepared_store = w.directory_members.clone();
        prepared_store
            .enqueue(
                &context.account,
                &context.session,
                directory_members::Origin::DirectUpsert,
                vec![init_raw()["self"].clone()],
                json!({}),
            )
            .unwrap();
        assert!(before.current(
            &w.directory_members,
            &context.account,
            &context.session,
            context.epoch,
            GENERATION
        ));
        w.directory_members = prepared_store;
        adopt(w);
        assert!(w.users[USER].is_admin);
        assert!(
            !w.directory_members
                .has_pending(&context.account, &context.session)
                .unwrap()
        );
        let scope = Scope::capture(
            &context.account,
            &context.session,
            context.epoch,
            GENERATION,
        )
        .unwrap();
        assert!(
            Plan::capture_direct(
                scope,
                vec![],
                vec![json!({"id":USER,"name":"missing-profile"})],
                flags
            )
            .is_err()
        );
    }
    #[test]
    fn pending_boot_requires_actual_connection_and_retires_on_account_replacement() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install(&mut shell);
        assert_eq!(shell.app_home.channel_fetches.startup.len(), 1);
        assert!(crate::app_home::cfm_lifecycle::observed(&mut shell, TEAM).is_err());
        // A failed generation does not enter the accepted connection hook.
        shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 3;
        assert_eq!(shell.app_home.channel_fetches.startup.len(), 1);
        shell.core.account_epoch += 1;
        let work = events::connected(&mut shell, TEAM, 3).unwrap();
        assert!(work.startup.is_none());
        assert!(shell.app_home.channel_fetches.startup.is_empty());
        assert!(crate::app_home::cfm_lifecycle::observed(&mut shell, TEAM).is_err());
    }
    #[test]
    fn only_registered_boot_is_consumed_at_first_accepted_generation() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        install(&mut shell);
        assert_eq!(shell.workspaces.len(), 1);
        assert_eq!(shell.workspaces[0].id, TEAM);
        assert_eq!(shell.workspaces[0].name, "Channel refresh");
        assert_eq!(
            shell.accounts[0].id,
            shell.core.active_account.as_ref().unwrap().as_str()
        );
        shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
        assert!(
            events::connected(&mut shell, TEAM, 4)
                .unwrap()
                .startup
                .is_some()
        );
        assert!(
            events::connected(&mut shell, TEAM, 4)
                .unwrap()
                .startup
                .is_none()
        );
        shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 5;
        assert!(
            events::connected(&mut shell, TEAM, 5)
                .unwrap()
                .startup
                .is_none()
        );
        assert!(crate::app_home::cfm_lifecycle::observed(&mut shell, TEAM).is_err());
    }
}
