//! Opt-in Feature server: original ACK and later channel read are independent.
//! No canonical state/event is seeded and no request can leave this module.
use super::*;
use super_platinum_core::{
    channel_workflows::{Read, feature as protocol, feature_permission::Read as PermissionRead},
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_FEATURE_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_FEATURE_CASE";
const TRIGGER: &str = "TR_REVIEW";
const MAX_READS: usize = 32;

fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_else(|_| "success".into())
}
fn supported(case: &str) -> bool {
    matches!(
        case,
        "success" | "denied" | "rejected" | "readback-missing" | "readback-failure" | "held"
    )
}
pub(crate) fn requested() -> bool {
    super::requested()
        && super::case() == "first-add-success"
        && std::env::var(CHECK_ENV).as_deref() == Ok("1")
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
    acknowledged: bool,
}
impl Model {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(Data {
            case: case(),
            reads: vec![],
            requests: vec![],
            responses: vec![],
            attempts: vec![],
            settlements: vec![],
            releases: vec![],
            held: vec![],
            acknowledged: false,
        })))
    }
    pub(crate) fn snapshot(&self) -> Value {
        let m = self.0.lock().unwrap();
        json!({"case":m.case,"reads":m.reads,"requests":m.requests,"responses":m.responses,
            "attempts":m.attempts,"settlements":m.settlements,"releases":m.releases,
            "held":m.held.iter().map(|(n,_)|n).collect::<Vec<_>>(),
            "acknowledged":m.acknowledged,"networkCalls":0})
    }
    pub(crate) fn release(&self, serial: usize) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        let i = m
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Original Feature response absent")?;
        let (_, tx) = m.held.remove(i);
        let value = json!({"serial":serial,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
        m.releases.push(value.clone());
        Ok(value)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, String> {
    if !requested() {
        return Err("Named closed Feature case required".into());
    }
    // Parent retains original account/epoch/full credentials even after Add's
    // modal disappears. Never reacquire authority from the successor surface.
    let parent = super::capture(s)?;
    let model = parent.0.lock().unwrap().feature.clone();
    if model.0.lock().unwrap().case != case() {
        return Err("Original Feature case changed".into());
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
        .filter(|session| session.team_id == "TWORKFLOW" && !session.token.is_empty())
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
fn discovery(session: &WorkspaceSession) -> Value {
    json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",session.team_id),
        "ttl_seconds":60,"opaque_receipt":"original-feature-discovery"})
}
fn channel(acknowledged: bool, scenario: &str) -> Result<Value, Error> {
    let mut raw = raw::channel(raw::DEST).ok_or_else(closed)?;
    if acknowledged && scenario != "readback-missing" {
        if raw.get("properties").is_none_or(Value::is_null) {
            raw["properties"] = json!({});
        }
        raw["properties"]["channel_workflows"] = json!([{
            "workflow_trigger_id":TRIGGER,"title":"Server featured review",
            "opaque_featured":{"source":"fresh-conversations-info"}
        }]);
    }
    Ok(raw)
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
fn method(request: &PreparedRequest) -> Option<&'static str> {
    let url = url::Url::parse(&request.url).ok()?;
    match url.path() {
        "/api/api.getFlannelHttpUrl" => Some("api.getFlannelHttpUrl"),
        "/cache/TWORKFLOW/permissions/info" => Some("permissions/info"),
        "/api/workflows.triggers.preview" => Some("workflows.triggers.preview"),
        "/api/bookmarks.list" => Some("bookmarks.list"),
        "/api/conversations.info" => Some("conversations.info"),
        _ => None,
    }
}
fn add_preflight(request: &PreparedRequest) -> bool {
    method(request) == Some("bookmarks.list")
        && matches!(&request.body, RequestBody::Form(fields)
            if fields.iter().any(|(k,v)| k=="_x_reason" && v=="bookmarks-list-api"))
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let name = method(request)?;
    if add_preflight(request) {
        // Add's duplicate preflight and browse deliberately share an endpoint,
        // but their actual factories have distinct reasons. Preserve Add's
        // original strict validator and ledger, even before navigation.
        return None;
    }
    // Existing Add selection reads retain their original endpoint/ledger.
    if name == "conversations.info" {
        let model = match capture(&state.peek()) {
            Ok(m) => m,
            Err(_) => return Some(Err(closed())),
        };
        if !model.0.lock().unwrap().acknowledged {
            return None;
        }
    }
    Some((|| {
        let s = state.peek();
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let original = raw::channel(raw::DEST).ok_or_else(closed)?;
        let permission = PermissionRead::capture(&original, &session)?;
        let expected = match name {
            "api.getFlannelHttpUrl" => permission.discovery(&client, &session)?,
            "permissions/info" => permission.request(&client, &session, &discovery(&session))?,
            "workflows.triggers.preview" => {
                Read::previews(&[TRIGGER.into()])?.request(&client, &session)
            }
            "bookmarks.list" => Read::bookmarks(raw::DEST)?.request(&client, &session),
            "conversations.info" => api::conversations_info(&client, &session, raw::DEST.into()),
            _ => return Err(closed()),
        };
        super::super::picker::same_request(request, &expected)?;
        let bookmarks = if name == "bookmarks.list" {
            if s.core.active_channel.as_deref() != Some(raw::DEST)
                || s.channel_tab != crate::state::ChannelTab::Workflows
            {
                return Err(closed());
            }
            let parent = super::capture(&s).map_err(|_| closed())?;
            let rows = parent.0.lock().unwrap().bookmarks.clone();
            if rows.len() != 1
                || rows[0]["shortcut_id"] != TRIGGER
                || rows[0]["channel_id"] != raw::DEST
            {
                return Err(closed());
            }
            rows
        } else {
            vec![]
        };
        let mut m = model.0.lock().unwrap();
        if m.reads.len() >= MAX_READS {
            return Err(closed());
        }
        let result = match name {
            "api.getFlannelHttpUrl" => Ok(discovery(&session)),
            "permissions/info" => Ok(
                json!({"ok":true,"allowed":if m.case=="denied" {vec![]}else{vec!["MANAGE_CHANNEL_WORKFLOWS"]},"opaque_receipt":"original-feature-permission"}),
            ),
            "workflows.triggers.preview" => {
                let all = crate::channel_workflows::fixture::raw::previews();
                Ok(
                    json!({"ok":true,"triggers":[all["triggers"][0]],"opaque_receipt":"original-feature-previews"}),
                )
            }
            "bookmarks.list" => Ok(
                json!({"ok":true,"bookmarks":bookmarks,"opaque_receipt":"original-feature-browse-bookmarks"}),
            ),
            "conversations.info" if m.case == "readback-failure" => Err(Error::Offline(
                "Synthetic Feature channel readback unavailable".into(),
            )),
            "conversations.info" => Ok(
                json!({"ok":true,"channel":channel(m.acknowledged,&m.case)?,"opaque_receipt":"original-feature-channel-readback"}),
            ),
            _ => return Err(closed()),
        };
        let serial = m.reads.len() + 1;
        m.reads.push(json!({"serial":serial,"method":name,"body":safe_body(&request.body),
            "endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),
            "owner":{"account":s.core.active_account,"epoch":s.core.account_epoch,"team":session.team_id,"user":session.user_id},
            "result":super::super::receipt(&result)}));
        result
    })())
}

fn inspect(
    request: &protocol::Request,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> Result<Value, Error> {
    let actual = request.inspect();
    let fields = json!([
        ["trigger_ids", TRIGGER],
        ["channel_id", raw::DEST],
        ["_x_reason", protocol::REASON]
    ]);
    if actual["method"] != protocol::METHOD
        || actual["fields"] != fields
        || actual["tokenPresent"] != true
    {
        return Err(closed());
    }
    // The closed Request type retains token/session custody; inspect never
    // exposes the token. Validate its complete endpoint/query separately.
    let expected = client.rest_form(session, protocol::METHOD, vec![]);
    let mut candidate = expected.clone();
    candidate.url = actual["url"].as_str().ok_or_else(closed)?.into();
    super::super::endpoint::same(&candidate, &expected)?;
    Ok(actual)
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
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let wire = inspect(request, &client, &session)?;
        let parent = super::capture(&s).map_err(|_| closed())?;
        if parent
            .0
            .lock()
            .unwrap()
            .bookmarks
            .iter()
            .filter(|b| b["shortcut_id"] == TRIGGER && b["channel_id"] == raw::DEST)
            .count()
            != 1
        {
            return Err(closed());
        }
        let mut m = model.0.lock().unwrap();
        if !m.requests.is_empty() || m.case == "denied" {
            return Err(closed());
        }
        let serial = 1;
        let rejected = m.case == "rejected";
        let raw = if rejected {
            json!({"ok":false,"error":"not_allowed","opaque_receipt":"original-feature-rejected"})
        } else {
            json!({"ok":true,"opaque_receipt":"original-feature-ack"})
        };
        m.requests.push(
            json!({"serial":serial,"request":wire,"owner":{"account":s.core.active_account,
            "epoch":s.core.account_epoch,"team":session.team_id,"user":session.user_id}}),
        );
        m.responses.push(json!({"serial":serial,"raw":raw}));
        let rx = if m.case == "held" {
            let (tx, rx) = oneshot::channel();
            m.held.push((serial, tx));
            Some(rx)
        } else {
            None
        };
        drop(m);
        Ok((model, serial, raw, rejected, rx))
    })();
    let (model, serial, raw, rejected, rx) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
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
        number: 1,
        status: Some(200),
        outcome: if !current {
            protocol::Outcome::Retired
        } else if rejected {
            protocol::Outcome::Rejected
        } else {
            protocol::Outcome::Acknowledged
        },
        raw: Some(raw.clone()),
        response_text: Some(raw.to_string()),
        retry_after_millis: None,
        may_have_applied: !current || !rejected,
    };
    {
        let mut m = model.0.lock().unwrap();
        m.acknowledged = current && !rejected;
        m.attempts.push(attempt.clone());
    }
    observe(attempt);
    settlement.returned = true;
    Some(if !current {
        Err(Error::Transport(
            "Feature owner retired after dispatch; response retained".into(),
        ))
    } else if rejected {
        Err(Error::Api("workflow_feature_api_rejected".into()))
    } else {
        Ok(raw)
    })
}

#[cfg(test)]
mod tests;
