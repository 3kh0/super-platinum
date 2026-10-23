//! Exact offline Save-owned Agentforce catalog failure and complete retry.
use super::*;
use crate::directory_member_fetches::Context;
use std::sync::OnceLock;
use super_platinum_core::block_actions::app_eligibility::agents;

pub(crate) const ID: &str = "UAGENT001";
pub(crate) const APP: &str = "AAGENT001";
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_MEMBER_AGENTS_CHECK").as_deref()
            == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-member-agents")
}
fn ledger() -> &'static Mutex<Vec<Value>> {
    static VALUE: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}
pub(crate) fn row() -> Value {
    json!({"id":ID,"team_id":"T1","name":"agent-raw-username","updated":101,
        "is_agentforce_bot":true,"is_bot":true,"deleted":false,"is_restricted":false,"is_ultra_restricted":false,
        "profile":{"display_name":"restricted-agent","real_name":"Restricted Agent","api_app_id":APP},
        "fixture_opaque":{"keep":"original-agent","ordered":["profile","access"]}})
}
pub(crate) fn seed_boot(boot: &mut Value) {
    if named() {
        boot["users"].as_array_mut().unwrap().push(row());
    }
}
pub(crate) fn snapshot() -> Value {
    if !named() {
        return json!({"enabled":false});
    }
    json!({"enabled":true,"requests":*ledger().lock().unwrap(),"noLiveNetwork":true})
}
fn refused() -> Error {
    Error::Offline("Closed plain agent fixture refused request".into())
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let ctx = Context::capture(&state.peek(), "T1")?;
    if !named()
        || !completion::enabled(&state.peek())
        || !ctx.current(&state.peek())
        || ctx.session.user_id != completion::SELF
    {
        return Err(refused());
    }
    let serial = ledger().lock().unwrap().len() + 1;
    let phase = (serial - 1) % 3;
    if serial > 6 {
        return Err(refused());
    }
    let (method, expected, raw) = match phase {
        0 => (
            "client.init",
            agents::boot_request(&ctx.client, &ctx.session),
            json!({"ok":true,"self":{"id":ctx.session.user_id,"name":"fixture-self","team_id":"T1","is_admin":true,"profile":{}},"team":{"id":"T1"}}),
        ),
        1 => (
            "client.extras",
            agents::preferences_request(&ctx.client, &ctx.session),
            json!({"ok":true,"team_prefs":{"toplevel_prefs":{"ai_apps_settings":{"apps":[{"app_id":APP,"is_ai_enabled":false}]}}}}),
        ),
        _ => (
            "aiApps.list",
            agents::list_request(&ctx.client, &ctx.session, None)?,
            json!({"ok":true,"apps":[{"id":APP,"is_agent_app":true,"agentforce":{},"bot_user":{"id":ID}}],"response_metadata":{"next_cursor":""}}),
        ),
    };
    if !completion::protocol::same_request(request, &expected) {
        return Err(refused());
    }
    let RequestBody::Form(form) = &request.body else {
        return Err(refused());
    };
    let args: serde_json::Map<String, Value> = form
        .iter()
        .filter(|(k, _)| k != "token")
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    let failed = serial == 3;
    let response = if failed {
        json!({"error":"offline"})
    } else {
        raw.clone()
    };
    let current = ctx.current(&state.peek());
    ledger().lock().unwrap().push(json!({"serial":serial,"method":method,"args":args,"owner":{"account":ctx.account,"epoch":ctx.epoch,"team":ctx.session.team_id,"user":ctx.session.user_id},"socket":state.peek().core.workspaces["T1"].rt_generation,"response":response,"disposition":if failed{"offline"}else{"response"},"currentAtReturn":current}));
    if !current {
        return Err(refused());
    }
    if failed {
        Err(Error::Offline("Synthetic agent catalog unavailable".into()))
    } else {
        Ok(raw)
    }
}
