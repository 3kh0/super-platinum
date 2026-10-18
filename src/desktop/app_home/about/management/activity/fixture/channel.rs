//! Closed channel and About reads for the navigation scenario. Never a network fallback.
use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, models::Channel},
};

pub(crate) const KNOWN: &str = "CACTKNOWN01";
pub(crate) const MISSING: &str = "CACTMISS01";
#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Data>>);
struct Data {
    account: Option<String>,
    epoch: u64,
    credentials: String,
    original: String,
    requests: Vec<Value>,
    about: Vec<Value>,
    mode: Mode,
    held: Option<oneshot::Sender<Result<Value, Error>>>,
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
pub(super) fn install(s: &mut ShellState) {
    let original = s.core.active_channel.clone().unwrap();
    let w = s.core.workspaces.get_mut("TWORKFLOW").unwrap();
    w.channels.retain(|id, _| id == &original);
    w.channels.insert(
        KNOWN.into(),
        serde_json::from_value(raw(KNOWN, "activity-known")).unwrap(),
    );
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
fn model(s: &ShellState) -> Result<Model, Error> {
    if case() != "channel" {
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
    let receiver = {
        let mut m = model.0.lock().unwrap();
        if m.held.is_some() {
            return Err(refused());
        }
        m.requests.push(json!({"method":method,"args":args}));
        match std::mem::take(&mut m.mode) {
            Mode::Held => {
                let (send, recv) = oneshot::channel();
                m.held = Some(send);
                Some(recv)
            }
            mode => {
                m.completed += 1;
                return match mode {
                    Mode::Offline => {
                        Err(Error::Offline("Synthetic channel read unavailable".into()))
                    }
                    Mode::Malformed => decode(json!({"ok":true,"channel":false})),
                    _ => decode(json!({"ok":true,"channel":raw(MISSING,"activity-fetched")})),
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
    let value = receiver.unwrap().await.map_err(|_| refused())??;
    finish.1 = true;
    decode(value)
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
    let model = model(s).map_err(|_| "Channel fixture unavailable")?;
    let sender = model
        .0
        .lock()
        .unwrap()
        .held
        .take()
        .ok_or("No held channel request")?;
    let delivered = sender
        .send(Ok(
            json!({"ok":true,"channel":raw(MISSING,"activity-fetched")}),
        ))
        .is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":!delivered}))
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
    let source=s.core.workspaces.get("TWORKFLOW").map(|w|w.channels.values().map(|c|json!({"id":c.id,"name":c.name,"isMember":c.extra.get("is_member"),"isOpen":c.extra.get("is_open"),"unreadCount":c.unread_count,"unreadDisplay":c.unread_count_display,"mentionCount":c.mention_count,"hasUnreads":c.has_unreads})).collect::<Vec<_>>());
    json!({"ledger":data,"documents":docs,"selected":s.core.active_channel,"generation":s.channel_generation,
        "sidebar":{"rows":rows,"sections":sections,"activeIndex":s.active_channel,"activeId":s.channels.get(s.active_channel).map(|c|&c.id)},"sourceRows":source,
        "aboutVisit":s.app_home.about.surface.as_ref().map(|h|h.id),"managementVisit":s.app_home.about.management.surface.as_ref().map(|h|h.id),
        "missingPresent":s.core.workspaces.get("TWORKFLOW").is_some_and(|w|w.channels.contains_key(MISSING))})
}
