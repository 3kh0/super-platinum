//! Named, closed typed destination search. Search is never selection or Send.
use super::*;
use std::time::Instant;
use super_platinum_core::{
    block_actions::directory_endpoint::DirectoryEndpoint, workflow_share::search as wire,
};
pub(crate) mod empty;
mod pages;
pub(crate) mod ranking;
mod read;
pub(crate) use read::{ordinary, read};
pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_SEARCH_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_SEARCH_CASE";
pub(crate) const CHANNEL: &str = "CSEARCH001";
pub(crate) const LABEL: &str = "research-destination";
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn requested() -> bool {
    std::env::var(CHECK_ENV).as_deref() == Ok("1")
        && drafts::requested()
        && super::case() == "success"
        && [
            dm::CHECK_ENV,
            dm::CASE_ENV,
            person::CHECK_ENV,
            person::CASE_ENV,
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
            "person-success"
                | "channel-success"
                | "prefixes"
                | "rapid-query"
                | "held-cancel"
                | "rejected"
                | "offline"
                | "ranking"
                | "empty"
                | "startup"
        )
        && (case() != "ranking" || ranking::requested())
        && (case() != "empty" || empty::requested())
        && (case() != "startup" || super::startup::requested())
}
pub(crate) fn person_enabled() -> bool {
    requested() && case() == "person-success"
}
pub(crate) fn seed(users: &mut Vec<Value>) {
    if requested() {
        if ranking::requested() {
            ranking::seed(users);
            return;
        }
        for n in 0..if empty::requested() || super::startup::requested() {
            75
        } else {
            51
        } {
            users.push(pages::person(n));
        }
    }
}
pub(crate) fn channel() -> Value {
    let mut row = raw::channel(raw::DEST).unwrap();
    row["id"] = json!(CHANNEL);
    row["name"] = json!(LABEL);
    row
}
pub(super) struct Data {
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<()>)>,
    queries: Vec<String>,
    posting_sources: Vec<Value>,
    ranking: ranking::Data,
}
impl Data {
    pub(super) fn new() -> Self {
        Self {
            requests: vec![],
            responses: vec![],
            settlements: vec![],
            releases: vec![],
            held: vec![],
            queries: vec![],
            posting_sources: vec![],
            ranking: ranking::Data::new(),
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        let mut value = json!({"case":case(),"postingSources":self.posting_sources,"requests":self.requests,"responses":self.responses,"settlements":self.settlements,"releases":self.releases,"held":self.held.iter().map(|(n,_)|n).collect::<Vec<_>>(),"networkCalls":0,"scheduling":"first-search-page-per-wire-query-held-until-native-capture","initialPeople":if ranking::requested(){42}else if empty::requested() || super::startup::requested(){75}else{51},"remotePersonInitiallyAbsent":true,"remoteChannelInitiallyAbsent":true,"ranking":self.ranking.snapshot()});
        if empty::requested() {
            value["emptyCase"] = json!(empty::case());
        }
        value
    }
}
pub(crate) fn release(model: &Model, serial: usize) -> Result<Value, String> {
    let mut m = model.0.lock().unwrap();
    let d = m.search.as_mut().ok_or("Search fixture absent")?;
    let i = d
        .held
        .iter()
        .position(|(n, _)| *n == serial)
        .ok_or("Original search sender absent")?;
    let (_, tx) = d.held.remove(i);
    let value =
        json!({"serial":serial,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
    d.releases.push(value.clone());
    Ok(value)
}
struct Settlement {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Settlement {
    fn drop(&mut self) {
        if let Some(d) = self.model.0.lock().unwrap().search.as_mut() {
            d.settlements
                .push(json!({"serial":self.serial,"returned":self.returned}));
        }
    }
}
