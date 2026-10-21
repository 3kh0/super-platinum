//! Initial raw producer inputs; no ready target observations are fabricated.
use super::*;
use super_platinum_core::config::WorkspaceSession;
const BOT: &str = "UHOME0001";
fn bot_raw() -> Value {
    json!({"id":BOT,"name":"Review requests","real_name":"Review requests","is_bot":true,"profile":{"display_name":"Review requests","real_name":"Review requests"}})
}
pub(super) fn person() -> Value {
    json!({"id":"URUNADA01","name":"ada_runner","real_name":"Ada Runner","team_id":"TWORKFLOW","updated":101,"deleted":false,"is_bot":false,"is_restricted":false,"is_ultra_restricted":false,"is_stranger":false,"profile":{"real_name":"Ada Runner","real_name_normalized":"Ada Runner","display_name":"Ada Runner","display_name_normalized":"Ada Runner"}})
}
pub(super) fn channel() -> Value {
    json!({"id":"CRUNTEAM1","name":"run-team","is_channel":true,"is_private":false,"is_archived":false,"is_member":true,"is_im":false,"is_mpim":false})
}
pub(super) fn teams() -> Vec<Value> {
    vec![
        json!({"id":"TWORKFLOW","name":"Home fixture","enterprise_id":"ERUNORG01"}),
        json!({"id":"TRUNINNER","name":"Run internal","enterprise_id":"ERUNORG01"}),
        json!({"id":"TRUNEXTER","name":"Run partner","enterprise_id":"ERUNOTHER"}),
    ]
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
        .map_err(|e| format!("Run-access member-history input: {e}"))?;
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}}))
        .map_err(|e| format!("Run-access assignment input: {e}"))?;
    store
        .prepare(
            account,
            session,
            epoch,
            "run-access-preflight",
            flags,
            native,
        )
        .map_err(|e| format!("Run-access member-hydration input: {e}"))?;
    Ok(())
}

pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    let model = capture(&state.peek())?;
    if model.0.lock().unwrap().seed.is_some() {
        return Err("Run-access seed must be original".into());
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
    let boot = json!({"ok":true,"team":teams[0],"self":self_user,"users":[self_user,bot_raw(),person()],"channels":[original,channel()],"prefs":{}});
    serde_json::from_value::<super_platinum_core::slack::models::BootData>(boot.clone())
        .map_err(|e| format!("Run-access Boot shape: {e}"))?;
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
    model.0.lock().unwrap().seed =
        Some(json!({"boot":boot,"init":init,"canonicalTeams":canonical}));
    Ok(())
}
