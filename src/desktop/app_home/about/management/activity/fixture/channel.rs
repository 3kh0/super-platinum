//! Closed channel and About reads for the navigation scenario. Never a network fallback.
use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, models::Channel},
};

pub(crate) mod shared_frozen;
pub(crate) const KNOWN: &str = "CACTKNOWN01";
pub(crate) const MISSING: &str = "CACTMISS01";
pub(crate) const ARCHIVED: &str = "CACTARCH01";
pub(crate) const PRIVATE_ARCHIVED: &str = "CACTPRARCH01";
#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Data>>);
struct Data {
    account: Option<String>,
    epoch: u64,
    credentials: String,
    original: String,
    requests: Vec<Value>,
    responses: Vec<Value>,
    about: Vec<Value>,
    mode: Mode,
    held: Option<(usize, oneshot::Sender<Result<Value, Error>>)>,
    completed: usize,
    cancelled: usize,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Mode {
    #[default]
    Success,
    Malformed,
    Offline,
    Held,
}
pub(super) fn install(s: &mut ShellState, special: bool) {
    let original = s.core.active_channel.clone().unwrap();
    let w = s.core.workspaces.get_mut("TWORKFLOW").unwrap();
    w.channels.retain(|id, _| id == &original);
    w.channels.insert(
        KNOWN.into(),
        serde_json::from_value(known_raw(special)).unwrap(),
    );
    if special {
        for row in archived_rows() {
            let channel: Channel = serde_json::from_value(row).unwrap();
            w.channels.insert(channel.id.clone(), channel);
        }
    }
    let bot = w.users.get_mut("U_HOME_BOT").unwrap();
    bot.name = Some("Review requests".into());
    bot.real_name = Some("Review requests".into());
    if let Some(profile) = bot.profile.as_mut() {
        profile.display_name = Some("Review requests".into());
        profile.real_name = Some("Review requests".into());
    }
    let session = s.core.session.as_ref().unwrap();
    s.workspaces = session
        .workspaces
        .values()
        .map(|w| crate::state::WorkspaceVm {
            id: w.team_id.clone(),
            name: w.name.clone(),
            initials: "W".into(),
        })
        .collect();
    s.active_workspace = 0;
    let data = Data {
        account: s.core.active_account.clone(),
        epoch: s.core.account_epoch,
        credentials: serde_json::to_string(session).unwrap(),
        original,
        requests: vec![],
        responses: vec![],
        about: vec![],
        mode: Mode::Success,
        held: None,
        completed: 0,
        cancelled: 0,
    };
    s.app_home
        .about
        .management
        .activity_fixture
        .as_ref()
        .unwrap()
        .0
        .lock()
        .unwrap()
        .channel = Some(Model(Arc::new(Mutex::new(data))));
    s.refresh_from_core();
}
fn raw(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"is_channel":true,"is_member":true,"is_archived":false,"is_private":false})
}
fn known_raw(special: bool) -> Value {
    let mut row = raw(KNOWN, "activity-known");
    row["is_private"] = json!(special);
    row
}
fn archived_rows() -> Vec<Value> {
    [
        (ARCHIVED, "activity-archived", false),
        (PRIVATE_ARCHIVED, "activity-private-archived", true),
    ]
    .into_iter()
    .map(|(id, name, private)| {
        let mut row = raw(id, name);
        row["is_archived"] = json!(true);
        row["is_private"] = json!(private);
        row
    })
    .collect()
}
pub(super) fn special_messages(row: &mut Value) {
    row["step_events"].as_array_mut().unwrap().push(json!({
        "step_execution_id":"STEP_ARCHIVED", "function_id":"FN_REVIEW",
        "last_updated":1781524800000_i64, "step_status":"SUCCESS",
        "outcome_message":{"template":"Open {archived} then {privateArchived}",
            "variables":{"archived":{"type":"CHANNEL","value":ARCHIVED},
            "privateArchived":{"type":"CHANNEL","value":PRIVATE_ARCHIVED}}},
        "future":{"preserved":19}
    }));
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !matches!(
        case().as_str(),
        "channel"
            | "channel-held-success"
            | "channel-special"
            | "channel-shared-frozen"
            | "channel-frozen-grace"
    ) && !super::mixed::is_case(&case())
    {
        return Err(refused());
    }
    let model = super::model(s)?
        .0
        .lock()
        .unwrap()
        .channel
        .clone()
        .ok_or_else(refused)?;
    let m = model.0.lock().unwrap();
    if s.core.active_account != m.account
        || s.core.account_epoch != m.epoch
        || s.core
            .session
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok())
            .as_ref()
            != Some(&m.credentials)
    {
        return Err(refused());
    }
    drop(m);
    Ok(model)
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    model(s).is_ok()
}
pub(crate) fn mode(s: &ShellState, mode: Mode) -> Result<(), String> {
    model(s)
        .map_err(|_| "Channel fixture unavailable")?
        .0
        .lock()
        .unwrap()
        .mode = mode;
    Ok(())
}
fn request_parts(request: &PreparedRequest) -> Result<(String, Value), Error> {
    if request.method != "POST" {
        return Err(refused());
    }
    let endpoint = request
        .url
        .strip_prefix("https://example.test/api/")
        .ok_or_else(refused)?;
    let method = endpoint.split('?').next().ok_or_else(refused)?.to_owned();
    let RequestBody::Form(fields) = &request.body else {
        return Err(refused());
    };
    let fields: serde_json::Map<String, Value> = fields
        .iter()
        .filter(|(k, _)| k != "token")
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    Ok((method, Value::Object(fields)))
}
pub(crate) async fn fetch(
    state: Signal<ShellState>,
    client: &SlackClient,
    session: &WorkspaceSession,
    id: &str,
) -> Result<Channel, Error> {
    serde_json::from_value(fetch_raw(state, client, session, id).await?)
        .map_err(|_| Error::Api("fixture_channel_response_invalid".into()))
}
pub(crate) async fn fetch_raw(
    state: Signal<ShellState>,
    client: &SlackClient,
    session: &WorkspaceSession,
    id: &str,
) -> Result<Value, Error> {
    let model = model(&state.peek())?;
    if id != MISSING || session.team_id != "TWORKFLOW" || session.user_id != "UWORKFLOW" {
        return Err(refused());
    }
    let request = api::conversations_info(client, session, id.into());
    let (method, args) = request_parts(&request)?;
    if method != "conversations.info" || args != json!({"channel":id,"include_num_members":"false"})
    {
        return Err(refused());
    }
    let (serial, receiver) = {
        let mut m = model.0.lock().unwrap();
        if m.held.is_some() {
            return Err(refused());
        }
        m.requests.push(json!({"method":method,"args":args}));
        match std::mem::take(&mut m.mode) {
            Mode::Held => {
                let (send, recv) = oneshot::channel();
                m.held = Some((m.requests.len(), send));
                (m.requests.len(), recv)
            }
            mode => {
                m.completed += 1;
                if super::mixed::is_case(&case()) {
                    let serial = m.requests.len();
                    let raw = match mode {
                        Mode::Offline => Value::Null,
                        Mode::Malformed => json!({"ok":true,"channel":false}),
                        _ => json!({"ok":true,"channel":raw(MISSING,"activity-fetched")}),
                    };
                    m.responses.push(json!({"serial":serial,"raw":raw,
                        "error":if matches!(mode,Mode::Offline){Some("offline")}else{None},
                        "cancelled":false}));
                }
                return match mode {
                    Mode::Offline => {
                        Err(Error::Offline("Synthetic channel read unavailable".into()))
                    }
                    Mode::Malformed => decode_raw(json!({"ok":true,"channel":false}), id),
                    _ => decode_raw(
                        json!({"ok":true,"channel":raw(MISSING,"activity-fetched")}),
                        id,
                    ),
                };
            }
        }
    };
    struct Finish(Model, bool);
    impl Drop for Finish {
        fn drop(&mut self) {
            let mut m = self.0.0.lock().unwrap();
            m.completed += 1;
            if !self.1 {
                m.cancelled += 1;
            }
        }
    }
    let mut finish = Finish(model, false);
    let value = receiver.await.map_err(|_| refused())??;
    finish.1 = true;
    if super::mixed::is_case(&case()) {
        let mut m = finish.0.0.lock().unwrap();
        m.responses
            .push(json!({"serial":serial,"raw":value,"error":null,"cancelled":false}));
    }
    decode_raw(value, id)
}
fn decode_raw(raw: Value, id: &str) -> Result<Value, Error> {
    // Validate using the unchanged typed decoder, but return the original map:
    // typed serialization would invent omitted values before canonical ingress.
    let channel = decode(raw.clone())?;
    if channel.id != id {
        return Err(Error::Api("fixture_channel_response_invalid".into()));
    }
    Ok(raw["channel"].clone())
}
fn decode(raw: Value) -> Result<Channel, Error> {
    #[derive(serde::Deserialize)]
    struct Page {
        channel: Channel,
    }
    // Same response shape as api::fetch_conversation_info; malformed replies are errors.
    if raw["ok"] != true {
        return Err(refused());
    }
    serde_json::from_value::<Page>(raw)
        .map(|p| p.channel)
        .map_err(|_| Error::Api("fixture_channel_response_invalid".into()))
}
pub(crate) fn release(s: &ShellState) -> Result<Value, String> {
    release_value(
        s,
        json!({"ok":true,"channel":raw(MISSING,"activity-fetched")}),
        false,
    )
}
pub(crate) fn release_mixed(s: &ShellState, success: bool) -> Result<Value, String> {
    if !super::mixed::is_case(&case()) {
        return Err("Mixed channel release unavailable".into());
    }
    let value = if success {
        json!({"ok":true,"channel":raw(MISSING,"activity-fetched")})
    } else {
        json!({"ok":true,"channel":false})
    };
    release_value(s, value, true)
}
fn release_value(s: &ShellState, value: Value, mixed: bool) -> Result<Value, String> {
    let model = model(s).map_err(|_| "Channel fixture unavailable")?;
    let (serial, sender) = model
        .0
        .lock()
        .unwrap()
        .held
        .take()
        .ok_or("No held channel request")?;
    let receiver_retired = sender.is_closed();
    let delivered = sender.send(Ok(value.clone())).is_ok();
    let mut proof = json!({"delivered":delivered,"cancelledBeforeReply":!delivered});
    if case() == "channel-held-success" {
        proof["serial"] = json!(serial);
        proof["requestCount"] = json!(model.0.lock().unwrap().requests.len());
    }
    if mixed {
        proof["serial"] = json!(serial);
        proof["requestCount"] = json!(model.0.lock().unwrap().requests.len());
        proof["receiverRetired"] = json!(receiver_retired);
        proof["raw"] = value;
    }
    Ok(proof)
}
/// Called only inside the normal About read, with its original Owner/Key guards.
pub(crate) fn about_response(s: &ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    let model = model(s)?;
    let mut m = model.0.lock().unwrap();
    if s.core.active_channel.as_ref() != Some(&m.original) {
        return Err(refused());
    }
    let (method, args) = request_parts(request)?;
    let value = match method.as_str() {
        "conversations.info"
            if args
                == json!({"channel":m.original,"return_app_home":"true","_x_reason":"app-home-open-api"}) =>
        {
            let channel = s.core.workspaces["TWORKFLOW"].channels[&m.original].clone();
            json!({"ok":true,"channel":channel,"app_home":{"app_id":"A_HOME","app_installed_team_id":"TWORKFLOW","home_tab_enabled":true,"messages_tab_enabled":true,"home_view_id":"V_HOME"}})
        }
        "apps.about.info" if args["app_id"] == "A_HOME" => {
            json!({"ok":true,"app_id":"A_HOME","name":"Review requests","owner_ids":["UWORKFLOW"]})
        }
        "apps.profile.get" if args["app"] == "A_HOME" => {
            json!({"ok":true,"app_profile":{"id":"A_HOME","team_id":"TWORKFLOW","is_installed":true,"is_workflow_app":true}})
        }
        "experiments.getByUser" => {
            json!({"ok":true,"assignments":{"agent_about_page":{"group":"on"},"flexpane_content_parity":{"group":"on"}}})
        }
        _ => return Err(refused()),
    };
    m.about.push(json!({"method":method,"args":args}));
    Ok(value)
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let data=s.app_home.about.management.activity_fixture.as_ref().and_then(|m|m.0.lock().unwrap().channel.clone()).map(|m|{let m=m.0.lock().unwrap();json!({"original":m.original,"requests":m.requests,"aboutRequests":m.about,"completed":m.completed,"cancelled":m.cancelled,"held":m.held.is_some(),"networkCalls":0})});
    let docs=s.app_home.about.management.documents.iter().map(|d|json!({"id":d.id,"title":d.title,"description":d.description,"revision":d.revision,"dirty":d.dirty(),"pending":d.pending})).collect::<Vec<_>>();
    let rows=s.sidebar_sections.iter().flat_map(|section|section.channel_indices.iter().filter_map(|index|s.channels.get(*index).map(|row|json!({"id":row.id,"name":row.name,"index":index,"section":section.id,"active":*index==s.active_channel})))).collect::<Vec<_>>();
    let sections=s.sidebar_sections.iter().map(|section|json!({"id":section.id,"title":section.title,"indices":section.channel_indices})).collect::<Vec<_>>();
    let mut source=s.core.workspaces.get("TWORKFLOW").map(|w|w.channels.values().map(|c|json!({"id":c.id,"name":c.name,"isMember":c.extra.get("is_member"),"isOpen":c.extra.get("is_open"),"unreadCount":c.unread_count,"unreadDisplay":c.unread_count_display,"mentionCount":c.mention_count,"hasUnreads":c.has_unreads})).collect::<Vec<_>>());
    if case() == "channel-special" || shared_frozen::requested() {
        if let Some(rows) = source.as_mut() {
            for row in rows {
                if let Some(c) = row["id"]
                    .as_str()
                    .and_then(|id| s.core.workspaces["TWORKFLOW"].channels.get(id))
                {
                    row["isPrivate"] = json!(c.is_private);
                    row["isGroup"] = json!(c.is_group);
                    row["isArchived"] = json!(c.is_archived);
                }
            }
        }
    }
    let mut value = json!({"ledger":data,"documents":docs,"selected":s.core.active_channel,"generation":s.channel_generation,
        "sidebar":{"rows":rows,"sections":sections,"activeIndex":s.active_channel,"activeId":s.channels.get(s.active_channel).map(|c|&c.id)},"sourceRows":source,
        "aboutVisit":s.app_home.about.surface.as_ref().map(|h|h.id),"managementVisit":s.app_home.about.management.surface.as_ref().map(|h|h.id),
        "missingPresent":s.core.workspaces.get("TWORKFLOW").is_some_and(|w|w.channels.contains_key(MISSING))});
    if super::mixed::is_case(&case()) {
        if let Ok(model) = model(s) {
            value["ledger"]["responses"] = json!(model.0.lock().unwrap().responses);
        }
    }
    if shared_frozen::requested() {
        value["seed"] = shared_frozen::evidence();
        value["labels"] = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .map(|h| super::super::channel_labels::snapshot(s, h.id))
            .unwrap_or(Value::Null);
    }
    value
}
