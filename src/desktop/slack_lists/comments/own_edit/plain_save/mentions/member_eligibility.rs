//! Original 4wTh/fRHe ordinary Save predicates, not popup aD6p policy.
//! The caller retains Context + full Snapshot currentness through issuance.
use crate::directory_member_fetches::{Context, search::Snapshot};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future};
#[path = "member_agents.rs"]
mod agents;
use super_platinum_core::{
    slack::{
        Error, PreparedRequest,
        api::custom::texty::{groups, member_policy::environment},
        models::User,
    },
    slack_lists::comments::edit_seed::local_search::member::{
        self, Flags, GroupFacts, MemberFacts,
    },
};

fn unavailable() -> Error {
    Error::Api("list_save_member_eligibility_unavailable".into())
}
fn missing() -> member::Error {
    member::Error::MissingObservation("original Save member eligibility".into())
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}

/// Positive complete experiments response. Missing assignment is observed off,
/// never a failed request converted into off. Registered OrdinaryClientV2's
/// unified-user default is true (the session auth flag is a different fact).
pub(super) fn flags(assignments: &Value, beyond: bool) -> Result<Flags, Error> {
    if assignments["ok"] != true || assignments.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    let map = assignments["assignments"]
        .as_object()
        .ok_or_else(unavailable)?;
    let group = match map.get("slackforce_seamless_block_app_membership") {
        None | Some(Value::Null) => None,
        Some(Value::Object(row)) => match row.get("group") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.as_str()),
            _ => return Err(unavailable()),
        },
        _ => return Err(unavailable()),
    };
    Ok(Flags {
        unified_user: Some(true),
        salesforce_blocked: Some(group == Some("on")),
        beyond: Some(beyond),
    })
}

pub(super) struct Observation {
    rows: BTreeMap<String, (Value, MemberFacts)>,
    flags: Flags,
    agents: Option<agents::Receipt>,
}
impl Observation {
    /// Only the original source row, never the renderer's derived projection
    /// or a same-ID replacement, can consume these selector facts.
    pub(super) fn member(&self, row: &Value) -> Result<MemberFacts, member::Error> {
        let key = row["id"].as_str().ok_or_else(missing)?;
        let (_, facts) = self
            .rows
            .get(key)
            .filter(|(original, _)| original == row)
            .ok_or_else(missing)?;
        Ok(*facts)
    }
    /// The parent supplies the original captured effective group. Unified
    /// client mode does not reach rLUS.dk, so no locality bool is manufactured.
    pub(super) fn group(&self, row: &groups::Snapshot) -> Result<GroupFacts, member::Error> {
        if self.flags.unified_user != Some(true) {
            return Err(missing());
        }
        let raw = row.effective();
        Ok(GroupFacts {
            from_another_team: None,
            active: Some(
                !truth(&raw["isUnknown"])
                    && !truth(&raw["isNonExistent"])
                    && !truth(&raw["date_delete"]),
            ),
        })
    }
    pub(super) fn snapshot(&self) -> Value {
        let mut value = json!({"registration":{"client":"OrdinaryClientV2","unifiedUser":self.flags.unified_user,"agentEnvironment":true},
            "salesforceBlocked":self.flags.salesforce_blocked,
            "rows":self.rows.iter().map(|(id,(raw,f))| json!({"id":id,"source":raw,
                "interactionAllowed":f.interaction_allowed,"slackbot":f.slackbot,
                "systemNotification":f.system_notification,"searchBot":f.search_bot,
                "channelExpertBot":f.channel_expert_bot,"agentEnvironment":f.agent_environment,
                "agent":f.agent,"admittedAgent":f.admitted_agent,"workflow":f.workflow,
                "deleted":f.deleted,"externalHuddleGuest":f.external_huddle_guest,
                "salesforceApp":f.salesforce_app,"profileOnly":f.profile_only})).collect::<Vec<_>>()});
        if let Some(agents) = &self.agents {
            value["agentAccess"] = agents.snapshot();
        }
        value
    }
}

pub(super) fn capture(
    context: &Context,
    source: &Snapshot,
    flags: Flags,
) -> Result<Observation, Error> {
    if flags.unified_user != Some(true)
        || flags.salesforce_blocked.is_none()
        || flags.beyond.is_none()
    {
        return Err(unavailable());
    }
    let mut app = None;
    let mut rows = BTreeMap::new();
    for current in source.rows() {
        let raw = &current.source;
        let facts = facts(raw, current.interaction, flags, &mut || {
            if let Some(app) = app {
                return Ok(app);
            }
            let actual = environment::salesforce_app(&context.client, &context.session)?;
            app = Some(actual);
            Ok(actual)
        })?;
        let id = raw["id"].as_str().ok_or_else(unavailable)?;
        if rows.insert(id.to_owned(), (raw.clone(), facts)).is_some() {
            return Err(unavailable());
        }
    }
    Ok(Observation {
        rows,
        flags,
        agents: None,
    })
}

/// Complete the reached agent branch once for the entire original Save
/// inventory. The caller retains its source/prefs/policy/serial guard through
/// issuance and passes an execute_current reader using that same predicate.
pub(super) async fn acquire<F, Fut>(
    context: &Context,
    source: &Snapshot,
    flags: Flags,
    assignments: &Value,
    current: &impl Fn() -> bool,
    reader: F,
) -> Result<Observation, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    if !current() {
        return Err(unavailable());
    }
    let mut observation = capture(context, source, flags)?;
    let reached: Vec<_> = observation
        .rows
        .iter()
        .filter(|(_, (raw, f))| {
            !truth(&raw["isUnknown"])
                && !truth(&raw["isNonExistent"])
                && f.interaction_allowed == Some(true)
                && f.agent_environment == Some(true)
                && f.agent == Some(true)
        })
        .map(|(id, _)| id.clone())
        .collect();
    if reached.is_empty() {
        return Ok(observation);
    }
    let receipt = agents::capture(
        &context.client,
        &context.session,
        assignments,
        current,
        &reader,
    )
    .await?;
    if !current() {
        return Err(unavailable());
    }
    let mut app = None;
    for id in reached {
        let (raw, facts) = observation.rows.get_mut(&id).ok_or_else(unavailable)?;
        let user: User = serde_json::from_value(raw.clone()).map_err(|_| unavailable())?;
        facts.admitted_agent = Some(receipt.access.allows(&user, None));
        observe_salesforce(raw, facts, flags, &mut || {
            if let Some(app) = app {
                return Ok(app);
            }
            let actual = environment::salesforce_app(&context.client, &context.session)?;
            app = Some(actual);
            Ok(actual)
        })?;
    }
    if !current() {
        return Err(unavailable());
    }
    observation.agents = Some(receipt);
    Ok(observation)
}

fn facts(
    raw: &Value,
    interaction: Option<bool>,
    flags: Flags,
    salesforce: &mut impl FnMut() -> Result<&'static str, Error>,
) -> Result<MemberFacts, Error> {
    let id = raw["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(unavailable)?;
    if !raw.is_object() {
        return Err(unavailable());
    }
    // th9D.zP only excludes explicit false in an initialized canonical source.
    let mut result = MemberFacts {
        interaction_allowed: Some(interaction != Some(false)),
        ..Default::default()
    };
    if interaction == Some(false) {
        return Ok(result);
    }
    result.slackbot = Some(truth(&raw["is_slackbot"]) || id == "USLACKBOT");
    result.system_notification = Some(id == "USLACK");
    result.search_bot = Some(id == "USEARCH");
    result.channel_expert_bot = Some(id == "UCHANNELEXPERT");
    // XSM4.TZ is bootData.app === 'client'. This is the existing registered
    // native ordinary-client product contract, not an environment guess.
    result.agent_environment = Some(true);
    result.agent = Some(truth(&raw["is_agentforce_bot"]));
    // The synchronous capture does not grant unknown agent access. acquire()
    // fills it only from the original completed catalog/default observation.
    result.admitted_agent = None;
    result.workflow = Some(truth(&raw["is_workflow_bot"]));
    result.deleted = Some(truth(&raw["deleted"]));
    result.external_huddle_guest =
        Some(truth(&raw["is_restricted"]) && raw["restricted_user_type"] == "external_guest");
    result.profile_only = Some(truth(&raw["is_profile_only_user"]));
    // jRFC.l reaches the configured environment only for actual app/bot rows,
    // after the original prior predicates and before profile-only filtering.
    observe_salesforce(raw, &mut result, flags, salesforce)?;
    Ok(result)
}
fn observe_salesforce(
    raw: &Value,
    result: &mut MemberFacts,
    flags: Flags,
    salesforce: &mut impl FnMut() -> Result<&'static str, Error>,
) -> Result<(), Error> {
    let preceding = result.interaction_allowed == Some(true)
        && result.slackbot == Some(false)
        && result.system_notification == Some(false)
        && result.search_bot == Some(false)
        && result.channel_expert_bot == Some(false)
        && (result.agent == Some(false) || result.admitted_agent == Some(true))
        && result.workflow == Some(false)
        && result.deleted == Some(false)
        && result.external_huddle_guest == Some(false);
    if preceding && flags.salesforce_blocked == Some(true) {
        result.salesforce_app = Some(
            if truth(&raw["is_app_user"]) || truth(&raw["is_bot"]) || raw["id"] == "USLACK" {
                raw["profile"]["api_app_id"].as_str() == Some(salesforce()?)
            } else {
                false
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(on: bool) -> Flags {
        Flags {
            unified_user: Some(true),
            salesforce_blocked: Some(on),
            beyond: Some(false),
        }
    }
    #[test]
    fn complete_assignment_absence_is_distinct_from_missing_carrier() {
        assert!(flags(&json!({}), false).is_err());
        assert!(flags(&json!({"ok":true}), false).is_err());
        assert_eq!(
            flags(&json!({"ok":true,"assignments":{}}), true)
                .unwrap()
                .salesforce_blocked,
            Some(false)
        );
        for (group, expected) in [("on", true), ("treatment", false), ("off", false)] {
            assert_eq!(flags(&json!({"ok":true,"assignments":{"slackforce_seamless_block_app_membership":{"group":group}}}),false).unwrap().salesforce_blocked,Some(expected));
        }
        assert!(flags(&json!({"ok":true,"assignments":{"slackforce_seamless_block_app_membership":{"group":true}}}),false).is_err());
    }
    #[test]
    fn initialized_interaction_absence_allows_ordinary_but_agents_remain_unavailable() {
        let raw = json!({"id":"UMEMBER01","profile":{"display_name":"Alex"}});
        let f = facts(&raw, None, options(true), &mut || {
            panic!("ordinary human must not reach Salesforce environment")
        })
        .unwrap();
        assert!(f.allows(true).unwrap());
        assert!(
            !facts(&raw, Some(false), options(true), &mut || panic!())
                .unwrap()
                .allows(true)
                .unwrap()
        );
        let mut agent = raw.clone();
        agent["is_agentforce_bot"] = json!(true);
        assert!(
            facts(&agent, None, options(true), &mut || panic!())
                .unwrap()
                .allows(true)
                .is_err()
        );
        assert_eq!(raw["profile"]["display_name"], "Alex");
    }
    #[test]
    fn fixed_exclusions_and_reached_salesforce_use_source_selectors() {
        for id in ["USLACKBOT", "USLACK", "USEARCH", "UCHANNELEXPERT"] {
            assert!(
                !facts(&json!({"id":id}), None, options(true), &mut || panic!())
                    .unwrap()
                    .allows(true)
                    .unwrap()
            );
        }
        for key in ["deleted", "is_workflow_bot", "is_profile_only_user"] {
            let mut row = json!({"id":"UMEMBER01"});
            row[key] = json!(true);
            assert!(
                !facts(&row, None, options(true), &mut || panic!())
                    .unwrap()
                    .allows(true)
                    .unwrap()
            );
        }
        let row = json!({"id":"UMEMBER01","is_bot":true,"profile":{"api_app_id":"A04T99UKKQE"}});
        let mut calls = 0;
        let f = facts(&row, None, options(true), &mut || {
            calls += 1;
            Ok("A04T99UKKQE")
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert!(!f.allows(true).unwrap());
        assert!(
            facts(&row, None, options(false), &mut || panic!())
                .unwrap()
                .allows(false)
                .unwrap()
        );
    }
    #[test]
    fn exact_member_source_and_group_date_delete_are_independent() {
        let row = json!({"id":"UMEMBER01","name":"Original"});
        let f = facts(&row, None, options(false), &mut || panic!()).unwrap();
        let observation = Observation {
            rows: BTreeMap::from([("UMEMBER01".into(), (row.clone(), f))]),
            flags: options(false),
            agents: None,
        };
        assert!(observation.member(&row).unwrap().allows(false).unwrap());
        assert!(
            observation
                .member(&json!({"id":"UMEMBER01","name":"Changed"}))
                .is_err()
        );
        for (deleted, active) in [
            (json!(null), true),
            (json!(0), true),
            (json!(1), false),
            (json!("0"), false),
        ] {
            let raw = json!({"id":"SGROUP001","name":"Group","date_delete":deleted});
            let group = groups::Snapshot::capture(raw.clone(), raw).unwrap();
            let facts = observation.group(&group).unwrap();
            assert_eq!(facts.from_another_team, None);
            assert_eq!(facts.allows(true).unwrap(), active);
        }
    }

    #[test]
    fn admitted_agent_reaches_salesforce_but_denied_agent_does_not() {
        let raw = json!({"id":"UAGENT001","is_agentforce_bot":true,"is_bot":true,
            "profile":{"api_app_id":"A04T99UKKQE"}});
        let mut facts = facts(&raw, None, options(true), &mut || panic!()).unwrap();
        facts.admitted_agent = Some(false);
        observe_salesforce(&raw, &mut facts, options(true), &mut || panic!()).unwrap();
        assert!(!facts.allows(true).unwrap());
        facts.admitted_agent = Some(true);
        let mut calls = 0;
        observe_salesforce(&raw, &mut facts, options(true), &mut || {
            calls += 1;
            Ok("A04T99UKKQE")
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(facts.salesforce_app, Some(true));
        assert!(!facts.allows(true).unwrap());
    }
}
