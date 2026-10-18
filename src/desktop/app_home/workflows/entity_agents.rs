//! Workflow-owned acquisition of the shared source-backed Agentforce facts.
//! Ordinary bots do not enter this branch and need no Calendar permission grant.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::{
    app_eligibility::{self, agents as facts},
    home::workflow::DirectoryUser,
};

pub(super) async fn load(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    users: &[DirectoryUser],
) -> Result<Option<facts::Access>, Error> {
    let users: Vec<_> = users
        .iter()
        .filter(|u| u.raw()["is_agentforce_bot"] == true)
        .collect();
    if users.is_empty() {
        return Ok(None);
    }
    let ctx = &query.owner().owner.ctx;
    let boot = query
        .read(state, facts::boot_request(&ctx.client, &ctx.session))
        .await?;
    let (current, enterprise) = facts::boot(&ctx.session, &boot)?;
    let prefs = query
        .read(state, facts::preferences_request(&ctx.client, &ctx.session))
        .await?;
    let org = if enterprise {
        Some(
            query
                .read(state, facts::enterprise_request(&ctx.client, &ctx.session))
                .await?,
        )
    } else {
        None
    };
    let settings = facts::settings(&prefs, org.as_ref())?;
    let assignments = query
        .read(state, api::list_experiments(&ctx.client, &ctx.session))
        .await?;
    let mode = facts::list_mode(&assignments, &current)?;
    let mut accessible = BTreeSet::new();
    let mut profiles = BTreeMap::new();
    if mode == facts::ListMode::Fetch {
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        let mut complete = false;
        for _ in 0..50 {
            let request = facts::list_request(&ctx.client, &ctx.session, cursor.as_deref())?;
            let page = facts::Page::decode(&query.read(state, request).await?)?;
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
                        return Err(invalid());
                    }
                    cursor = Some(next);
                }
            }
        }
        if !complete {
            return Err(invalid());
        }
    }
    // A separately observed profile supplies only the source's default-agent
    // exception; it cannot manufacture accessible list membership.
    let apps: BTreeSet<_> = users
        .iter()
        .filter_map(|u| app_eligibility::app_id(u.user()))
        .map(str::to_owned)
        .collect();
    for app in apps {
        if profiles.contains_key(&app) {
            continue;
        }
        let request = app_eligibility::profile_request(&ctx.client, &ctx.session, &app)?;
        let profile = facts::Profile::decode(&app, &query.read(state, request).await?)?;
        profiles.insert(app, profile);
    }
    if !query.current(&state.peek()) {
        return Err(invalid());
    }
    Ok(Some(facts::Access::new(settings, accessible, profiles)))
}
fn invalid() -> Error {
    Error::Api("home_workflow_agent_observation_unavailable".into())
}
