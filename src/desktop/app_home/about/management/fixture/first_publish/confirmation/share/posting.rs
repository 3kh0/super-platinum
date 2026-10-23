//! Independent closed restricted-send read; no request may escape this fixture.
use super::*;
use crate::app_home::channel_fetches::{canonical, context::Context, search::direct::PostingRead};
use super_platinum_core::state::posting_metadata::authority::{Authority, RawChannel};

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_POSTING_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_POSTING_CASE";
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
        ]
        .iter()
        .all(|k| std::env::var(k).unwrap_or_default().is_empty())
        && matches!(
            (case().as_str(), super::case().as_str()),
            (
                "success" | "read-failure" | "held-newer" | "missing-policy",
                "rejected"
            ) | ("retry-restricted", "lost-response")
        )
}
pub(crate) fn retry_requested() -> bool {
    requested() && case() == "retry-restricted"
}
fn configure_team(team: &mut Value, case: &str) {
    if matches!(
        case,
        "success" | "read-failure" | "held-newer" | "retry-restricted"
    ) {
        team["plan"] = json!("plus");
    }
}
/// Raw producer input only. The existing BootRead/Init path acquires it; this
/// does not mutate posting metadata or substitute a feature-access observation.
pub(crate) fn seed_team(team: &mut Value) {
    if requested() {
        configure_team(team, &case());
    }
}
pub(super) struct Data {
    case: String,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Option<oneshot::Sender<()>>,
    original: Option<RawChannel>,
    newer: Vec<Value>,
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
            original: None,
            newer: vec![],
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"case":self.case,"requests":self.requests,"responses":self.responses,"settlements":self.settlements,"releases":self.releases,"held":if self.held.is_some(){vec![1]}else{vec![]},"newer":self.newer,"networkCalls":0,"scheduling":"closed-fixture-original-response-held"})
    }
}
pub(crate) fn response(newer: bool) -> Value {
    let mut channel = raw::channel(raw::DEST).expect("closed original channel literal");
    channel["is_read_only"] = json!(!newer);
    channel["is_thread_only"] = json!(newer);
    channel["properties"] =
        json!({"posting_refresh_marker":if newer{"newer-source"}else{"original-refresh"}});
    json!({"ok":true,"channel":channel,"opaque_receipt":if newer{"newer-posting-response"}else{"original-posting-response"}})
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let original = capture(s).ok().and_then(|m| {
        m.0.lock()
            .unwrap()
            .posting
            .as_ref()
            .and_then(|d| d.original.clone())
    });
    let authority = Authority::capture(&s.core, "TWORKFLOW").ok();
    let raw = authority
        .as_ref()
        .and_then(|a| a.raw_channel(&s.core, raw::DEST).ok());
    let flags = authority
        .as_ref()
        .zip(raw.as_ref())
        .ok_or(super_platinum_core::state::posting_metadata::Unknown::Stale)
        .and_then(|(a, r)| a.metadata(&s.core)?.effective(a.configuration(), r.value()));
    let canonical = canonical::observed(s, "TWORKFLOW").ok();
    json!({"authorityCurrent":authority.as_ref().is_some_and(|a|a.current(&s.core)),
        "rawCurrent":raw.as_ref().is_some_and(|r|r.current(&s.core)),"raw":raw.as_ref().map(|r|r.value()),
        "originalRaw":original.as_ref().map(|r|r.value()),"originalCurrent":original.as_ref().map(|r|r.current(&s.core)),
        "effective":flags.as_ref().ok().map(|f|json!({"readOnly":f.read_only,"threadOnly":f.thread_only,"permitsRootPost":f.permits_root_post()})),
        "effectiveFailure":flags.as_ref().err().map(|e|format!("{e:?}")),
        "canonicalCurrent":canonical.as_ref().is_some_and(|c|c.current(s)),"canonical":canonical.as_ref().and_then(|c|c.row(raw::DEST)),
        "typed":s.core.workspaces.get("TWORKFLOW").and_then(|w|w.channels.get(raw::DEST))})
}
pub(crate) fn release(model: &Model) -> Result<Value, String> {
    let mut m = model.0.lock().unwrap();
    let d = m.posting.as_mut().ok_or("Posting model absent")?;
    let tx = d.held.take().ok_or("Original posting sender absent")?;
    let v = json!({"serial":1,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
    d.releases.push(v.clone());
    Ok(v)
}
struct Settled {
    model: Model,
    returned: bool,
}
impl Drop for Settled {
    fn drop(&mut self) {
        if let Some(d) = self.model.0.lock().unwrap().posting.as_mut() {
            d.settlements
                .push(json!({"serial":1,"returned":self.returned}));
        }
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let expected = protocol::posting_refresh::request(&client, &session, raw::DEST)?;
        super::super::super::picker::same_request(request, &expected)?;
        let h = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .ok_or_else(closed)?;
        let p = crate::app_home::about::management::permissions::finish_up::snapshot(&s, h.id);
        let refresh = p["attempts"]
            .as_array()
            .and_then(|v| v.last())
            .and_then(|a| a["confirmationShare"]["operation"]["postingRefresh"].as_array())
            .and_then(|v| v.last())
            .ok_or_else(closed)?;
        if refresh["finished"] != false || refresh["outcome"] != "reading" {
            return Err(closed());
        }
        let mut m = model.0.lock().unwrap();
        let d = m.posting.as_mut().ok_or_else(closed)?;
        if !d.requests.is_empty() {
            return Err(closed());
        }
        d.original = Authority::capture(&s.core, "TWORKFLOW")
            .ok()
            .and_then(|a| a.raw_channel(&s.core, raw::DEST).ok());
        if d.original.is_none() {
            return Err(closed());
        }
        d.requests.push(json!({"serial":1,"generation":refresh["generation"],"method":"conversations.info","body":safe_body(&request.body),"endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),"owner":owner(&s,&session)}));
        let failure = d.case == "read-failure";
        d.responses.push(if failure {
            json!({"serial":1,"error":"offline"})
        } else {
            json!({"serial":1,"raw":response(false)})
        });
        let (tx, rx) = oneshot::channel();
        d.held = Some(tx);
        drop(m);
        Ok((model, rx, failure))
    })();
    let (model, rx, failure) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settled = Settled {
        model,
        returned: false,
    };
    if !matches!(
        tokio::time::timeout(Duration::from_secs(45), rx).await,
        Ok(Ok(()))
    ) {
        return Some(Err(closed()));
    }
    settled.returned = true;
    Some(if failure {
        Err(Error::Offline(
            "Synthetic posting refresh unavailable".into(),
        ))
    } else {
        Ok(response(false))
    })
}
/// An explicit synthetic newer source response, through the real ordered paired
/// publisher. This is not a UI action, permission setter or HTTP request.
pub(crate) async fn newer(state: Signal<ShellState>) -> Result<Value, String> {
    if !requested() || case() != "held-newer" {
        return Err("Named newer-source case required".into());
    }
    let model = capture(&state.peek())?;
    {
        let m = model.0.lock().unwrap();
        let d = m.posting.as_ref().ok_or("Posting data absent")?;
        if d.held.is_none() || !d.newer.is_empty() {
            return Err("Original held read/newer ingress changed".into());
        }
    }
    let ctx =
        Context::capture(&state.peek(), "TWORKFLOW").map_err(|_| "Newer source context absent")?;
    let before = snapshot(&state.peek());
    let read = PostingRead::capture(state, "TWORKFLOW", raw::DEST, &|s| ctx.current(s))
        .map_err(|_| "Newer paired read refused")?;
    let raw = response(true);
    read.publish(state, raw["channel"].clone(), |s| ctx.current(s))
        .await
        .map_err(|_| "Newer paired response not published")?;
    let proof = json!({"kind":"synthetic-source-response-ingress","requestDispatched":false,"raw":raw,"before":before,"after":snapshot(&state.peek())});
    model
        .0
        .lock()
        .unwrap()
        .posting
        .as_mut()
        .unwrap()
        .newer
        .push(proof.clone());
    Ok(proof)
}

#[cfg(test)]
mod tests;
