//! Named closed person-selection server. Opening and message delivery are separate.
use super::*;
use std::time::Instant;
use super_platinum_core::{
    block_actions::directory_endpoint::{DirectoryEndpoint, member_info},
    workflow_share::person as wire,
};

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_PERSON_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_PERSON_CASE";
pub(crate) use super::dm::{CHANNEL, LABEL, PEER};
pub(crate) fn case() -> String {
    if search::person_enabled() {
        "success".into()
    } else {
        std::env::var(CASE_ENV).unwrap_or_default()
    }
}
pub(crate) fn requested() -> bool {
    std::env::var(CHECK_ENV).as_deref() == Ok("1")
        && drafts::requested()
        && super::case() == "success"
        && [
            dm::CHECK_ENV,
            dm::CASE_ENV,
            resend::CHECK_ENV,
            resend::CASE_ENV,
            posting::CHECK_ENV,
            posting::CASE_ENV,
            "SUPER_PLATINUM_WORKFLOW_SHARE_CANCEL_CHECK",
        ]
        .iter()
        .all(|k| std::env::var(k).unwrap_or_default().is_empty())
        && matches!(
            case().as_str(),
            "success"
                | "rejected"
                | "missing"
                | "mismatched-peer"
                | "held-cancel"
                | "posting-failure"
        )
}
pub(crate) fn seed(users: &mut Vec<Value>) {
    if requested() {
        users.push(dm::peer());
    }
}
pub(crate) fn channel() -> Value {
    dm::channel()
}
fn response(scenario: &str) -> Value {
    if scenario == "rejected" {
        return json!({"ok":false,"error":"not_allowed","opaque_receipt":"original-person-open-rejected"});
    }
    if scenario == "missing" {
        return json!({"ok":true,"opaque_receipt":"original-person-open-missing"});
    }
    let mut c = channel();
    c["latest"] = json!({"ts":"1781524808.000001"});
    c["last_read"] = json!("1781524802.000001");
    if scenario == "mismatched-peer" {
        c["user"] = json!("USHARE002");
    }
    json!({"ok":true,"channel":c,"opaque_receipt":"original-person-open"})
}
pub(super) struct Data {
    reads: Vec<Value>,
    replies: Vec<Value>,
    requests: Vec<Value>,
    responses: Vec<Value>,
    attempts: Vec<wire::Attempt>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Option<oneshot::Sender<()>>,
}
impl Data {
    pub(super) fn new() -> Self {
        Self {
            reads: vec![],
            replies: vec![],
            requests: vec![],
            responses: vec![],
            attempts: vec![],
            settlements: vec![],
            releases: vec![],
            held: None,
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"case":case(),"peer":PEER,"channel":CHANNEL,"reads":self.reads,"replies":self.replies,
            "requests":self.requests,"responses":self.responses,"attempts":self.attempts,
            "settlements":self.settlements,"releases":self.releases,"held":if self.held.is_some(){vec![1]}else{vec![]},
            "networkCalls":0,"scheduling":"closed-fixture-hold-until-original-selection-capture"})
    }
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    use crate::app_home::channel_fetches::canonical;
    use super_platinum_core::state::posting_metadata::authority::Authority;
    let authority = Authority::capture(&s.core, "TWORKFLOW").ok();
    let raw = authority
        .as_ref()
        .and_then(|a| a.raw_channel(&s.core, CHANNEL).ok());
    let effective = authority.as_ref().zip(raw.as_ref()).and_then(|(a, r)| {
        a.metadata(&s.core)
            .ok()
            .and_then(|m| m.effective(a.configuration(), r.value()).ok())
    });
    let canonical = canonical::observed(s, "TWORKFLOW").ok();
    let bag = s
        .core
        .workspaces
        .get("TWORKFLOW")
        .and_then(|w| w.messages.get(CHANNEL));
    json!({"canonical":canonical.as_ref().and_then(|c|c.row(CHANNEL)),"raw":raw.as_ref().map(|r|r.value()),
        "rawCurrent":raw.as_ref().is_some_and(|r|r.current(&s.core)),
        "effective":effective.map(|f|json!({"readOnly":f.read_only,"threadOnly":f.thread_only,"permitsRootPost":f.permits_root_post()})),
        "latest":bag.and_then(|b|b.latest.as_ref()),"lastRead":bag.and_then(|b|b.last_read.as_ref()),"readRevision":bag.map(|b|b.read_revision)})
}
pub(crate) fn release(model: &Model) -> Result<Value, String> {
    let mut m = model.0.lock().unwrap();
    let d = m.person.as_mut().ok_or("Person model absent")?;
    let tx = d.held.take().ok_or("Original person sender absent")?;
    let value =
        json!({"serial":1,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
    d.releases.push(value.clone());
    Ok(value)
}
fn read_record(
    s: &ShellState,
    request: &PreparedRequest,
    method: &str,
    raw: Option<&Value>,
) -> Result<(), Error> {
    let model = capture(s).map_err(|_| closed())?;
    let (_, session) = context(s)?;
    let mut m = model.0.lock().unwrap();
    let d = m.person.as_mut().ok_or_else(closed)?;
    if d.reads.len() >= 12 {
        return Err(closed());
    }
    let serial = d.reads.len() + 1;
    d.reads.push(json!({"serial":serial,"method":method,"body":safe_body(&request.body),
        "endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),"owner":owner(s,&session)}));
    d.replies.push(match raw {
        Some(v) => json!({"serial":serial,"raw":v,"keys":null}),
        None => json!({"serial":serial,"error":"offline"}),
    });
    Ok(())
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() && !search::person_enabled() {
        return None;
    }
    let method = crate::app_home::about::management::fixture::decode(request)
        .ok()
        .map(|v| v.0)?;
    if method != "api.getFlannelHttpUrl"
        && !(method == "conversations.info"
            && matches!(&request.body,RequestBody::Form(f) if f.iter().any(|(k,v)|k=="channel"&&v==CHANNEL)))
    {
        return None;
    }
    Some((|| {
        let s = state.peek();
        if !active(&s) {
            return Err(closed());
        }
        let (client, session) = context(&s)?;
        let (expected, raw) = if method == "api.getFlannelHttpUrl" {
            (
                DirectoryEndpoint::discovery(&client, &session)?,
                dm::discovery(),
            )
        } else {
            (
                api::conversations_info(&client, &session, CHANNEL.into()),
                json!({"ok":true,"channel":channel(),"opaque_receipt":"original-person-posting"}),
            )
        };
        super::super::super::picker::same_request(request, &expected)?;
        let failed = method == "conversations.info" && case() == "posting-failure";
        read_record(&s, request, &method, (!failed).then_some(&raw))?;
        if failed {
            Err(Error::Offline(
                "Synthetic person posting read unavailable".into(),
            ))
        } else {
            Ok(raw)
        }
    })())
}
pub(crate) async fn member(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    if !requested() && !search::person_enabled() {
        return None;
    }
    Some((|| {
        let s = state.peek();
        if !active(&s) {
            return Err(closed());
        }
        let (client, session) = context(&s)?;
        let now = Instant::now();
        let endpoint = DirectoryEndpoint::decode(&session, &dm::discovery(), now)?;
        let read = member_info::Read::capture(vec![PEER.into()])?;
        super::super::super::picker::same_request(
            request,
            &endpoint.member_info(&client, &session, now, &read)?,
        )?;
        let raw = json!({"ok":true,"results":[dm::peer()],"can_interact":{(PEER):true},"next_marker":"","opaque_receipt":"original-person-peer"});
        read_record(&s, request, "users/info", Some(&raw))?;
        Ok((raw, None))
    })())
}
struct Settled {
    model: Model,
    returned: bool,
}
impl Drop for Settled {
    fn drop(&mut self) {
        if let Some(d) = self.model.0.lock().unwrap().person.as_mut() {
            d.settlements
                .push(json!({"serial":1,"returned":self.returned}));
        }
    }
}
pub(crate) async fn open(
    state: Signal<ShellState>,
    request: &wire::Request,
    current: impl Fn() -> bool,
    mut observe: impl FnMut(wire::Attempt),
) -> Option<Result<Value, Error>> {
    if !requested() && !search::person_enabled() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        if !active(&s) || !current() {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let expected =
            wire::Open::new(&dm::peer(), "TWORKFLOW", None)?.request(&client, &session)?;
        super::super::super::picker::same_request(request.prepared(), expected.prepared())?;
        let raw = response(&case());
        let (tx, rx) = oneshot::channel();
        {
            let mut m = model.0.lock().unwrap();
            let d = m.person.as_mut().ok_or_else(closed)?;
            if !d.requests.is_empty() {
                return Err(closed());
            }
            d.requests
                .push(json!({"serial":1,"request":request.inspect(),"owner":owner(&s,&session)}));
            d.responses
                .push(json!({"serial":1,"raw":raw,"mayHaveOpened":case()!="rejected"}));
            d.held = Some(tx);
        }
        Ok((model, rx, raw))
    })();
    let (model, rx, raw) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settled = Settled {
        model: model.clone(),
        returned: false,
    };
    // Original closed response remains held until the native selection capture.
    // Retirement is still checked by the original supplied owner before adoption.
    if !matches!(
        tokio::time::timeout(Duration::from_secs(45), rx).await,
        Ok(Ok(()))
    ) {
        return Some(Err(closed()));
    }
    let live = current();
    let rejected = raw["ok"] == false;
    let attempt = wire::Attempt {
        number: 1,
        status: Some(200),
        outcome: if !live {
            wire::Outcome::Retired
        } else if rejected {
            wire::Outcome::Rejected
        } else if request.open().decode(raw.clone()).is_ok() {
            wire::Outcome::Acknowledged
        } else {
            wire::Outcome::Unknown
        },
        raw: Some(raw.clone()),
        response_text: Some(raw.to_string()),
        retry_after_millis: None,
        may_have_applied: !rejected,
    };
    model
        .0
        .lock()
        .unwrap()
        .person
        .as_mut()
        .unwrap()
        .attempts
        .push(attempt.clone());
    observe(attempt);
    settled.returned = true;
    Some(if !live {
        Err(Error::Transport(
            "Original person selection retired; response retained".into(),
        ))
    } else if rejected {
        Err(Error::Api("not_allowed".into()))
    } else {
        Ok(raw)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replies_preserve_actual_identity_and_distinct_missing_error() {
        let o = wire::Open::new(&dm::peer(), "TWORKFLOW", None).unwrap();
        assert_eq!(o.decode(response("success")).unwrap().id(), CHANNEL);
        for case in ["rejected", "missing", "mismatched-peer"] {
            assert!(o.decode(response(case)).is_err());
        }
        assert_eq!(response("success")["channel"]["user"], PEER);
    }
    #[test]
    fn no_fake_canonical_fields_in_original_open() {
        let c = channel();
        assert!(c.get("fromAnotherTeam").is_none());
        assert!(c.get("is_member").is_none());
    }
}
