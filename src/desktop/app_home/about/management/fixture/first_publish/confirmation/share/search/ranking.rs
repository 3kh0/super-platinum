//! Closed original observations for Share's mixed ranking. No fixture ranker,
//! canonical writes, cache injection or permission grants live in this module.
use super::*;
mod reads;
mod rows;
#[cfg(test)]
mod tests;
pub(crate) use reads::{cache_read, ordinary, read};
pub(super) use rows::{search_reply, seed};

pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_RANKING_CASE";
pub(crate) const DESTINATION: &str = "CRANKR000";
pub(crate) const DESTINATION_LABEL: &str = "review-remote-00";
pub(crate) fn destination() -> Option<&'static str> {
    (requested() && matches!(case().as_str(), "cold-mixed" | "cached-write")).then_some(DESTINATION)
}
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn requested() -> bool {
    std::env::var(super::CHECK_ENV).as_deref() == Ok("1")
        && std::env::var(super::CASE_ENV).as_deref() == Ok("ranking")
        && matches!(
            case().as_str(),
            "cold-mixed" | "cached-write" | "prefixes" | "stale-cancel" | "read-failure"
        )
}
/// Existing search cases also acquire real ranking observations. The cache
/// outlives the modal; the original named fixture/account still owns every read.
pub(crate) fn enabled(s: &ShellState) -> bool {
    super::super::requested() && capture(s).is_ok() && context(s).is_ok()
}

/// A private namespace for the real navigation storage path. The original
/// synthetic account/epoch/credentials still fence every fixture access.
pub(crate) fn navigation_owner(s: &ShellState) -> Option<String> {
    if !crate::fixture::is_fixture() || !enabled(s) {
        return None;
    }
    let model = capture(s).ok()?;
    let m = model.0.lock().ok()?;
    Some(data(&m)?.navigation_owner.clone())
}

/// Exactly one account-owned fixture observation ledger. Keep the original
/// search receipt path; ordinary Share gets a separate additive field.
fn data(m: &super::super::Data) -> Option<&Data> {
    match (&m.search, &m.ranking) {
        (Some(search), None) => Some(&search.ranking),
        (None, Some(ranking)) => Some(ranking),
        _ => None,
    }
}
fn data_mut(m: &mut super::super::Data) -> Option<&mut Data> {
    match (&mut m.search, &mut m.ranking) {
        (Some(search), None) => Some(&mut search.ranking),
        (None, Some(ranking)) => Some(ranking),
        _ => None,
    }
}

pub(in super::super) struct Data {
    navigation_owner: String,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
}
impl Data {
    pub(in super::super) fn new() -> Self {
        Self {
            navigation_owner: uuid::Uuid::new_v4().to_string(),
            requests: vec![],
            responses: vec![],
            settlements: vec![],
        }
    }
    pub(in super::super) fn snapshot(&self) -> Value {
        json!({"case":if requested(){case()}else if super::requested(){"existing-search".into()}else{"ordinary-share".into()},
            "navigationOwner":self.navigation_owner,
            "requests":self.requests,"responses":self.responses,"settlements":self.settlements,
            "networkCalls":0,"cacheInjection":false,"canonicalWrites":0,
            "localPeople":if requested(){json!(rows::local_people())}else{Value::Null},
            "modelMode":if requested()&&case()=="cached-write"{"actual-read-write-coefficients"}else{"actual-read-absent-model"}})
    }
}

pub(crate) fn startup_response(method: &str, self_raw: &Value) -> Result<Value, Error> {
    reads::reply(method, self_raw, "startup")
}
