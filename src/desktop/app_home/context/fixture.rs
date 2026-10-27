//! Named closed fixture adapter; actual sources are published by shared producers.
use super::*;
use crate::app_home::inputs::external::fixture as external;
use serde_json::json;
pub(crate) mod check;
const BOT: &str = "UHOME0001";
pub(super) const TWO: &str = "TWORK0002";
pub(super) const THREE: &str = "TWORK0003";
pub(super) const INSTALLED: &str = "TINSTALL1";
#[cfg(test)]
pub(in crate::app_home) fn select_override(s: &mut ShellState, team: &str) {
    let account =
        Arc::new(SourceContext::capture(s, s.core.active_team.as_ref().unwrap()).unwrap());
    let channel = s.core.active_channel.clone().unwrap();
    s.app_home.context.overrides.insert(
        key(&account, &channel),
        Override {
            account,
            team: team.into(),
            revision: next(),
        },
    );
}
pub(crate) struct ActionGate {
    waiting: bool,
    release: Option<tokio::sync::oneshot::Sender<()>>,
}
pub(super) fn hold_action(s: &mut ShellState) {
    s.app_home.context.fixture_action = Some(ActionGate {
        waiting: false,
        release: None,
    });
}
/// A held native lease is test scheduling, not route authority. The normal
/// action path rechecks its original witness after this closed fixture pause.
pub(crate) async fn after_native_lock(mut state: Signal<ShellState>) {
    if !named() {
        return;
    }
    let receiver = {
        let mut s = state.write();
        s.app_home.context.fixture_action.as_mut().map(|gate| {
            gate.waiting = true;
            let (tx, rx) = tokio::sync::oneshot::channel();
            gate.release = Some(tx);
            rx
        })
    };
    if let Some(receiver) = receiver {
        let _ = tokio::time::timeout(Duration::from_secs(8), receiver).await;
    }
}
pub(crate) fn action_finished(s: &mut ShellState, admitted: bool) {
    if named() {
        s.app_home
            .context
            .fixture_actions
            .push(json!({"admitted":admitted,"afterNativeLock":true}));
    }
}
pub(super) fn release_action(s: &mut ShellState) -> bool {
    s.app_home
        .context
        .fixture_action
        .take()
        .and_then(|mut gate| gate.release.take())
        .is_some_and(|tx| tx.send(()).is_ok())
}
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_APP_HOME_UNIFIED_CONTEXT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-unified-context" | "app-home-unified-context-narrow")
        )
        && matches!(
            case().as_str(),
            "selection" | "failure" | "retirement" | "empty"
        )
}
pub(super) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_UNIFIED_CONTEXT_CASE").unwrap_or_default()
}

pub(crate) fn install(s: &mut ShellState) {
    // This source-backed fixture needs real member-shaped IDs before the Home
    // owner/session is captured. The generic visual fixture's U0 is not one.
    let team = s.core.active_team.clone().unwrap();
    s.core.workspaces.get_mut(&team).unwrap().self_user_id = "UHOMESELF".into();
    external::install(s);
    // Discard only this disconnected fixture's visual-only typed users. They
    // have no source history and include short/underscore IDs. Keep the outer
    // Home's bot eligibility; the actual Boot below supplies source authority.
    // In the empty case this typed bot deliberately remains unobserved.
    let channel = s.core.active_channel.clone().unwrap();
    let w = s.core.workspaces.get_mut(&team).unwrap();
    w.users = [(BOT.into(), serde_json::from_value(bot()).unwrap())].into();
    // Canonical channel Ready requires source coverage of the native inventory.
    // The generic visual sidebar has unrelated channels with no supplied wire
    // rows; this closed source fixture supplies only its Home conversation.
    w.channels.retain(|id, _| id == &channel);
    w.channels.get_mut(&channel).unwrap().user = Some(BOT.into());
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team.clone(), true);
    let h = s.app_home.surface.as_mut().unwrap();
    h.home.as_mut().unwrap().installed_team = INSTALLED.into();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["app_installed_team_id"] = json!(INSTALLED);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.opened = false;
    inputs::hydrate(s);
    let session = s.core.session.as_ref().unwrap().workspaces[&team].clone();
    let transport = s.core.transport.clone().unwrap();
    let boot = super_platinum_core::state::posting_metadata::authority::BootRead::begin_full(
        &mut s.core,
        &transport,
        &session,
    )
    .unwrap();
    let raw = boot_raw(s);
    assert!(boot.apply(&mut s.core, raw, true).unwrap());
    assert!(
        s.core.workspaces[&team]
            .directory_members
            .has_pending(s.core.active_account.as_deref().unwrap(), &session)
            .unwrap()
    );
    channel_fetches::request_initialization(s, &team, true);
    s.refresh_from_core();
}
fn bot() -> Value {
    json!({"id":BOT,"name":"Release reviews","is_bot":true,"profile":{},"enterprise_user":{"teams":[TWO,THREE]}})
}
fn self_row(s: &ShellState) -> Value {
    let team = s.core.active_team.as_ref().unwrap();
    json!({"id":s.core.workspaces[team].self_user_id,"name":"fixture-self","team_id":team,"profile":{}})
}
fn boot_raw(s: &ShellState) -> Value {
    let team = s.core.active_team.as_ref().unwrap();
    let (users, channels) = if case() == "empty" {
        (json!([self_row(s)]), json!([]))
    } else {
        (
            json!([self_row(s), bot()]),
            json!([{"id":s.core.active_channel,"is_im":true,"is_member":true,"user":BOT,"name":"Release reviews"}]),
        )
    };
    let mut channels = channels;
    if crate::app_home::inputs::channels::fixture::requested() {
        channels
            .as_array_mut()
            .unwrap()
            .extend(crate::app_home::inputs::channels::fixture::channels());
    }
    let mut users = users;
    if crate::app_home::inputs::users::fixture::requested()
        || crate::app_home::inputs::users::dispatch::fixture::requested()
    {
        users
            .as_array_mut()
            .unwrap()
            .extend(crate::app_home::inputs::users::fixture::members());
    }
    if crate::app_home::inputs::channels::conversation::fixture::requested() {
        users
            .as_array_mut()
            .unwrap()
            .extend(crate::app_home::inputs::channels::conversation::fixture::members());
    }
    json!({"ok":true,"team":{"id":team,"name":"Home fixture"},"self":self_row(s),"users":users,"channels":channels,"prefs":{}})
}

pub(super) fn init_raw(s: &ShellState, empty: bool, selected: &str) -> Value {
    json!({"ok":true,"team":{"id":s.core.active_team,"name":"Home fixture"},"self":self_row(s),
        "workspaces":[{"id":TWO,"name":"Design workspace","allow_external_users":empty},{"id":THREE,"name":"Engineering workspace","allow_external_users":empty}],
        "prefs":{"workspace_filter_selected":selected}})
}
pub(crate) async fn publish(
    state: Signal<ShellState>,
    empty: bool,
    selected: &str,
) -> Result<Value, String> {
    let (team, init) = {
        let s = state.peek();
        (
            s.core.active_team.clone().unwrap(),
            init_raw(&s, empty, selected),
        )
    };
    let extras = json!({"ok":true,"default_workspace":THREE});
    channel_fetches::fixture::unified::publish(state, &team, init.clone(), extras.clone()).await?;
    if !empty {
        // Exercise the actual post-drain full source proof, not native presence
        // or a copied raw row. Known-empty context intentionally needs no DM.
        let canonical = channel_fetches::canonical::observed(&state.peek(), &team)
            .map_err(|e| format!("Unified fixture canonical source did not settle: {e}"))?;
        if !canonical.current(&state.peek()) {
            return Err("Unified fixture canonical source retired after drain".into());
        }
    }
    Ok(json!({"init":init,"extras":extras}))
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let route = Witness::capture(s).ok();
    let choice = Choice::capture(s).ok().flatten();
    json!({"case":case(),"route":route.as_ref().map(|w|w.proof(s)),
        "choices":choice.as_ref().map(|c|&c.rows),"choiceCurrent":choice.as_ref().is_some_and(|c|c.current(s)),
        "external":external::snapshot(s),"opens":s.app_home.context.fixture_opens,
        "actionGateWaiting":s.app_home.context.fixture_action.as_ref().is_some_and(|v|v.waiting),"actionResults":s.app_home.context.fixture_actions,"sourceReads":s.app_home.context.fixture_source_reads,
        "boot":boot_raw(s),"init":init_raw(s,case()=="empty",TWO)})
}
/// No-network interceptor reached only after production ensure/current/opened.
pub(crate) fn accept_open(
    s: &mut ShellState,
    route: &Witness,
    request: super_platinum_core::slack::PreparedRequest,
) -> Result<Value, Error> {
    if !(named() || crate::app_home::inputs::external::multi_fixture::unified())
        || !route.current(s)
    {
        return Err(unavailable());
    }
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        return Err(unavailable());
    };
    let proof = json!({"fields":fields.into_iter().collect::<BTreeMap<_,_>>(),"method":request.method,"endpoint":request.url.split('?').next(),"route":route.proof(s)});
    s.app_home.context.fixture_opens.push(proof);
    Ok(json!({"ok":true}))
}
pub(crate) async fn opened(state: Signal<ShellState>) -> Result<Value, String> {
    let (owner, id, revision, home, before) = {
        let s = state.peek();
        let h = s.app_home.surface.as_ref().ok_or("No Home")?;
        (
            h.owner.clone(),
            h.id,
            h.revision,
            h.home.clone().ok_or("No Home source")?,
            s.app_home.context.fixture_opens.len(),
        )
    };
    crate::app_home::notify_opened(state, owner, id, revision, home).await;
    let s = state.peek();
    if s.app_home.context.fixture_opens.len() != before + 1 {
        return Err("Genuine Home-open was not admitted exactly once".into());
    }
    Ok(s.app_home.context.fixture_opens.last().unwrap().clone())
}
pub(super) async fn return_home(
    mut state: Signal<ShellState>,
    detached: external::Detached,
) -> Result<Value, String> {
    {
        let mut s = state.write();
        external::return_home(&mut s, detached);
        s.app_home.surface.as_mut().unwrap().opened = false;
    }
    opened(state).await
}

/// Cached Home metadata is the closed fixture read adapter. Actual Refresh still
/// enters production notify_opened/ensure and its final one-visit dispatch guard.
pub(crate) async fn refresh_cached(state: Signal<ShellState>) {
    let captured = {
        let s = state.peek();
        s.app_home
            .surface
            .as_ref()
            .filter(|h| {
                h.fixture
                    && h.owner.current(&s)
                    && !h.loading
                    && s.overlay.is_none()
                    && !workflows::locked(&s)
                    && !cooling(&s, &h.owner.ctx)
            })
            .and_then(|h| {
                h.home
                    .clone()
                    .map(|home| (h.owner.clone(), h.id, h.revision, home))
            })
    };
    if let Some((owner, id, revision, home)) = captured {
        crate::app_home::notify_opened(state, owner, id, revision, home).await;
    }
}
pub(super) async fn acquire_sources(mut state: Signal<ShellState>) -> Result<(), String> {
    if !named() {
        return Err("Closed source read unavailable".into());
    }
    if !state.peek().app_home.context.fixture_source_ready {
        state
            .write()
            .app_home
            .context
            .fixture_source_reads
            .push(json!({"result":"offline","published":false}));
        return Err("Closed source acquisition offline".into());
    }
    let proof = publish(state, case() == "empty", TWO).await?;
    state
        .write()
        .app_home
        .context
        .fixture_source_reads
        .push(json!({"result":"accepted","ingress":proof}));
    Ok(())
}
