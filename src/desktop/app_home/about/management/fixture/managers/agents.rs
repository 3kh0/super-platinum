//! Closed, request-bound app facts for the four manager-agent cases only.
use super::*;
use super_platinum_core::block_actions::app_eligibility::{self, agents as facts};

pub(crate) fn active() -> bool {
    matches!(
        case().as_str(),
        "managers-agents"
            | "managers-agents-recovery"
            | "managers-agents-query-retirement"
            | "managers-agents-account-retirement"
    )
}

pub(super) fn rows(query: &str) -> Vec<Value> {
    if query == "ordinary" {
        return vec![raw::person("UMANLOCAL", "Ordinary Manager")];
    }
    [
        ("UMANNEW01", "Agent Manager", "AMANAGENT", false),
        ("UMANBOT01", "Ordinary Bot Manager", "AMANBOT", false),
        ("UMANAFYES", "Accessible Agent Manager", "AMANAFYES", true),
        ("UMANAFNO1", "Denied Agent Manager", "AMANAFNO1", true),
        ("UMANAFDEF", "Default Agent Manager", "AMANAFDEF", true),
        ("UMANAFDIS", "Disabled Agent Manager", "AMANAFDIS", true),
        ("UMANAFBAD", "Wrong Bot Agent Manager", "AMANAFBAD", true),
    ]
    .into_iter()
    .map(|(id, label, app, agentforce)| {
        let mut row = raw::person(id, label);
        row["is_bot"] = json!(true);
        row["profile"]["api_app_id"] = json!(app);
        if agentforce {
            row["is_agentforce_bot"] = json!(true);
        }
        row
    })
    .collect()
}

fn profile(app: &str) -> Option<Value> {
    let user = match app {
        "AMANAGENT" => "UMANNEW01",
        "AMANBOT" => "UMANBOT01",
        "AMANAFYES" => "UMANAFYES",
        "AMANAFNO1" => "UMANAFNO1",
        "AMANAFDEF" => "UMANAFDEF",
        "AMANAFDIS" => "UMANAFDIS",
        "AMANAFBAD" => "UWRONGBOT",
        _ => return None,
    };
    let mut row = json!({"id":app,"is_agent_app":app!="AMANBOT","bot_user":{"id":user},"opaque":{"preserve":[app,"original-profile"]}});
    if app.starts_with("AMANAF") {
        row["agentforce"] = json!({});
    }
    Some(row)
}

fn loading_snapshot(v: &Value) -> bool {
    let c = &v["confirmation"];
    let m = &c["managerDraft"];
    let facts = &m["agentFacts"];
    v["visible"] == true
        && c["current"] == true
        && c["visit"].as_u64().is_some()
        && c["visit"] == v["visit"]
        && c["document"].as_u64().is_some()
        && c["document"] == v["document"]["id"]
        && c["revision"].as_u64().is_some()
        && c["revision"] == v["document"]["revision"]
        && m["pending"] == true
        && m["generation"].as_u64().is_some()
        && facts["generation"] == m["generation"]
        && m["query"].is_string()
        && facts["query"] == m["query"]
        && facts["status"] == "loading"
}

fn loading(s: &ShellState) -> bool {
    let Some(h) = s.app_home.about.management.surface.as_ref() else {
        return false;
    };
    h.owner.ctx.current(s)
        && loading_snapshot(&crate::app_home::about::management::permissions::snapshot(
            s, h.id,
        ))
}

pub(super) async fn read(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    // Metadata policy also uses client.init and experiment requests. A method
    // (or even identical experiment fields) cannot identify an agent read.
    // Delegate those reads unless the original manager batch is loading now.
    if !active() || !loading(&state.peek()) {
        return None;
    }
    let (method, fields) = match super::super::decode(request) {
        Ok(v) => v,
        Err(_) => return None,
    };
    if !matches!(
        method,
        "client.init"
            | "client.extras"
            | "enterprise.prefs.get"
            | "experiments.getByUser"
            | "aiApps.list"
            | "apps.profile.get"
    ) {
        return None;
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        let ctx = &s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .ok_or_else(closed)?
            .owner
            .ctx;
        let (expected, mut response) = match method {
            "client.init" => {
                let seed = model.0.lock().unwrap().seed.clone().ok_or_else(closed)?;
                (
                    facts::boot_request(&ctx.client, &ctx.session),
                    json!({"ok":true,"self":seed["boot"]["self"],"team":seed["boot"]["team"]}),
                )
            }
            "client.extras" => (
                facts::preferences_request(&ctx.client, &ctx.session),
                json!({"ok":true,"team_prefs":{"toplevel_prefs":{"default_channel_knowledge_agent":{"app_id":"AMANAFDEF"},"ai_apps_settings":{"apps":[{"app_id":"AMANAFDIS","is_ai_enabled":false}]}}}}),
            ),
            // The original seed has no enterprise. This route cannot be
            // silently satisfied by a fabricated organization preference.
            "enterprise.prefs.get" => return Err(closed()),
            "experiments.getByUser" => (
                super_platinum_core::slack::api::list_experiments(&ctx.client, &ctx.session),
                json!({"ok":true,"assignments":{}}),
            ),
            "aiApps.list" => (
                facts::list_request(&ctx.client, &ctx.session, None)?,
                json!({"ok":true,"apps":[profile("AMANAFYES"),profile("AMANAFDIS"),profile("AMANAFBAD")],"response_metadata":{"next_cursor":""},"opaque":{"preserve":["access","original"]}}),
            ),
            "apps.profile.get" => {
                let app = fields
                    .iter()
                    .find(|(k, _)| k == "app")
                    .map(|(_, v)| v.as_str())
                    .ok_or_else(closed)?;
                (
                    app_eligibility::profile_request(&ctx.client, &ctx.session, app)?,
                    json!({"ok":true,"app_profile":profile(app).ok_or_else(closed)?}),
                )
            }
            _ => return Err(closed()),
        };
        wire::same_request(request, &expected)?;
        let (serial, hold) = {
            let mut m = model.0.lock().unwrap();
            let prior = m
                .requests
                .iter()
                .filter(|r| r["phase"] == "agent-fact" && r["method"] == method)
                .count();
            if method == "aiApps.list" && case() == "managers-agents-recovery" && prior == 0 {
                response["apps"][0]["isUnknown"] = json!(true);
            }
            let hold = method == "aiApps.list"
                && !m.held_once
                && (case() == "managers-agents-recovery" && prior == 1
                    || matches!(
                        case().as_str(),
                        "managers-agents-query-retirement" | "managers-agents-account-retirement"
                    ) && prior == 0);
            let serial = m.requests.len() + 1;
            m.requests.push(wire::row(
                serial,
                "agent-fact",
                method,
                fields,
                request.retry_safe(),
                owner(&s),
                false,
            ));
            (serial, hold)
        };
        s.app_home
            .about
            .management
            .fixture
            .as_mut()
            .unwrap()
            .requests
            .push(request.clone());
        Ok((model, serial, hold, response))
    })();
    Some(match prepared {
        Ok((model, serial, hold, response)) => deliver(model, serial, hold, Ok(response)).await,
        Err(error) => Err(error),
    })
}

impl Model {
    /// Passive original receiver observation; never manufactures a settlement.
    pub(crate) fn agent_receiver_retired(&self) -> bool {
        active()
            && self
                .0
                .lock()
                .unwrap()
                .held
                .as_ref()
                .is_some_and(|(_, tx)| tx.is_closed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::manager_settings::eligibility::{self, Eligibility, Member};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn actual_agent_vectors_require_profile_and_independent_access() {
        let members = rows("manager");
        let profiles = members
            .iter()
            .map(|row| {
                let app = row["profile"]["api_app_id"].as_str().unwrap();
                (
                    app.to_owned(),
                    eligibility::decode_profile(
                        app,
                        &json!({"ok":true,"app_profile":profile(app).unwrap()}),
                    )
                    .unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let settings=facts::Settings::decode(&json!({"default_channel_knowledge_agent":{"app_id":"AMANAFDEF"},"ai_apps_settings":{"apps":[{"app_id":"AMANAFDIS","is_ai_enabled":false}]}}),None).unwrap();
        let access = facts::Access::new(
            settings,
            ["AMANAFYES", "AMANAFDIS", "AMANAFBAD"]
                .into_iter()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>(),
            profiles.clone(),
        );
        for row in members {
            let m = Member::decode(&row).unwrap();
            let app = row["profile"]["api_app_id"].as_str().unwrap();
            assert!(m.eligibility("TWORKFLOW", Some(true), None, None) != Eligibility::Allowed);
            let expected = matches!(
                m.user().id.as_str(),
                "UMANNEW01" | "UMANAFYES" | "UMANAFDEF"
            );
            assert_eq!(
                m.eligibility("TWORKFLOW", Some(true), profiles.get(app), Some(&access))
                    == Eligibility::Allowed,
                expected,
                "{}",
                m.user().id
            );
        }
    }

    #[test]
    fn exact_agent_case_gate_and_unknown_raw_refusal() {
        for case in [
            "managers-agents",
            "managers-agents-recovery",
            "managers-agents-query-retirement",
            "managers-agents-account-retirement",
        ] {
            assert!(supported(case));
        }
        assert!(!supported("managers-agents-foreign"));
        let mut raw = json!({"ok":true,"apps":[profile("AMANAFYES").unwrap()],"response_metadata":{"next_cursor":""}});
        assert!(eligibility::decode_page(&raw).is_ok());
        raw["apps"][0]["isUnknown"] = json!(true);
        assert!(eligibility::decode_page(&raw).is_err());
        assert_eq!(raw["apps"][0]["bot_user"]["id"], "UMANAFYES");
    }

    #[test]
    fn metadata_and_stale_batches_never_route_to_agent_fact_adapter() {
        // Even identical experiments.getByUser fields must delegate before
        // or after the actual original fact batch, independently of method.
        let original = json!({"visit":10,"visible":true,
            "document":{"id":20,"revision":30},
            "confirmation":{"current":true,"visit":10,"document":20,"revision":30,
                "managerDraft":{"pending":true,"generation":40,"query":"manager",
                    "agentFacts":{"generation":40,"query":"manager","status":"loading"}}}});
        assert!(loading_snapshot(&original));
        for (pointer, value) in [
            ("/confirmation", Value::Null),
            ("/visible", json!(false)),
            ("/confirmation/current", json!(false)),
            ("/confirmation/visit", json!(11)),
            ("/document/id", json!(21)),
            ("/document/revision", json!(31)),
            ("/confirmation/managerDraft/pending", json!(false)),
            ("/confirmation/managerDraft/generation", json!(41)),
            ("/confirmation/managerDraft/query", json!("ordinary")),
            ("/confirmation/managerDraft/agentFacts", Value::Null),
            (
                "/confirmation/managerDraft/agentFacts/status",
                json!("ready"),
            ),
            (
                "/confirmation/managerDraft/agentFacts/status",
                json!("failed"),
            ),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(!loading_snapshot(&changed), "{pointer}");
        }
    }
}
