//! Complete source Agentforce accessibility under one original Save guard.
//! No popup lease, cache mutation, partial-list grant or automatic retry.
use super::unavailable;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
};
use super_platinum_core::{
    block_actions::app_eligibility::{self, agents as facts},
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};

pub(super) struct Receipt {
    pub(super) access: facts::Access,
    reads: Vec<Value>,
    mode: facts::ListMode,
    assignments: Value,
}
impl Receipt {
    pub(super) fn snapshot(&self) -> Value {
        json!({"mode": match self.mode { facts::ListMode::Fetch => "fetch", facts::ListMode::Skip => "skip" },
            "complete":true,"assignments":self.assignments,"reads":self.reads})
    }
}

pub(super) async fn capture<F, Fut>(
    client: &SlackClient,
    session: &WorkspaceSession,
    assignments: &Value,
    current: &impl Fn() -> bool,
    reader: &F,
) -> Result<Receipt, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    if !current() {
        return Err(unavailable());
    }
    let assignments = assignments.clone();
    let mut reads = Vec::new();
    let boot = read(
        facts::boot_request(client, session),
        current,
        reader,
        &mut reads,
    )
    .await?;
    let (me, enterprise) = facts::boot(session, &boot)?;
    let extras = read(
        facts::preferences_request(client, session),
        current,
        reader,
        &mut reads,
    )
    .await?;
    let org = if enterprise {
        Some(
            read(
                facts::enterprise_request(client, session),
                current,
                reader,
                &mut reads,
            )
            .await?,
        )
    } else {
        None
    };
    let settings = facts::settings(&extras, org.as_ref())?;
    let mode = facts::list_mode(&assignments, &me)?;
    let mut profiles = BTreeMap::new();
    let mut accessible = BTreeSet::new();
    if mode == facts::ListMode::Fetch {
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        let mut complete = false;
        for _ in 0..50 {
            let raw = read(
                facts::list_request(client, session, cursor.as_deref())?,
                current,
                reader,
                &mut reads,
            )
            .await?;
            let page = facts::Page::decode(&raw)?;
            accessible.extend(page.accessible);
            for profile in page.profiles {
                profiles.insert(profile.app().to_owned(), profile);
            }
            match page.cursor {
                None => {
                    complete = true;
                    break;
                }
                Some(next) => {
                    if !seen.insert(next.clone()) {
                        return Err(unavailable());
                    }
                    cursor = Some(next);
                }
            }
        }
        if !complete {
            return Err(unavailable());
        }
    }
    // Outside the completed accessible list only the enabled actual default
    // can grant access. Read exactly that missing profile, including in Skip
    // mode; other member app associations cannot grant the source exception.
    if let Some(app) = settings
        .enabled_default_app()
        .filter(|app| !profiles.contains_key(*app))
    {
        let raw = read(
            app_eligibility::profile_request(client, session, app)?,
            current,
            reader,
            &mut reads,
        )
        .await?;
        profiles.insert(app.to_owned(), facts::Profile::decode(app, &raw)?);
    }
    if !current() {
        return Err(unavailable());
    }
    Ok(Receipt {
        access: facts::Access::new(settings, accessible, profiles),
        reads,
        mode,
        assignments,
    })
}

async fn read<F, Fut>(
    request: PreparedRequest,
    current: &impl Fn() -> bool,
    reader: &F,
    reads: &mut Vec<Value>,
) -> Result<Value, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    if !current() {
        return Err(unavailable());
    }
    let method = request.method.to_owned();
    let raw = reader(request).await;
    // Retired results, including failures, never publish into a later owner.
    if !current() {
        return Err(unavailable());
    }
    let raw = raw?;
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    reads.push(json!({"method":method,"raw":raw}));
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    fn owner() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "TWORKFLOW".into(),
            user_id: "UWORKFLOW".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            url: "https://fixture.slack.com/".into(),
            token: "synthetic".into(),
        }
    }
    fn boot() -> Value {
        json!({"ok":true,"self":{"id":"UWORKFLOW"},"team":{"id":"TWORKFLOW"}})
    }
    fn extras() -> Value {
        json!({"ok":true,"team_prefs":{"toplevel_prefs":{}}})
    }
    fn agent() -> super_platinum_core::slack::models::User {
        serde_json::from_value(json!({"id":"UAGENT001","is_bot":true,"is_agentforce_bot":true,"profile":{"api_app_id":"AAGENT001"}})).unwrap()
    }
    fn profile() -> Value {
        json!({"id":"AAGENT001","agentforce":{},"is_agent_app":true,"bot_user":{"id":"UAGENT001"}})
    }
    async fn load(values: Vec<Value>, assignments: Value) -> Result<Receipt, Error> {
        let values = RefCell::new(values.into_iter());
        capture(
            &SlackClient::default(),
            &owner(),
            &assignments,
            &|| true,
            &|_| std::future::ready(Ok(values.borrow_mut().next().expect("unexpected read"))),
        )
        .await
    }
    #[tokio::test]
    async fn complete_pages_supply_access_and_actual_disabled_settings_exclude() {
        let first = json!({"ok":true,"apps":[],"response_metadata":{"next_cursor":"next"}});
        let last = json!({"ok":true,"apps":[profile()]});
        let receipt = load(
            vec![boot(), extras(), first.clone(), last.clone()],
            json!({"ok":true,"assignments":{}}),
        )
        .await
        .unwrap();
        assert!(receipt.access.allows(&agent(), None));
        assert_eq!(receipt.reads.len(), 4);
        let mut disabled = extras();
        disabled["team_prefs"]["toplevel_prefs"]["ai_apps_settings"] =
            json!({"apps":[{"app_id":"AAGENT001","is_ai_enabled":false}]});
        let receipt = load(
            vec![boot(), disabled, first, last],
            json!({"ok":true,"assignments":{}}),
        )
        .await
        .unwrap();
        assert!(!receipt.access.allows(&agent(), None));
    }
    #[tokio::test]
    async fn skipped_list_requires_actual_enabled_default_profile_for_exception() {
        let assignments =
            json!({"ok":true,"assignments":{"support_lots_of_agents":{"group":"on"}}});
        let mut prefs = extras();
        prefs["team_prefs"]["toplevel_prefs"]["default_channel_knowledge_agent"] =
            json!({"app_id":"AAGENT001"});
        let receipt = load(
            vec![
                boot(),
                prefs.clone(),
                json!({"ok":true,"app_profile":profile()}),
            ],
            assignments.clone(),
        )
        .await
        .unwrap();
        assert_eq!(receipt.mode, facts::ListMode::Skip);
        assert!(receipt.access.allows(&agent(), None));
        assert_eq!(receipt.reads.len(), 3);
        prefs["team_prefs"]["toplevel_prefs"]["ai_apps_settings"] =
            json!({"apps":[{"app_id":"AAGENT001","is_ai_enabled":false}]});
        let receipt = load(vec![boot(), prefs], assignments).await.unwrap();
        assert!(!receipt.access.allows(&agent(), None));
        assert_eq!(receipt.reads.len(), 2);
    }
    #[tokio::test]
    async fn incomplete_cycle_and_error_are_not_empty_catalogs() {
        let page =
            json!({"ok":true,"apps":[profile()],"response_metadata":{"next_cursor":"repeat"}});
        assert!(
            load(
                vec![boot(), extras(), page.clone(), page],
                json!({"ok":true,"assignments":{}})
            )
            .await
            .is_err()
        );
        assert!(
            load(
                vec![boot(), extras(), json!({"ok":true,"error":"bad","apps":[]})],
                json!({"ok":true,"assignments":{}})
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn retirement_after_actual_response_stops_next_read_and_refuses_publication() {
        let live = Cell::new(true);
        let calls = Cell::new(0);
        let receipt = capture(
            &SlackClient::default(),
            &owner(),
            &json!({"ok":true,"assignments":{}}),
            &|| live.get(),
            &|_| {
                calls.set(calls.get() + 1);
                live.set(false);
                std::future::ready(Ok(boot()))
            },
        )
        .await;
        assert!(receipt.is_err());
        assert_eq!(calls.get(), 1);
        let mut reads = Vec::new();
        assert!(
            read(
                facts::boot_request(&SlackClient::default(), &owner()),
                &|| false,
                &|_| async { panic!("retired read dispatched") },
                &mut reads
            )
            .await
            .is_err()
        );
        assert!(reads.is_empty());
    }
}
