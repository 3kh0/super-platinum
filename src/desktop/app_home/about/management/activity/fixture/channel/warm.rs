//! Isolated cache reload followed by fresh canonical Boot/Init, never a live cache owner.
use super::*;
use crate::app_home::channel_fetches::{canonical, plan_info};
use std::sync::OnceLock;
use super_platinum_core::cache::{Cache, WorkspaceSnapshot};

pub(crate) const ACCOUNT: &str = "67c1fd64-2911-4d9a-aa50-fd73155a98f8";
const TEAM: &str = "TWORKFLOW";
struct Ledger {
    reload: Value,
    old: Option<plan_info::warm_fixture::Held>,
    display: plan_info::Display,
    actions: Vec<Value>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
pub(crate) fn requested() -> bool {
    cfg!(feature = "protected-native-fixture")
        && crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_PLAN_INFO_WARM_CHECK").as_deref() == Ok("1")
        && case() == "channel-frozen-grace"
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested()
        && super::super::enabled(s)
        && s.core.active_account.as_deref() == Some(ACCOUNT)
        && s.core.active_team.as_deref() == Some(TEAM)
        && s.core.cache.is_none()
        && s.core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(TEAM))
            .is_some_and(|s| {
                s.user_id == "UWORKFLOW"
                    && s.url == "https://example.test"
                    && s.token == "synthetic-home-token"
            })
}
pub(crate) async fn reload(
    mut state: Signal<ShellState>,
    boot: &Value,
    init: &Value,
) -> Result<(), String> {
    if !enabled(&state.peek()) || LEDGER.get().is_some() {
        return Err("Fresh isolated warm fixture required".into());
    }
    let path = std::path::PathBuf::from(
        std::env::var("SUPER_PLATINUM_PLAN_INFO_WARM_DB")
            .map_err(|_| "Private PlanInfo DB missing")?,
    );
    if !path.is_absolute() || path.exists() {
        return Err("New absolute private PlanInfo DB required".into());
    }
    let (original, old, save, session, epoch, saved) = {
        let s = state.peek();
        let original = plan_info::capture(&s, TEAM).ok_or("Original positive PlanInfo absent")?;
        let old = plan_info::warm_fixture::hold(
            &s,
            json!({"ok":true,"plan_info":{"is_plan_frozen":false,"reconnect_grace_period_start_ts":1,"late":true}}),
        )?;
        let w = &s.core.workspaces[TEAM];
        let saved = serde_json::to_value(
            w.cfm_payload
                .plan_info_cache
                .as_ref()
                .ok_or("Accepted cache provenance absent")?,
        )
        .unwrap();
        (
            original,
            old,
            WorkspaceSnapshot::full(w),
            s.core.session.as_ref().unwrap().workspaces[TEAM].clone(),
            s.core.account_epoch,
            saved,
        )
    };
    let db_path = path.clone();
    let session_copy = session.clone();
    let loaded = tokio::task::spawn_blocking(move || -> Result<_, String> {
        {
            let cache = Cache::open(&db_path).map_err(|e| e.to_string())?;
            save.save(&cache).map_err(|e| e.to_string())?;
        }
        let cache = Cache::open(&db_path).map_err(|e| e.to_string())?;
        cache
            .load_workspace_for_startup(&session_copy)
            .map_err(|e| e.to_string())?
            .ok_or("Saved workspace absent".into())
    })
    .await
    .map_err(|e| e.to_string())??;
    let restored = serde_json::to_value(&loaded.cfm_payload.plan_info_cache).unwrap();
    if restored != saved || loaded.cfm_payload.plan_info.is_some() {
        return Err("Private reload changed history or restored live raw".into());
    }
    let foreign =
        super_platinum_core::state::cfm_payload::PlanInfoCache::restore(&saved, &session).unwrap();
    let foreign_account_matches = foreign.matches("c386ef8b-2908-4c2d-817d-9dbb1394866d", &session);
    {
        let mut s = state.write();
        if !enabled(&s) || !original.current(&s) {
            return Err("Original cache save owner retired".into());
        }
        // Before any user draft/read: actual loaded workspace and a new runtime epoch.
        let model = super::model(&s).map_err(|_| "Original closed channel ledger absent")?;
        let mut m = model.0.lock().unwrap();
        if !m.requests.is_empty() || !m.about.is_empty() {
            return Err("Reload must precede Activity reads".into());
        }
        s.core.account_epoch = s
            .core
            .account_epoch
            .checked_add(1)
            .ok_or("Epoch exhausted")?;
        m.epoch = s.core.account_epoch;
        s.core.workspaces.insert(TEAM.into(), loaded);
        s.refresh_from_core();
    }
    // Fresh source, with genuinely missing PlanInfo Extras. This must not mint live capture.
    canonical::fixture_seed_with_extras(state, boot.clone(), init.clone(), json!({"ok":true}))
        .await?;
    let s = state.peek();
    let display = plan_info::display(&s, TEAM).ok_or("Retained current display unavailable")?;
    if plan_info::capture(&s, TEAM).is_some()
        || display.provenance() != "retained-cache"
        || original.current(&s)
        || old.current(&s)
        || foreign_account_matches
    {
        return Err("Reload incorrectly promoted or resurrected source ownership".into());
    }
    LEDGER.set(Mutex::new(Ledger{reload:json!({"path":path,"saved":saved,"restored":restored,
        "saveClosedBeforeLoad":true,"startupLoad":true,"liveRawRestored":false,
        "previousEpoch":epoch,"currentEpoch":s.core.account_epoch,"oldLiveCurrent":original.current(&s),
        "oldSourceCurrent":old.current(&s),"foreignAccountMatches":foreign_account_matches,
        "boot":boot,"init":init,"extras":{"ok":true},"defaultCacheInstalled":false}),old:Some(old),display,actions:vec![]}))
        .map_err(|_|"Duplicate warm reload")?;
    Ok(())
}
pub(crate) fn action(state: Signal<ShellState>, kind: &str) -> Result<(), String> {
    if !enabled(&state.peek()) {
        return Err("Original warm action refused".into());
    }
    let proof = match kind {
        "empty-positive" | "null-positive" => {
            let payload = if kind == "empty-positive" {
                json!({})
            } else {
                json!({"reconnect_grace_period_start_ts":null})
            };
            let held = plan_info::warm_fixture::hold(
                &state.peek(),
                json!({"ok":true,"plan_info":payload}),
            )?;
            held.deliver(state)?
        }
        "foreign-refused" => plan_info::warm_fixture::foreign(state)?,
        "late-refused" => {
            let old = LEDGER
                .get()
                .ok_or("Warm ledger absent")?
                .lock()
                .unwrap()
                .old
                .take()
                .ok_or("Original late envelope spent")?;
            old.deliver(state)?
        }
        _ => return Err("Unknown PlanInfo action".into()),
    };
    LEDGER
        .get()
        .unwrap()
        .lock()
        .unwrap()
        .actions
        .push(json!({"kind":kind,"proof":proof}));
    Ok(())
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    if !requested() {
        return Value::Null;
    }
    let Some(ledger) = LEDGER.get() else {
        return Value::Null;
    };
    let l = ledger.lock().unwrap();
    let display = plan_info::display(s, TEAM);
    let live = plan_info::capture(s, TEAM);
    json!({"reload":l.reload,"actions":l.actions,"oldDisplayCurrent":l.display.current(s),
        "oldSourceCurrent":l.old.as_ref().map(|v|v.current(s)),"account":s.core.active_account,"epoch":s.core.account_epoch,
        "live":live.as_ref().map(|v|json!({"current":v.current(s),"raw":v.raw()})),
        "display":display.as_ref().map(|v|json!({"current":v.current(s),"provenance":v.provenance(),"raw":v.raw()})),
        "retained":s.core.workspaces.get(TEAM).and_then(|w|w.cfm_payload.plan_info_cache.as_ref())})
}
