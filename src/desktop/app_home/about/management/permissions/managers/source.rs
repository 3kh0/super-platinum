use super::*;
use super_platinum_core::state::directory_members::Ready;
pub(super) struct Source {
    account: Option<String>,
    native: Value,
    invalidated: std::collections::BTreeSet<String>,
    ready: Option<Ready>,
}
pub(super) struct Row {
    pub id: String,
    pub label: String,
    pub raw: Value,
    pub eligible: bool,
    pub eligibility: &'static str,
}
impl Source {
    pub(super) fn capture(s: &ShellState, o: &Original) -> Option<Self> {
        let w = s.core.workspaces.get(&o.owner.ctx.session.team_id)?;
        Some(Self {
            account: s.core.active_account.clone(),
            native: serde_json::to_value(&w.users).ok()?,
            invalidated: w.directory_members.invalidated(),
            ready: s.core.active_account.as_ref().and_then(|a| {
                w.directory_members
                    .ready(
                        a,
                        &o.owner.ctx.session,
                        o.owner.ctx.epoch,
                        "workflow-managers",
                        &w.users,
                    )
                    .ok()
            }),
        })
    }
    pub(super) fn current(&self, s: &ShellState, o: &Original) -> bool {
        o.owner.current(s)
            && s.core.active_account == self.account
            && s.core
                .workspaces
                .get(&o.owner.ctx.session.team_id)
                .is_some_and(|w| {
                    serde_json::to_value(&w.users).ok().as_ref() == Some(&self.native)
                        && w.directory_members.invalidated() == self.invalidated
                        && self.ready.as_ref().is_none_or(|r| {
                            self.account.as_ref().is_some_and(|a| {
                                r.current(
                                    &w.directory_members,
                                    a,
                                    &o.owner.ctx.session,
                                    o.owner.ctx.epoch,
                                    "workflow-managers",
                                    &w.users,
                                )
                            })
                        })
                })
    }
    pub(super) fn rows(&self, _s: &ShellState, o: &Original) -> BTreeMap<String, Row> {
        self.ready
            .iter()
            .flat_map(|r| r.rows())
            .filter_map(|r| row(&r.source, &o.owner.ctx.session.team_id))
            .map(|r| (r.id.clone(), r))
            .collect()
    }
}
// This owned messaging client implements bootData.app == "client" semantics.
// It is not inferred from the host OS, enterprise identity, or raw agent flags.
pub(super) const CLIENT_CONTEXT: Option<bool> = Some(true);
use protocol::manager_settings::eligibility::{Eligibility, Member};
use super_platinum_core::block_actions::app_eligibility::agents::{Access, Profile};

pub(super) fn row(raw: &Value, team: &str) -> Option<Row> {
    with_facts(raw, team, None, None)
}
pub(super) fn with_facts(
    raw: &Value,
    team: &str,
    profile: Option<&Profile>,
    access: Option<&Access>,
) -> Option<Row> {
    let member = Member::decode(raw).ok()?;
    let id = member.user().id.as_str();
    let label = raw
        .pointer("/profile/display_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            raw.pointer("/profile/real_name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .or_else(|| raw["real_name"].as_str())
        .or_else(|| raw["name"].as_str())?
        .to_owned();
    let eligibility = match member.eligibility(team, CLIENT_CONTEXT, profile, access) {
        Eligibility::Allowed => "allowed",
        Eligibility::Denied => "excluded",
        _ => "unresolved-app-facts",
    };
    Some(Row {
        id: id.into(),
        label,
        raw: raw.clone(),
        eligible: eligibility == "allowed",
        eligibility,
    })
}

/// Carry only a selected, source-admitted manager into this same Permissions
/// transaction's named-run union. Direct run search keeps its own classifier.
pub(super) fn run_row(
    row: &Row,
    selected: &[String],
    context: &protocol::run_settings::Context,
) -> Option<(protocol::run_settings::Entity, settings::Row)> {
    let (mut entity, label) = settings::source::row(&row.raw, context)?;
    // Source union retains a run target after its manager chip is removed.
    // Preserve that prior same-context grant, but never create one from a raw
    // bot flag or from an unselected candidate with no existing grant.
    let granted = context
        .entities
        .iter()
        .any(|e| e.id == row.id && e.eligible);
    if row.eligible && (selected.contains(&row.id) || granted) {
        entity.eligible = true;
    }
    Some((entity, label))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn member() -> Value {
        json!({"id":"UMANAGER1","team_id":"T1","profile":{"real_name":"Manager"}})
    }
    #[test]
    fn local_predicates_use_source_fields_and_no_extra_stranger_or_ultra_exclusion() {
        let mut raw = member();
        raw["is_stranger"] = json!(true);
        raw["is_ultra_restricted"] = json!(true);
        assert!(row(&raw, "T1").unwrap().eligible);
        for key in [
            "deleted",
            "is_restricted",
            "is_profile_only_user",
            "is_workflow_bot",
            "is_slackbot",
            "is_slacksecuritybot",
        ] {
            let mut raw = member();
            raw[key] = json!(true);
            assert!(!row(&raw, "T1").unwrap().eligible);
        }
        let mut raw = member();
        raw["team_id"] = json!("T2");
        assert!(!row(&raw, "T1").unwrap().eligible);
        raw["enterprise_user"] = json!({"teams":["T1"]});
        assert!(row(&raw, "T1").unwrap().eligible);
    }
    #[test]
    fn agent_flags_never_fabricate_profile_access_and_original_raw_is_retained() {
        for key in ["is_bot", "is_app_user", "is_agentforce_bot"] {
            let mut raw = member();
            raw[key] = json!(true);
            raw["profile"]["api_app_id"] = json!("AAGENT001");
            let r = row(&raw, "T1").unwrap();
            assert_eq!(r.eligibility, "unresolved-app-facts");
            assert!(!r.eligible);
            assert_eq!(r.raw, raw);
        }
    }
    #[test]
    fn bot_without_actual_app_association_is_excluded_not_a_profile_request() {
        let mut raw = member();
        raw["is_bot"] = json!(true);
        let r = row(&raw, "T1").unwrap();
        assert!(!r.eligible);
        assert_eq!(r.eligibility, "excluded");
        assert_eq!(r.raw, raw);
    }
}
