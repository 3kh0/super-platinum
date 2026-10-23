//! Closed existing-human-DM acquisition, separate from the original send ledger.
use super::*;
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{DirectoryEndpoint, member_info};

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_DM_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_DM_CASE";
pub(crate) const CHANNEL: &str = "DSHARE001";
pub(crate) const PEER: &str = "USHARE001";
pub(crate) const LABEL: &str = "Dana Reviewer";
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn requested() -> bool {
    std::env::var(CHECK_ENV).as_deref() == Ok("1")
        && drafts::requested()
        && [
            "SUPER_PLATINUM_WORKFLOW_SHARE_CANCEL_CHECK",
            resend::CHECK_ENV,
            resend::CASE_ENV,
            posting::CHECK_ENV,
            posting::CASE_ENV,
        ]
        .iter()
        .all(|k| std::env::var(k).unwrap_or_default().is_empty())
        && matches!(
            (case().as_str(), super::case().as_str()),
            (
                "success" | "deleted" | "mismatched-peer" | "held-cancel",
                "success"
            ) | ("rejected", "rejected")
        )
}
pub(crate) fn destination() -> &'static str {
    if requested() { CHANNEL } else { raw::DEST }
}
pub(crate) fn peer() -> Value {
    let mut row = super::super::super::picker::person(PEER, LABEL);
    row["is_app_user"] = json!(false);
    row["is_workflow_bot"] = json!(false);
    row["is_profile_only"] = json!(false);
    row
}
pub(crate) fn channel() -> Value {
    json!({"id":CHANNEL,"is_im":true,"user":PEER,"context_team_id":"TWORKFLOW",
        "is_mpim":false,"is_archived":false,"is_frozen":false,
        "is_pending_ext_shared":false,"is_read_only":false,"is_thread_only":false,
        "is_open":true})
}
pub(crate) fn seed(users: &mut Vec<Value>, channels: &mut Vec<Value>) {
    if requested() {
        users.push(peer());
        channels.push(channel());
    }
}
pub(crate) fn discovery() -> Value {
    json!({"ok":true,"url":"https://edgeapi.slack.com/cache/TWORKFLOW","ttl_seconds":60})
}
fn peer_reply(case: &str) -> Value {
    let mut row = peer();
    if case == "deleted" {
        row["deleted"] = json!(true);
    }
    if case == "mismatched-peer" {
        row["id"] = json!("USHARE002");
    }
    let id = row["id"].as_str().unwrap().to_owned();
    json!({"ok":true,"results":[row],"can_interact":{(id):true},"next_marker":"",
        "opaque_receipt":"original-share-dm-peer"})
}
pub(super) struct Data {
    case: String,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Option<(usize, oneshot::Sender<()>)>,
}
impl Data {
    pub(super) fn new() -> Self {
        Self {
            case: case(),
            requests: vec![],
            responses: vec![],
            settlements: vec![],
            releases: vec![],
            held: None,
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"case":self.case,"channel":CHANNEL,"peer":PEER,"requests":self.requests,
            "responses":self.responses,"settlements":self.settlements,"releases":self.releases,
            "held":self.held.as_ref().map(|(n,_)|vec![*n]).unwrap_or_default(),"networkCalls":0})
    }
}
pub(crate) fn release(model: &Model) -> Result<Value, String> {
    let mut m = model.0.lock().unwrap();
    let d = m.dm.as_mut().ok_or("Original DM model absent")?;
    let (serial, tx) = d.held.take().ok_or("Original DM sender absent")?;
    let value =
        json!({"serial":serial,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
    d.releases.push(value.clone());
    Ok(value)
}
fn record(
    s: &ShellState,
    model: &Model,
    request: &PreparedRequest,
    method: &str,
    raw: &Value,
    hold: bool,
) -> Result<(usize, Option<oneshot::Receiver<()>>), Error> {
    let (_, session) = context(s)?;
    let mut m = model.0.lock().unwrap();
    let d = m.dm.as_mut().ok_or_else(closed)?;
    if d.requests.len() >= 12 {
        return Err(closed());
    }
    let serial = d.requests.len() + 1;
    d.requests.push(json!({"serial":serial,"method":method,"body":safe_body(&request.body),
        "endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),"owner":owner(s,&session)}));
    d.responses
        .push(json!({"serial":serial,"raw":raw,"keys":null}));
    let rx = if hold {
        let (tx, rx) = oneshot::channel();
        d.held = Some((serial, tx));
        Some(rx)
    } else {
        None
    };
    Ok((serial, rx))
}
struct Settled {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Settled {
    fn drop(&mut self) {
        if let Some(d) = self.model.0.lock().unwrap().dm.as_mut() {
            d.settlements
                .push(json!({"serial":self.serial,"returned":self.returned}));
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
    let method = crate::app_home::about::management::fixture::decode(request)
        .ok()
        .map(|v| v.0)?;
    if method != "api.getFlannelHttpUrl"
        && !(method == "conversations.info"
            && matches!(&request.body,RequestBody::Form(f) if f.iter().any(|(k,v)| k=="channel"&&v==CHANNEL)))
    {
        return None;
    }
    Some((|| {
        let s = state.peek();
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let (expected, raw) = if method == "api.getFlannelHttpUrl" {
            (
                DirectoryEndpoint::discovery(&client, &session)?,
                discovery(),
            )
        } else {
            (
                api::conversations_info(&client, &session, CHANNEL.into()),
                json!({"ok":true,"channel":channel(),"opaque_receipt":"original-share-dm-channel"}),
            )
        };
        super::super::super::picker::same_request(request, &expected)?;
        let (serial, _) = record(&s, &model, request, &method, &raw, false)?;
        drop(Settled {
            model,
            serial,
            returned: true,
        });
        Ok(raw)
    })())
}
pub(crate) async fn member(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
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
        let now = Instant::now();
        let endpoint = DirectoryEndpoint::decode(&session, &discovery(), now)?;
        let read = member_info::Read::capture(vec![PEER.into()])?;
        super::super::super::picker::same_request(
            request,
            &endpoint.member_info(&client, &session, now, &read)?,
        )?;
        let raw = peer_reply(&case());
        let hold = case() == "held-cancel";
        let (serial, rx) = record(&s, &model, request, "users/info", &raw, hold)?;
        Ok((model, serial, rx, raw))
    })();
    let (model, serial, rx, raw) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settled = Settled {
        model,
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
    settled.returned = true;
    Some(Ok((raw, None)))
}

#[cfg(test)]
mod tests;
