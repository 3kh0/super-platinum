//! Closed two-human group resolution. Raw membership is never inferred from IDs.
use super::*;
use super_platinum_core::workflow_share::group as wire;
mod open;
mod reads;
pub(crate) use open::open;
pub(crate) use reads::{member, read};

pub(crate) const CHECK_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_MULTIPLE_CHECK";
pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_MULTIPLE_CASE";
pub(crate) const PEERS: [&str; 2] = ["USHARE001", "USHARE002"];
pub(crate) const CHANNEL: &str = "GSHARE001";
pub(crate) const LABEL: &str = "Dana Reviewer, Evan Reviewer";
pub(crate) const DISPLAY: &str = "@dana_reviewer,evan_reviewer";
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
            search::CHECK_ENV,
            search::CASE_ENV,
            "SUPER_PLATINUM_WORKFLOW_SHARE_CANCEL_CHECK",
        ]
        .iter()
        .all(|k| std::env::var(k).unwrap_or_default().is_empty())
        && matches!(
            case().as_str(),
            "success"
                | "reuse"
                | "rejected"
                | "missing-members"
                | "mismatched-members"
                | "held-change-cancel"
        )
}
pub(crate) fn peer(id: &str) -> Option<Value> {
    let label = match id {
        "USHARE001" => "Dana Reviewer",
        "USHARE002" => "Evan Reviewer",
        _ => return None,
    };
    let mut row = super::super::super::picker::person(id, label);
    for key in ["is_app_user", "is_workflow_bot", "is_profile_only"] {
        row[key] = json!(false);
    }
    Some(row)
}
pub(crate) fn channel() -> Value {
    json!({"id":CHANNEL,"name":"mpdm-dana--evan-1","is_mpim":true,"is_im":false,
        "members":["UWORKFLOW","USHARE001","USHARE002"],"context_team_id":"TWORKFLOW",
        "is_private":true,"is_archived":false,"is_frozen":false,"is_ext_shared":false,
        "is_pending_ext_shared":false,"is_read_only":false,"is_thread_only":false,"is_open":true})
}
pub(crate) fn seed(users: &mut Vec<Value>, channels: &mut Vec<Value>) {
    if !requested() {
        return;
    }
    users.extend(PEERS.iter().map(|id| peer(id).unwrap()));
    channels.push(dm::channel());
    if case() == "reuse" {
        channels.push(channel());
    }
}
fn response(scenario: &str) -> Value {
    if scenario == "rejected" {
        return json!({"ok":false,"error":"not_allowed","opaque_receipt":"original-multiple-open-rejected"});
    }
    let mut row = channel();
    row["latest"] = json!({"ts":"1781524809.000001"});
    row["last_read"] = json!("1781524803.000001");
    if scenario == "missing-members" {
        row.as_object_mut().unwrap().remove("members");
    }
    if scenario == "mismatched-members" {
        row["members"] = json!(["UWORKFLOW", "USHARE001", "USHARE003"]);
    }
    json!({"ok":true,"channel":row,"opaque_receipt":"original-multiple-open"})
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
        json!({"case":case(),"peers":PEERS,"channel":CHANNEL,"reads":self.reads,"replies":self.replies,
            "requests":self.requests,"responses":self.responses,"attempts":self.attempts,
            "settlements":self.settlements,"releases":self.releases,"held":if self.held.is_some(){vec![1]}else{vec![]},
            "networkCalls":0,"scheduling":"closed-fixture-hold-until-original-group-capture"})
    }
}
pub(crate) fn release(model: &Model) -> Result<Value, String> {
    let mut m = model.0.lock().unwrap();
    let d = m.multiple.as_mut().ok_or("Multiple model absent")?;
    let tx = d.held.take().ok_or("Original group sender absent")?;
    let value =
        json!({"serial":1,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
    d.releases.push(value.clone());
    Ok(value)
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
    json!({"canonical":canonical.as_ref().and_then(|c|c.row(CHANNEL)),"canonicalCurrent":canonical.as_ref().is_some_and(|c|c.current(s)),
        "raw":raw.as_ref().map(|r|r.value()),"rawCurrent":raw.as_ref().is_some_and(|r|r.current(&s.core)),
        "effective":effective.map(|f|json!({"readOnly":f.read_only,"threadOnly":f.thread_only,"permitsRootPost":f.permits_root_post()})),
        "latest":bag.and_then(|b|b.latest.as_ref()),"lastRead":bag.and_then(|b|b.last_read.as_ref()),"readRevision":bag.map(|b|b.read_revision)})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_original_group_and_sparse_wrong_set_remain_distinct() {
        let peers = PEERS.iter().map(|id| peer(id).unwrap()).collect::<Vec<_>>();
        let plan = wire::Open::new(&peers, "TWORKFLOW", "UWORKFLOW", None).unwrap();
        let raw = response("success");
        let opened = plan.decode(raw.clone()).unwrap();
        assert_eq!(opened.raw(), &raw);
        assert_eq!(opened.id(), CHANNEL);
        assert_eq!(opened.member_ids(), ["UWORKFLOW", "USHARE001", "USHARE002"]);
        for scenario in ["rejected", "missing-members", "mismatched-members"] {
            assert!(plan.decode(response(scenario)).is_err());
        }
        assert!(channel().get("fromAnotherTeam").is_none());
        assert!(channel().get("is_member").is_none());
        assert!(channel().get("latest").is_none());
    }
    #[test]
    fn exact_canonical_open_set_reuses_without_open_request() {
        let peers = PEERS.iter().map(|id| peer(id).unwrap()).collect::<Vec<_>>();
        let mut c = channel();
        c["fromAnotherTeam"] = json!(false);
        assert!(
            wire::Open::new(&peers, "TWORKFLOW", "UWORKFLOW", Some(&c))
                .unwrap()
                .reusable_channel()
                .is_some()
        );
    }
}
