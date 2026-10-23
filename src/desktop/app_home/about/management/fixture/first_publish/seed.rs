//! Initial raw producer inputs; no ready target observations are fabricated.
use super::*;
use super_platinum_core::config::WorkspaceSession;
const BOT: &str = "UHOME0001";
fn bot_raw() -> Value {
    json!({"id":BOT,"name":"Review requests","real_name":"Review requests","is_bot":true,"profile":{"display_name":"Review requests","real_name":"Review requests"}})
}
fn teams() -> Vec<Value> {
    let mut team = json!({"id":"TWORKFLOW","name":"Home fixture"});
    super::confirmation::share::posting::seed_team(&mut team);
    vec![team]
}
fn preflight_members(
    account: &str,
    session: &WorkspaceSession,
    epoch: u64,
    native: &std::collections::HashMap<String, super_platinum_core::slack::models::User>,
    boot: &Value,
) -> Result<(), String> {
    use super_platinum_core::{
        block_actions::workflow_management::member_hydration::Flags,
        state::directory_members::Store,
    };
    // Validate the actual full native identity set as well as the incoming rows.
    // This temporary store is discarded; only BootRead below publishes authority.
    let mut store = Store::default();
    store
        .observe_boot(account, session, &native.keys().cloned().collect(), boot)
        .map_err(|e| format!("First-Publish member-history input: {e}"))?;
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}}))
        .map_err(|e| format!("First-Publish assignment input: {e}"))?;
    store
        .prepare(
            account,
            session,
            epoch,
            "first-publish-preflight",
            flags,
            native,
        )
        .map_err(|e| format!("First-Publish member-hydration input: {e}"))?;
    Ok(())
}

pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    let model = capture(&state.peek())?;
    if model.0.lock().unwrap().seed.is_some() {
        return Err("First-Publish seed must be original".into());
    }
    {
        let mut s = state.write();
        let original = s
            .core
            .active_channel
            .clone()
            .ok_or("Original app channel absent")?;
        let w = s
            .core
            .workspaces
            .get_mut("TWORKFLOW")
            .ok_or("Fixture workspace absent")?;
        if w.channels.get(&original).and_then(|c| c.user.as_deref()) != Some("U_HOME_BOT") {
            return Err("Original synthetic app bot changed before seed".into());
        }
        w.users
            .remove("U_HOME_BOT")
            .ok_or("Original synthetic bot absent")?;
        w.users.insert(
            BOT.into(),
            serde_json::from_value(bot_raw()).map_err(|_| "Invalid synthetic bot")?,
        );
        w.channels.get_mut(&original).unwrap().user = Some(BOT.into());
        w.users
            .retain(|id, _| matches!(id.as_str(), "UWORKFLOW" | BOT));
        if super::confirmation::requested() {
            // The new Add cases acquire their entire synthetic channel universe
            // below through BootRead/Init. Unrelated display-fixture channels
            // have no raw source receipt and would correctly keep Ready stale.
            w.channels.retain(|id, _| id == &original);
        }
    }
    let (original, mut self_user) = {
        let s = state.peek();
        let w = &s.core.workspaces["TWORKFLOW"];
        (
            serde_json::to_value(&w.channels[s.core.active_channel.as_ref().unwrap()]).unwrap(),
            serde_json::to_value(&w.users["UWORKFLOW"]).unwrap(),
        )
    };
    self_user["team_id"] = json!("TWORKFLOW");
    if !self_user["profile"].is_object() {
        return Err("Original synthetic self profile absent".into());
    }
    let teams = teams();
    let mut users = vec![self_user.clone(), bot_raw()];
    if matches!(
        case().as_str(),
        "first-settings" | "first-settings-rejected-held"
    ) {
        users.push(super::picker::person("UMANOLD01", "Original Manager"));
    }
    let mut channels = vec![original];
    if super::confirmation::requested() {
        channels.extend(super::confirmation::raw::channels());
    }
    super::confirmation::share::dm::seed(&mut users, &mut channels);
    super::confirmation::share::person::seed(&mut users);
    super::confirmation::share::multiple::seed(&mut users, &mut channels);
    super::confirmation::share::search::seed(&mut users);
    let mut boot = json!({"ok":true,"team":teams[0],"self":self_user,"users":users,"channels":channels,"prefs":{}});
    if super::confirmation::share::requested() {
        // Name-policy authority acquires membership from this original Boot
        // response. The separate canonical Init response cannot supply it.
        boot["workspaces"] = json!(teams);
    }
    serde_json::from_value::<super_platinum_core::slack::models::BootData>(boot.clone())
        .map_err(|e| format!("First-Publish Boot shape: {e}"))?;
    {
        let s = state.peek();
        let session = &s.core.session.as_ref().ok_or("Session absent")?.workspaces["TWORKFLOW"];
        preflight_members(
            s.core.active_account.as_deref().ok_or("Account absent")?,
            session,
            s.core.account_epoch,
            &s.core.workspaces["TWORKFLOW"].users,
            &boot,
        )?;
    }
    let init = json!({"ok":true,"self":self_user,"team":teams[0],"workspaces":teams,"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot.clone(), init.clone())
        .await?;
    let mut add_canonical = None;
    if super::confirmation::requested() {
        for _ in 0..120 {
            {
                let s = state.peek();
                if let Ok(rows) =
                    crate::app_home::channel_fetches::canonical::observed(&s, "TWORKFLOW")
                {
                    if rows.current(&s)
                        && super::confirmation::raw::channels().iter().all(|raw| {
                            rows.row(raw["id"].as_str().unwrap())
                                .is_some_and(|v| v["fromAnotherTeam"] == false)
                        })
                    {
                        add_canonical = Some(json!(rows.rows()));
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if add_canonical.is_none() {
            let s = state.peek();
            let w = &s.core.workspaces["TWORKFLOW"];
            let saved = w.directory_channels.cached().unwrap_or(Value::Null);
            let observation = match crate::app_home::channel_fetches::canonical::observed(
                &s,
                "TWORKFLOW",
            ) {
                Ok(rows) => {
                    json!({"current":rows.current(&s),"rows":rows.rows().iter().map(|v|json!({"id":v["id"],"fromAnotherTeam":v["fromAnotherTeam"]})).collect::<Vec<_>>()})
                }
                Err(error) => json!({"error":error.to_string()}),
            };
            let diagnostic = json!({"nativeIds":w.channels.keys().collect::<Vec<_>>(),
                "savedIds":saved["records"].as_array().map(|rows|rows.iter().map(|v|v["source"]["id"].clone()).collect::<Vec<_>>()),
                "pendingIds":saved["pending"].as_array().map(|batches|batches.iter().flat_map(|v|v["rows"].as_array().into_iter().flatten().map(|r|r["id"].clone())).collect::<Vec<_>>()),
                "invalidated":saved["invalidated"],"overflowed":saved["overflowed"],"observation":observation});
            return Err(format!(
                "Original Add canonical channel producer did not settle: {diagnostic}"
            ));
        }
    }
    let canonical = {
        let s = state.peek();
        let rows = crate::app_home::channel_fetches::teams::Observation::capture(&s, "TWORKFLOW")
            .map_err(|_| "Canonical team observation unavailable")?;
        if !rows.current(&s)
            || teams.iter().any(|t| {
                rows.rows()
                    .get(t["id"].as_str().unwrap())
                    .is_none_or(|r| r["enterprise_id"] != t["enterprise_id"])
            })
        {
            return Err("Original canonical team fields not adopted".into());
        }
        json!(rows.rows())
    };
    {
        // One initial setup boundary before a user draft or native lease exists.
        let mut s = state.write();
        let before = (
            s.core.active_account.clone(),
            s.core.active_channel.clone(),
            s.channel_generation,
        );
        s.channel_tab = crate::model::ChannelTab::AppHome;
        let home = crate::app_home::Owner::capture(&s).ok_or("Initial Home recapture refused")?;
        let surface = s.app_home.surface.as_mut().ok_or("Initial Home absent")?;
        surface.id = home.home_visit;
        surface.owner = home;
        s.channel_tab = crate::model::ChannelTab::AppAbout;
        let about =
            crate::app_home::about::Owner::capture(&s).ok_or("Initial About recapture refused")?;
        s.app_home
            .about
            .surface
            .as_mut()
            .ok_or("Initial About absent")?
            .owner = about;
        if before
            != (
                s.core.active_account.clone(),
                s.core.active_channel.clone(),
                s.channel_generation,
            )
        {
            return Err("Seed changed original navigation".into());
        }
    }
    if !model.current(&state.peek()) {
        return Err("First-Publish owner changed during canonical seed".into());
    }
    let mut seed = json!({"boot":boot,"init":init,"canonicalTeams":canonical});
    if let Some(rows) = add_canonical {
        seed["canonicalChannels"] = rows;
        let s = state.peek();
        seed["unifiedUserClientEnabled"] =
            json!(s.core.session.as_ref().unwrap().unified_user_client_enabled);
    }
    model.0.lock().unwrap().seed = Some(seed);
    super::confirmation::share::connectivity::install(&mut state.write())?;
    crate::channel_workflows::details::fixture::thread::install(&mut state.write())?;
    if super::confirmation::share::startup::requested() {
        crate::app_home::autocomplete::startup::schedule(state, "TWORKFLOW");
    }
    Ok(())
}
