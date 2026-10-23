//! Closed Share endpoint. Delivery, server application and modal closure are
//! independent observations; no request from this fixture reaches a network.
use super::*;
pub(crate) mod connectivity;
pub(crate) mod dm;
pub(crate) mod drafts;
pub(crate) mod multiple;
pub(crate) mod person;
pub(crate) mod posting;
pub(crate) mod resend;
pub(crate) mod search;
pub(crate) mod startup;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
    workflow_share as protocol,
};

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_CASE";
pub(crate) const MESSAGE: &str = "Share review \u{4e2d}\nPlease take a look.";
pub(crate) const SERVER_TS: &str = "1781524999.000001";
const MAX_READS: usize = 24;

fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_else(|_| "success".into())
}
fn supported(value: &str) -> bool {
    matches!(
        value,
        "success" | "rejected" | "lost-response" | "held" | "destination-failure"
    )
}
pub(crate) fn requested() -> bool {
    super::requested()
        && super::case() == "first-add-success"
        && std::env::var(CHECK_ENV).as_deref() == Ok("1")
        && std::env::var(feature::CHECK_ENV)
            .unwrap_or_default()
            .is_empty()
        && std::env::var(feature::CASE_ENV)
            .unwrap_or_default()
            .is_empty()
        && supported(&case())
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    capture(s).is_ok()
}

#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Data>>);
struct Data {
    case: String,
    reads: Vec<Value>,
    requests: Vec<Value>,
    responses: Vec<Value>,
    attempts: Vec<protocol::Attempt>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<()>)>,
    // Immutable synthetic server rows, deduplicated by actual client_msg_id.
    applied: Vec<Value>,
    started: bool,
    drafts: Option<drafts::Data>,
    resend: Option<resend::Data>,
    posting: Option<posting::Data>,
    dm: Option<dm::Data>,
    person: Option<person::Data>,
    search: Option<search::Data>,
    ranking: Option<search::ranking::Data>,
    connectivity: Option<connectivity::Data>,
    startup: Option<startup::Data>,
    multiple: Option<multiple::Data>,
}
impl Model {
    pub(crate) fn new() -> Self {
        Self::for_case(case())
    }
    fn for_case(case: String) -> Self {
        Self(Arc::new(Mutex::new(Data {
            case,
            reads: vec![],
            requests: vec![],
            responses: vec![],
            attempts: vec![],
            settlements: vec![],
            releases: vec![],
            held: vec![],
            applied: vec![],
            started: false,
            drafts: drafts::requested().then(drafts::Data::default),
            resend: resend::requested().then(resend::Data::new),
            posting: posting::requested().then(posting::Data::new),
            dm: dm::requested().then(dm::Data::new),
            person: (person::requested() || search::person_enabled()).then(person::Data::new),
            search: search::requested().then(search::Data::new),
            ranking: (!search::requested()).then(search::ranking::Data::new),
            connectivity: None,
            startup: startup::requested().then(startup::Data::default),
            multiple: multiple::requested().then(multiple::Data::new),
        })))
    }
    pub(crate) fn snapshot(&self) -> Value {
        let m = self.0.lock().unwrap();
        let mut value = json!({"case":m.case,"reads":m.reads,"requests":m.requests,
            "responses":m.responses,"attempts":m.attempts,"settlements":m.settlements,
            "releases":m.releases,"held":m.held.iter().map(|(n,_)|n).collect::<Vec<_>>(),
            "applied":m.applied,"started":m.started,"networkCalls":0,
            "retrySchedule":{"kind":"closed-fixture-source-minimum","delayMillis":1000,"maxAttempts":3}});
        if let Some(drafts) = &m.drafts {
            value["drafts"] = drafts.snapshot();
        }
        if let Some(resend) = &m.resend {
            value["resend"] = resend.snapshot();
        }
        if let Some(posting) = &m.posting {
            value["posting"] = posting.snapshot();
        }
        if let Some(dm) = &m.dm {
            value["dm"] = dm.snapshot();
        }
        if let Some(person) = &m.person {
            value["person"] = person.snapshot();
        }
        if let Some(search) = &m.search {
            value["search"] = search.snapshot();
        }
        if let Some(ranking) = &m.ranking {
            value["ranking"] = ranking.snapshot();
        }
        if let Some(startup) = &m.startup {
            value["startup"] = startup.snapshot();
        }
        if let Some(connectivity) = &m.connectivity {
            value["connectivity"] = connectivity.snapshot();
        }
        if let Some(multiple) = &m.multiple {
            value["multiple"] = multiple.snapshot();
        }
        value
    }
    pub(crate) fn release(&self, serial: usize) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        let i = m
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Original Share response sender absent")?;
        let (_, tx) = m.held.remove(i);
        let value = json!({"serial":serial,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
        m.releases.push(value.clone());
        Ok(value)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, String> {
    if !requested() {
        return Err("Named closed Share case required".into());
    }
    let parent = super::capture(s)?;
    let model = parent
        .0
        .lock()
        .unwrap()
        .share
        .clone()
        .ok_or("Original Share model absent")?;
    if model.0.lock().unwrap().case != case() {
        return Err("Original Share case changed".into());
    }
    Ok(model)
}
fn context(s: &ShellState) -> Result<(SlackClient, WorkspaceSession), Error> {
    capture(s).map_err(|_| closed())?;
    let session = s
        .core
        .session
        .as_ref()
        .and_then(|a| a.workspaces.get("TWORKFLOW"))
        .filter(|w| w.team_id == "TWORKFLOW" && w.user_id == "UWORKFLOW" && !w.token.is_empty())
        .ok_or_else(closed)?;
    if !s
        .core
        .workspaces
        .get(&session.team_id)
        .is_some_and(|w| w.self_user_id == session.user_id)
    {
        return Err(closed());
    }
    Ok((s.core.client.clone(), session.clone()))
}
fn active(s: &ShellState) -> bool {
    let Some(h) = s.app_home.about.management.surface.as_ref() else {
        return false;
    };
    let value = crate::app_home::about::management::permissions::finish_up::snapshot(s, h.id);
    value["attempts"].as_array().is_some_and(|rows| {
        rows.iter().any(|a| {
            a["finished"] == true
                && a["complete"] == true
                && a["closed"] == false
                && a["confirmationShare"]["active"] == true
        })
    })
}
fn owner(s: &ShellState, session: &WorkspaceSession) -> Value {
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,
        "team":session.team_id,"user":session.user_id})
}
fn safe_body(body: &RequestBody) -> Value {
    match body {
        RequestBody::Form(fields) => json!(
            fields
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "token" | "enterprise_token"))
                .collect::<Vec<_>>()
        ),
        RequestBody::Json(value) => {
            let mut value = value.clone();
            if let Some(obj) = value.as_object_mut() {
                obj.remove("token");
                obj.remove("enterprise_token");
            }
            value
        }
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() || !active(&state.peek()) {
        return None;
    }
    if let Some(result) = search::ordinary(state, request).await {
        return Some(result);
    }
    if let Some(result) = dm::read(state, request).await {
        return Some(result);
    }
    Some((|| {
        let s = state.peek();
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let (method, fields) = crate::app_home::about::management::fixture::decode(request)?;
        if method != "conversations.info" {
            return Err(closed());
        }
        let ids = fields
            .iter()
            .filter(|(k, _)| k == "channel")
            .map(|(_, v)| v)
            .collect::<Vec<_>>();
        if ids.len() != 1 {
            return Err(closed());
        }
        let id = ids[0];
        let raw = raw::channel(id).ok_or_else(closed)?;
        let expected = api::conversations_info(&client, &session, id.clone());
        super::super::picker::same_request(request, &expected)?;
        let mut m = model.0.lock().unwrap();
        if m.reads.len() >= MAX_READS {
            return Err(closed());
        }
        let result = if m.case == "destination-failure" && id == raw::DEST {
            Err(Error::Offline(
                "Synthetic Share destination unavailable".into(),
            ))
        } else {
            Ok(json!({"ok":true,"channel":raw,"opaque_receipt":"original-share-channel"}))
        };
        let serial = m.reads.len() + 1;
        m.reads.push(
            json!({"serial":serial,"method":method,"body":safe_body(&request.body),
            "endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),
            "owner":owner(&s,&session),"result":super::super::receipt(&result)}),
        );
        result
    })())
}
pub(crate) fn destination() -> &'static str {
    if multiple::requested() {
        return multiple::CHANNEL;
    }
    if let Some(id) = search::ranking::destination() {
        return id;
    }
    if person::requested() || search::person_enabled() {
        person::CHANNEL
    } else if search::requested() && search::case() == "channel-success" {
        search::CHANNEL
    } else {
        dm::destination()
    }
}
fn inspect(
    request: &protocol::Request,
    trigger: &Value,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> Result<Value, Error> {
    let message = request.message();
    if message.trigger() != trigger {
        return Err(closed());
    }
    let expected = protocol::Message::new(
        trigger,
        MESSAGE,
        destination(),
        message.client_msg_id(),
        message.local_ts(),
    )?
    .request(client, session)?;
    super::super::picker::same_request(request.prepared(), expected.prepared())?;
    Ok(request.inspect())
}
fn reply(message: &protocol::Message, rejected: bool) -> Value {
    if rejected {
        return json!({"ok":false,"error":"restricted_action_read_only_channel",
        "opaque_receipt":"original-share-rejected"});
    }
    json!({"ok":true,"channel":message.channel(),"ts":SERVER_TS,
        "message":{"type":"message","user":"UWORKFLOW","ts":SERVER_TS,
            "text":message.text_fallback(),"blocks":message.blocks(),"client_msg_id":message.client_msg_id()},
        "opaque_receipt":"original-share-ack"})
}
fn dispatch(
    model: &Model,
    serial: usize,
    wire: &Value,
    owner: &Value,
    raw: &Value,
) -> Option<oneshot::Receiver<()>> {
    let mut m = model.0.lock().unwrap();
    let rejected = m.case == "rejected";
    let lost = m.case == "lost-response";
    m.requests
        .push(json!({"serial":serial,"request":wire,"owner":owner}));
    if !rejected && m.applied.is_empty() {
        m.applied.push(raw["message"].clone());
    }
    m.responses.push(if lost {
        json!({"serial":serial,"error":"transport","applied":true})
    } else {
        json!({"serial":serial,"raw":raw,"applied":!rejected})
    });
    if m.case == "held" {
        let (tx, rx) = oneshot::channel();
        m.held.push((serial, tx));
        Some(rx)
    } else {
        None
    }
}
struct Settlement {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Settlement {
    fn drop(&mut self) {
        self.model
            .0
            .lock()
            .unwrap()
            .settlements
            .push(json!({"serial":self.serial,"returned":self.returned}));
    }
}

pub(crate) async fn mutation(
    state: Signal<ShellState>,
    request: &protocol::Request,
    mut observe: impl FnMut(protocol::Attempt),
) -> Option<Result<Value, Error>> {
    if resend::requested() && resend::in_flight(&state.peek()) {
        return resend::mutation(state, request, observe).await;
    }
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        if !active(&s) {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let parent = super::super::capture(&s).map_err(|_| closed())?;
        let trigger = parent.0.lock().unwrap().trigger.clone();
        let wire = inspect(request, &trigger, &client, &session)?;
        let mut m = model.0.lock().unwrap();
        if m.started || m.case == "destination-failure" {
            return Err(closed());
        }
        m.started = true;
        let scenario = m.case.clone();
        drop(m);
        Ok((model, wire, owner(&s, &session), scenario))
    })();
    let (model, wire, owner, scenario) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let count = if scenario == "lost-response" { 3 } else { 1 };
    for number in 1..=count {
        if !capture(&state.peek()).is_ok_and(|m| Arc::ptr_eq(&m.0, &model.0)) {
            return Some(Err(Error::Transport(
                "Share owner retired before dispatch".into(),
            )));
        }
        let rejected = scenario == "rejected";
        let lost = scenario == "lost-response";
        let raw = reply(request.message(), rejected);
        let serial = number as usize;
        let rx = dispatch(&model, serial, &wire, &owner, &raw);
        let mut settlement = Settlement {
            model: model.clone(),
            serial,
            returned: false,
        };
        if let Some(rx) = rx {
            if !matches!(
                tokio::time::timeout(Duration::from_secs(45), rx).await,
                Ok(Ok(()))
            ) {
                return Some(Err(closed()));
            }
        }
        let current = capture(&state.peek()).is_ok_and(|m| Arc::ptr_eq(&m.0, &model.0));
        let attempt = protocol::Attempt {
            number,
            status: if lost { None } else { Some(200) },
            outcome: if !current {
                protocol::Outcome::Retired
            } else if lost {
                protocol::Outcome::Unknown
            } else if rejected {
                protocol::Outcome::Rejected
            } else {
                protocol::Outcome::Acknowledged
            },
            raw: (!lost).then(|| raw.clone()),
            response_text: (!lost).then(|| raw.to_string()),
            retry_after_millis: None,
            may_have_applied: !rejected,
        };
        model.0.lock().unwrap().attempts.push(attempt.clone());
        observe(attempt);
        settlement.returned = true;
        drop(settlement);
        if !current {
            return Some(Err(Error::Transport(
                "Share owner retired after dispatch".into(),
            )));
        }
        if !lost {
            return Some(if rejected {
                Err(Error::Api("restricted_action_read_only_channel".into()))
            } else {
                Ok(raw)
            });
        }
        if number < count {
            // Source jitter permits 1s minimum. This closed server deliberately
            // chooses that bound; it does not claim production random timing.
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }
    Some(Err(Error::Transport(
        "Synthetic Share replies lost after application".into(),
    )))
}

#[cfg(test)]
mod tests;
