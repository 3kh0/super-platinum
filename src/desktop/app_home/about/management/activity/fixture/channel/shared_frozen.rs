//! Raw shared/frozen inputs through the actual canonical Boot/Init/Extras path.
use super::*;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static SEED: OnceLock<Value> = OnceLock::new();
const BOT: &str = "UHOME0001";
fn bot_raw() -> Value {
    json!({"id":BOT,"name":"Review requests","real_name":"Review requests","is_bot":true,
        "profile":{"display_name":"Review requests","real_name":"Review requests"}})
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
        .map_err(|e| format!("Shared/frozen member-history input: {e}"))?;
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}}))
        .map_err(|e| format!("Shared/frozen assignment input: {e}"))?;
    store
        .prepare(
            account,
            session,
            epoch,
            "shared-frozen-preflight",
            flags,
            native,
        )
        .map_err(|e| format!("Shared/frozen member-hydration input: {e}"))?;
    Ok(())
}
pub(crate) fn requested() -> bool {
    matches!(
        case().as_str(),
        "channel-shared-frozen" | "channel-frozen-grace"
    )
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested()
        && super::super::enabled(s)
        && s.core.active_account.as_deref()
            == Some(if super::warm::requested() {
                super::warm::ACCOUNT
            } else {
                "synthetic-workflow-activity-channel"
            })
        && s.core.active_team.as_deref() == Some("TWORKFLOW")
        && s.core
            .session
            .as_ref()
            .and_then(|v| v.workspaces.get("TWORKFLOW"))
            .is_some_and(|v| {
                v.user_id == "UWORKFLOW"
                    && v.url == "https://example.test"
                    && v.token == "synthetic-home-token"
            })
}
pub(crate) fn grace() -> bool {
    case() == "channel-frozen-grace"
}
/// ID, raw name, expected label, expected official icon.
pub(crate) fn specs() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    specs_for(grace())
}
fn specs_for(grace: bool) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    if grace {
        vec![
            ("CGRPRIV01", "grace-private", "grace-private", "LockFilled"),
            ("CGRARCH01", "grace-archived", "grace-archived", "Tag"),
            (
                "CGRPRAR01",
                "grace-private-archived",
                "grace-private-archived",
                "LockFilled",
            ),
            (
                "CGRNOGT01",
                "grace-public-no-gate",
                "grace-public-no-gate",
                "Archive",
            ),
        ]
    } else {
        vec![
            (
                "CSHPRIV01",
                "shared-private",
                "shared-private",
                "LockFilled",
            ),
            (
                "CSHARCH01",
                "shared-archived",
                "shared-archived (archived)",
                "Archive",
            ),
            ("CFRZACT01", "frozen-active", "frozen-active", "Archive"),
            (
                "CFRZPRV01",
                "frozen-private",
                "frozen-private",
                "ArchiveFilled",
            ),
        ]
    }
}
fn rows(start: u64, grace: bool) -> Vec<Value> {
    let mut rows = vec![raw(KNOWN, "activity-known")];
    rows.extend(
        specs_for(grace)
            .iter()
            .map(|(id, name, _, _)| raw(id, name)),
    );
    if grace {
        for row in &mut rows {
            row["is_frozen"] = json!(true);
            row["unlinked"] = json!(start);
        }
        for index in [0, 2] {
            rows[index]["grace_period_end"] = json!(1);
        }
        for index in [1, 3] {
            rows[index]["is_private"] = json!(true);
        }
        for index in [2, 3] {
            rows[index]["is_archived"] = json!(true);
        }
    } else {
        for row in rows.iter_mut().take(3) {
            row["is_ext_shared"] = json!(true);
            row["is_shared"] = json!(true);
            row["is_org_shared"] = json!(true);
            row["connected_teams"] = json!([{"id":"TWORKFLOW"}, {"id":"TPARTNER"}]);
        }
        rows[1]["is_private"] = json!(true);
        rows[2]["is_archived"] = json!(true);
        rows[3]["is_frozen"] = json!(true);
        rows[3]["unlinked"] = json!(start);
        rows[4]["is_frozen"] = json!(true);
        rows[4]["is_private"] = json!(true);
    }
    rows
}
pub(crate) fn messages(row: &mut Value) {
    let vectors = specs();
    let variables: serde_json::Map<_, _> = vectors
        .iter()
        .enumerate()
        .map(|(i, (id, _, _, _))| (format!("channel{i}"), json!({"type":"CHANNEL","value":id})))
        .collect();
    row["step_events"].as_array_mut().unwrap().push(json!({
        "step_execution_id":"STEP_SHARED_FROZEN", "function_id":"FN_REVIEW",
        "last_updated":1781524800000_i64,"step_status":"SUCCESS",
        "outcome_message":{"template":"Open {channel0} then {channel1} then {channel2} then {channel3}","variables":variables},
        "future":{"preserved":19}}));
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !enabled(&state.peek()) || SEED.get().is_some() {
        return Err("Fresh closed shared/frozen seed required".into());
    }
    let start = if super::grace_clock::requested() {
        super::grace_clock::START_SECONDS
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Clock before epoch")?
            .as_secs()
            .checked_sub(3600)
            .ok_or("Clock too early")?
    };
    {
        // New cases alone traverse strict canonical member hydration. The old
        // fixture-only U_HOME_BOT identity is not a supported member wire ID.
        // Normalize initial synthetic input before any draft/native admission.
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
        let bot = serde_json::from_value(bot_raw()).map_err(|_| "Invalid synthetic bot")?;
        w.users.insert(BOT.into(), bot);
        w.channels.get_mut(&original).unwrap().user = Some(BOT.into());
        // Store::begin validates all native IDs, including warm rows absent from
        // this Boot payload. Unrelated short-ID display fixtures are not source
        // members; this new case starts with only its two original valid users.
        w.users
            .retain(|id, _| matches!(id.as_str(), "UWORKFLOW" | BOT));
    }
    let (original, self_user, users) = {
        let s = state.peek();
        let w = &s.core.workspaces["TWORKFLOW"];
        let self_user = serde_json::to_value(&w.users["UWORKFLOW"]).unwrap();
        if !self_user["profile"].is_object() {
            return Err("Original synthetic self has no member profile object".into());
        }
        (
            serde_json::to_value(&w.channels[s.core.active_channel.as_ref().unwrap()]).unwrap(),
            self_user.clone(),
            vec![self_user, bot_raw()],
        )
    };
    let mut channels = rows(start, grace());
    channels.push(original);
    let boot = json!({"ok":true,"team":{"id":"TWORKFLOW","name":"Shared frozen fixture"},
        "self":self_user,"users":users,"channels":channels,"prefs":{}});
    // Fail with the concrete fixture-shape error before starting Boot ownership.
    // This validates input only; actual publication still uses BootRead below.
    serde_json::from_value::<super_platinum_core::slack::models::BootData>(boot.clone())
        .map_err(|e| format!("Shared/frozen fixture Boot shape: {e}"))?;
    {
        let s = state.peek();
        let session = &s
            .core
            .session
            .as_ref()
            .ok_or("Fixture session absent")?
            .workspaces["TWORKFLOW"];
        preflight_members(
            s.core
                .active_account
                .as_deref()
                .ok_or("Fixture account absent")?,
            session,
            s.core.account_epoch,
            &s.core.workspaces["TWORKFLOW"].users,
            &boot,
        )?;
    }
    let init = json!({"ok":true,"self":self_user,"team":{"id":"TWORKFLOW","name":"Shared frozen fixture"},
        "workspaces":[{"id":"TWORKFLOW"}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    let extras = json!({"ok":true,"plan_info":{"is_plan_frozen":true,"reconnect_grace_period_start_ts":start}});
    crate::app_home::channel_fetches::canonical::fixture_seed_with_extras(
        state,
        boot.clone(),
        init.clone(),
        extras.clone(),
    )
    .await?;
    {
        let s = state.peek();
        let plan = crate::app_home::channel_fetches::plan_info::capture(&s, "TWORKFLOW")
            .ok_or("Genuine Extras PlanInfo was not published")?;
        if !plan.current(&s) || plan.raw() != &extras["plan_info"] {
            return Err("Original Extras PlanInfo differs from current observation".into());
        }
        let canonical = crate::app_home::channel_fetches::canonical::observed(&s, "TWORKFLOW")
            .map_err(|_| "Canonical shared/frozen seed unavailable")?;
        for raw in rows(start, grace()) {
            if canonical
                .row(raw["id"].as_str().unwrap())
                .is_none_or(|row| row["fromAnotherTeam"] != false)
            {
                return Err("Ordinary canonical channel lineage unavailable".into());
            }
        }
    }
    if super::warm::requested() {
        super::warm::reload(state, &boot, &init).await?;
    }
    {
        // Initial setup only. The genuine accepted connection retired the initial
        // surfaces; no user draft or native lease has been admitted yet.
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
    SEED.set(json!({"boot":boot,"init":init,"extras":extras,"startSeconds":start}))
        .map_err(|_| "Duplicate seed".to_owned())?;
    Ok(())
}
pub(super) fn evidence() -> Value {
    SEED.get().cloned().unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_native_history_preflight_refuses_unrelated_short_ids() {
        let session = WorkspaceSession {
            team_id: "TWORKFLOW".into(),
            user_id: "UWORKFLOW".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
        };
        let owner = json!({"id":"UWORKFLOW","name":"owner","profile":{}});
        let mut native = [owner.clone(), bot_raw(), json!({"id":"U1","profile":{}})]
            .into_iter()
            .map(|raw| {
                let user: super_platinum_core::slack::models::User =
                    serde_json::from_value(raw).unwrap();
                (user.id.clone(), user)
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut boot = json!({"ok":true,"users":[owner,bot_raw()]});
        let account = "synthetic-workflow-activity-channel";
        assert!(
            preflight_members(account, &session, 0, &native, &boot)
                .unwrap_err()
                .starts_with("Shared/frozen member-history input:")
        );
        native.remove("U1");
        assert!(preflight_members(account, &session, 0, &native, &boot).is_ok());
        boot["users"][1]["profile"] = Value::Null;
        assert!(
            preflight_members(account, &session, 0, &native, &boot)
                .unwrap_err()
                .starts_with("Shared/frozen member-hydration input:")
        );
    }
    #[test]
    fn both_original_case_boots_decode_with_team_connection_objects() {
        for grace in [false, true] {
            let user = json!({"id":"UWORKFLOW","name":"fixture-self","profile":{}});
            let raw = json!({"ok":true,"team":{"id":"TWORKFLOW","name":"Shared frozen fixture"},
                "self":user,"users":[user],"channels":rows(100,grace),"prefs":{}});
            let boot: super_platinum_core::slack::models::BootData =
                serde_json::from_value(raw).unwrap();
            assert_eq!(boot.channels.len(), 5);
            assert_eq!(boot.self_user.id, "UWORKFLOW");
            if !grace {
                assert_eq!(boot.channels[0].connected_teams[0].id, "TWORKFLOW");
                assert_eq!(boot.channels[0].connected_teams[1].id, "TPARTNER");
                let malformed = json!({"ok":true,"team":{"id":"TWORKFLOW"},"self":{"id":"UWORKFLOW"},
                    "channels":[{"id":KNOWN,"connected_teams":["TWORKFLOW","TPARTNER"]}]});
                assert!(
                    serde_json::from_value::<super_platinum_core::slack::models::BootData>(
                        malformed
                    )
                    .is_err()
                );
            }
        }
    }
    #[test]
    fn canonical_seed_members_use_supported_identity_and_profile_shapes() {
        use super_platinum_core::block_actions::workflow_management::member_hydration::{
            Flags, Plan, Scope,
        };
        let session = WorkspaceSession {
            team_id: "TWORKFLOW".into(),
            user_id: "UWORKFLOW".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
        };
        let scope =
            || Scope::capture("synthetic-workflow-activity-channel", &session, 0, "seed").unwrap();
        let flags = || Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
        let owner = json!({"id":"UWORKFLOW","name":"owner","profile":{}});
        assert!(Plan::capture_direct(scope(), vec![], vec![owner, bot_raw()], flags()).is_ok());
        for bad in [
            json!({"id":"U_HOME_BOT","profile":{}}),
            json!({"id":"U1","profile":{}}),
            json!({"id":BOT,"profile":null}),
        ] {
            assert!(Plan::capture_direct(scope(), vec![], vec![bad], flags()).is_err());
        }
    }
    #[test]
    fn raw_lookup_keeps_omissions_and_rejects_foreign_identity() {
        let original = json!({"id":MISSING,"name":"activity-fetched","future":{"opaque":7}});
        let response = json!({"ok":true,"channel":original});
        assert_eq!(
            super::super::decode_raw(response.clone(), MISSING).unwrap(),
            original
        );
        assert!(super::super::decode_raw(response, KNOWN).is_err());
        assert!(super::super::decode_raw(json!({"ok":true,"channel":false}), MISSING).is_err());
        assert!(super::super::decode_raw(json!({"ok":false,"channel":original}), MISSING).is_err());
    }
    #[test]
    fn shared_frozen_vectors_keep_composer_facts_independent() {
        let rows = rows(100, false);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0]["is_ext_shared"], true);
        assert_eq!(rows[1]["is_private"], true);
        assert_eq!(rows[2]["is_archived"], true);
        assert_eq!(rows[3]["is_frozen"], true);
        assert_eq!(rows[3]["is_archived"], false);
        assert_eq!(rows[3]["unlinked"], 100);
        assert!(rows[3].get("grace_period_end").is_none());
        assert_eq!(rows[4]["is_private"], true);
        assert!(rows[4].get("unlinked").is_none());
        assert_eq!(specs_for(false)[2].2, "frozen-active");
    }
    #[test]
    fn positive_grace_has_real_start_and_separate_public_gate() {
        let rows = rows(100, true);
        assert!(
            rows.iter()
                .all(|row| row["unlinked"] == 100 && row["is_frozen"] == true)
        );
        assert_eq!(rows[0]["grace_period_end"], 1);
        assert!(rows[1].get("grace_period_end").is_none());
        assert_eq!(rows[1]["is_private"], true);
        assert_eq!(rows[2]["is_archived"], true);
        assert_eq!(rows[2]["grace_period_end"], 1);
        assert_eq!(rows[3]["is_private"], true);
        assert_eq!(rows[3]["is_archived"], true);
        assert!(rows[4].get("grace_period_end").is_none());
        assert_eq!(specs_for(true)[1].2, "grace-archived");
    }
}
