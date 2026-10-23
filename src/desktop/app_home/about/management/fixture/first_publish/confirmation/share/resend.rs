//! Explicit user retry is distinct from the original transport's retry loop.
use super::*;

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_RESEND_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_RESEND_CASE";
pub(crate) fn case() -> String {
    if posting::retry_requested() {
        return "posting-restricted".into();
    }
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn requested() -> bool {
    if posting::retry_requested() {
        return true;
    }
    std::env::var(CHECK_ENV).as_deref() == Ok("1")
        && drafts::requested()
        && std::env::var("SUPER_PLATINUM_WORKFLOW_SHARE_CANCEL_CHECK")
            .unwrap_or_default()
            .is_empty()
        && matches!(
            (case().as_str(), super::case().as_str()),
            ("success" | "rejected" | "held", "lost-response") | ("suppressed", "rejected")
        )
}
pub(super) struct Data {
    case: String,
    requests: Vec<Value>,
    responses: Vec<Value>,
    attempts: Vec<protocol::Attempt>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Option<oneshot::Sender<()>>,
}
impl Data {
    pub(super) fn new() -> Self {
        Self {
            case: case(),
            requests: vec![],
            responses: vec![],
            attempts: vec![],
            settlements: vec![],
            releases: vec![],
            held: None,
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"case":self.case,"requests":self.requests,"responses":self.responses,
            "attempts":self.attempts,"settlements":self.settlements,"releases":self.releases,
            "held":if self.held.is_some(){vec![1]}else{vec![]},"networkCalls":0,
            "scheduling":"closed-fixture-hold-until-original-pending-capture"})
    }
}
fn operation(s: &ShellState) -> Option<Value> {
    let h = s.app_home.about.management.surface.as_ref()?;
    let p = crate::app_home::about::management::permissions::finish_up::snapshot(s, h.id);
    p["attempts"].as_array()?.iter().rev().find_map(|a| {
        let op = &a["confirmationShare"]["operation"];
        (a["closed"] == true && op["finished"] == true && op["queued"] == true).then(|| op.clone())
    })
}
pub(super) fn in_flight(s: &ShellState) -> bool {
    operation(s).is_some_and(|op| {
        op["resends"]
            .as_array()
            .and_then(|v| v.last())
            .is_some_and(|r| r["finished"] == false)
    })
}
pub(crate) fn release(model: &Model) -> Result<Value, String> {
    let mut m = model.0.lock().unwrap();
    let data = m.resend.as_mut().ok_or("Original resend model absent")?;
    let tx = data.held.take().ok_or("Original resend sender absent")?;
    let value = json!({"intent":1,"serial":1,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
    data.releases.push(value.clone());
    Ok(value)
}
fn inspect_resend(
    request: &protocol::Request,
    trigger: &Value,
    client: &SlackClient,
    session: &WorkspaceSession,
    initial: &Value,
    failure: Option<&Value>,
) -> Result<Value, Error> {
    let message = request.message();
    let original = inspect(&message.request(client, session)?, trigger, client, session)?;
    if original["fields"] != initial["fields"] || original["method"] != initial["method"] {
        return Err(closed());
    }
    let expected = message.resend_request(client, session, failure)?;
    super::super::super::picker::same_request(request.prepared(), expected.prepared())?;
    Ok(request.inspect())
}
struct Settled {
    model: Model,
    returned: bool,
}
impl Drop for Settled {
    fn drop(&mut self) {
        if let Some(d) = self.model.0.lock().unwrap().resend.as_mut() {
            d.settlements
                .push(json!({"intent":1,"serial":1,"returned":self.returned}));
        }
    }
}
pub(super) async fn mutation(
    state: Signal<ShellState>,
    request: &protocol::Request,
    mut observe: impl FnMut(protocol::Attempt),
) -> Option<Result<Value, Error>> {
    let prepared = (|| {
        let s = state.peek();
        if !requested() || !in_flight(&s) {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let op = operation(&s).ok_or_else(closed)?;
        if op["outcome"] != "unknown" || op["resends"].as_array().map(Vec::len) != Some(1) {
            return Err(closed());
        }
        let parent = super::super::super::capture(&s).map_err(|_| closed())?;
        let trigger = parent.0.lock().unwrap().trigger.clone();
        let mut m = model.0.lock().unwrap();
        if m.case != "lost-response" || m.requests.len() != 3 || m.applied.len() != 1 {
            return Err(closed());
        }
        let initial = &m.requests[0]["request"];
        let failure = (!op["raw"].is_null()).then_some(&op["raw"]);
        let wire = inspect_resend(request, &trigger, &client, &session, initial, failure)?;
        let raw = if case() == "posting-restricted" {
            json!({"ok":false,"error":"restricted_action_read_only_channel","opaque_receipt":"original-share-resend-restricted"})
        } else if case() == "rejected" {
            json!({"ok":false,"error":"not_allowed","opaque_receipt":"original-share-resend-rejected"})
        } else {
            let mut v = reply(request.message(), false);
            v["opaque_receipt"] = json!("original-share-resend-ack");
            v
        };
        // A lost original response already applied this exact identity. Retry
        // must not append another server row, even when the reply is an ACK.
        if m.applied[0]["client_msg_id"] != request.message().client_msg_id()
            || m.applied[0]["blocks"] != *request.message().blocks()
        {
            return Err(closed());
        }
        let d = m.resend.as_mut().ok_or_else(closed)?;
        if d.case != case() || !d.requests.is_empty() {
            return Err(closed());
        }
        d.requests
            .push(json!({"intent":1,"serial":1,"request":wire,"owner":owner(&s,&session)}));
        d.responses.push(
            json!({"intent":1,"serial":1,"raw":raw,"applied":raw["ok"]==true,"deduplicated":true}),
        );
        let (tx, rx) = oneshot::channel();
        d.held = Some(tx);
        drop(m);
        Ok((model, rx, raw, owner(&s, &session)))
    })();
    let (model, rx, raw, original_owner) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settled = Settled {
        model: model.clone(),
        returned: false,
    };
    if !matches!(
        tokio::time::timeout(Duration::from_secs(45), rx).await,
        Ok(Ok(()))
    ) {
        return Some(Err(closed()));
    }
    let current = {
        let s = state.peek();
        context(&s).is_ok_and(|(_, session)| owner(&s, &session) == original_owner)
            && capture(&s).is_ok_and(|m| Arc::ptr_eq(&m.0, &model.0))
    };
    let rejected = raw["ok"] == false;
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
        may_have_applied: !rejected,
    };
    model
        .0
        .lock()
        .unwrap()
        .resend
        .as_mut()
        .unwrap()
        .attempts
        .push(attempt.clone());
    observe(attempt);
    settled.returned = true;
    Some(if !current {
        Err(Error::Transport("Original resend owner retired".into()))
    } else if rejected {
        Err(Error::Api(
            raw["error"].as_str().unwrap_or("not_allowed").into(),
        ))
    } else {
        Ok(raw)
    })
}

#[cfg(test)]
mod tests;
