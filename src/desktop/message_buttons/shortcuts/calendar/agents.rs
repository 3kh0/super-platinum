//! Dialog/query-owned agent facts; no global grants or additional write path.
use super::*;
use super_platinum_core::{
    block_actions::app_eligibility::{self, agents as facts},
    slack::models::User,
};

#[derive(Clone, Default)]
pub(super) struct Facts {
    profiles: BTreeMap<String, facts::Profile>,
    access: Option<facts::Access>,
    slackbot: Option<facts::slackbot::Observation>,
    // Search responses are not globally upserted. Retain the exact users needed
    // to recheck selected remote guests at final Create admission.
    users: BTreeMap<String, User>,
}
pub(super) fn eligible(
    c: &Calendar,
    user: &User,
    workspace: &super_platinum_core::config::WorkspaceSession,
) -> bool {
    if !load::base_eligible(user, &workspace.user_id) {
        return false;
    }
    if facts::slackbot::is_slackbot(user)
        && !c
            .agents
            .slackbot
            .as_ref()
            .is_some_and(|p| p.allows(workspace))
    {
        return false;
    }
    let app = app_eligibility::app_id(user);
    let profile = app.and_then(|app| {
        c.agents
            .access
            .as_ref()
            .and_then(|a| a.profile(app))
            .or_else(|| c.agents.profiles.get(app))
    });
    if user.extra.get("is_agentforce_bot") == Some(&serde_json::Value::Bool(true))
        && !c
            .agents
            .access
            .as_ref()
            .is_some_and(|access| access.allows(user, profile))
    {
        return false;
    }
    (!user.is_bot && user.extra.get("is_app_user") != Some(&serde_json::Value::Bool(true)))
        || profile.is_some_and(|profile| profile.agent_for(user))
}
pub(super) fn selected_current(s: &ShellState, id: u64, user: &str) -> bool {
    let Some(c) = active(s, id) else {
        return false;
    };
    let Some(ctx) = context(s, id) else {
        return false;
    };
    s.core
        .workspaces
        .get(&ctx.session.team_id)
        .and_then(|w| w.users.get(user))
        .or_else(|| c.agents.users.get(user))
        .is_some_and(|user| eligible(c, user, &ctx.session))
}
fn current_query(s: &ShellState, id: u64, generation: u64) -> bool {
    active(s, id).is_some_and(|c| {
        c.guest_generation == generation
            && matches!(c.stage, Stage::Loading | Stage::Editing)
            && !c.confirm_close
    })
}
async fn read(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Context,
    request: super_platinum_core::slack::PreparedRequest,
) -> Result<serde_json::Value, Error> {
    if !current_query(&state.peek(), id, generation) {
        return Err(cal::invalid());
    }
    let result = load::read(state, id, ctx, request).await?;
    if !current_query(&state.peek(), id, generation) {
        return Err(cal::invalid());
    }
    Ok(result)
}

pub(super) async fn hydrate(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Context,
    users: Vec<User>,
) {
    let mut users_by_id = BTreeMap::new();
    {
        let s = state.peek();
        if !current_query(&s, id, generation) {
            return;
        }
        let c = active(&s, id).unwrap();
        // Selected remote users survive a different query, but fresh workspace
        // metadata takes precedence over the original search response.
        for person in &c.guests {
            if let Some(user) = s
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .and_then(|w| w.users.get(&person.id))
                .or_else(|| c.agents.users.get(&person.id))
            {
                users_by_id.insert(user.id.clone(), user.clone());
            }
        }
        for user in users.into_iter().take(512) {
            let user = s
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .and_then(|w| w.users.get(&user.id))
                .unwrap_or(&user)
                .clone();
            users_by_id.insert(user.id.clone(), user);
        }
    }
    let mut next = Facts {
        users: users_by_id,
        ..Facts::default()
    };
    let apps: std::collections::BTreeSet<String> = next
        .users
        .values()
        .filter(|u| load::base_eligible(u, &ctx.session.user_id))
        .filter_map(app_eligibility::app_id)
        .map(str::to_owned)
        .collect();
    for app in apps {
        if !current_query(&state.peek(), id, generation) {
            return;
        }
        let Ok(request) = app_eligibility::profile_request(&ctx.client, &ctx.session, &app) else {
            continue;
        };
        let result = read(state, id, generation, ctx, request)
            .await
            .and_then(|raw| facts::Profile::decode(&app, &raw));
        if let Ok(profile) = result {
            next.profiles.insert(app, profile);
        }
    }
    if next.users.values().any(facts::slackbot::is_slackbot) {
        next.slackbot = slackbot(state, id, generation, ctx).await.ok();
    }
    if next
        .users
        .values()
        .any(|u| u.extra.get("is_agentforce_bot") == Some(&serde_json::Value::Bool(true)))
    {
        next.access = access(state, id, generation, ctx).await.ok();
    }
    let mut s = state.write();
    if !current_query(&s, id, generation) {
        return;
    }
    active_mut(&mut s, id).unwrap().agents = next;
}
async fn slackbot(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Context,
) -> Result<facts::slackbot::Observation, Error> {
    let discovery = facts::slackbot::discovery(&ctx.client, &ctx.session)?;
    let endpoint = read(state, id, generation, ctx, discovery).await?;
    let request = facts::slackbot::request(&ctx.client, &ctx.session, &endpoint)?;
    let raw = read(state, id, generation, ctx, request).await?;
    facts::slackbot::Observation::decode(&ctx.session, &raw)
}
async fn access(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Context,
) -> Result<facts::Access, Error> {
    let boot = read(
        state,
        id,
        generation,
        ctx,
        facts::boot_request(&ctx.client, &ctx.session),
    )
    .await?;
    let (user, enterprise) = facts::boot(&ctx.session, &boot)?;
    let extras = read(
        state,
        id,
        generation,
        ctx,
        facts::preferences_request(&ctx.client, &ctx.session),
    )
    .await?;
    let org = if enterprise {
        Some(
            read(
                state,
                id,
                generation,
                ctx,
                facts::enterprise_request(&ctx.client, &ctx.session),
            )
            .await?,
        )
    } else {
        None
    };
    let settings = facts::settings(&extras, org.as_ref())?;
    let assignments = read(
        state,
        id,
        generation,
        ctx,
        api::list_experiments(&ctx.client, &ctx.session),
    )
    .await?;
    let mode = facts::list_mode(&assignments, &user)?;
    let mut accessible = std::collections::BTreeSet::new();
    let mut profiles = BTreeMap::new();
    if mode == facts::ListMode::Fetch {
        let mut cursor: Option<String> = None;
        let mut seen = std::collections::BTreeSet::new();
        // Exact VLYq source ceiling. A repeated cursor is not progress.
        for _ in 0..50 {
            let request = facts::list_request(&ctx.client, &ctx.session, cursor.as_deref())?;
            let page = facts::Page::decode(&read(state, id, generation, ctx, request).await?)?;
            accessible.extend(page.accessible);
            for profile in page.profiles {
                profiles.insert(profile.app().to_owned(), profile);
            }
            let Some(next) = page.cursor else {
                break;
            };
            if !seen.insert(next.clone()) {
                return Err(cal::invalid());
            }
            cursor = Some(next);
        }
    }
    Ok(facts::Access::new(settings, accessible, profiles))
}

#[cfg(test)]
mod tests;
