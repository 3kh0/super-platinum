//! Actual name-policy reads under this management owner. No Home dialog's
//! Origin, field inputs or permission authority is borrowed for this consumer.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::{name_policy as policy, shortcuts::grid},
    composer_emoji::history::History,
    state::posting_metadata::authority::{Authority, BootRead},
};
mod scope;
#[cfg(test)]
mod tests;
use scope::{Fence, PendingBoot, Scope};

#[derive(Clone)]
pub(in crate::app_home::about::management::activity) struct Observation {
    identity: Arc<()>,
    owner: Owner,
    visit: Visit,
    document: u64,
    revision: u64,
    cookie: String,
    authority: Authority,
    pub(super) policy: policy::Snapshot,
    pub(super) history: History,
    pub(super) enterprise: Option<String>,
    pub(super) raw_user: Value,
    pub(super) raw_enterprise: Option<Value>,
    pub(super) raw_teams: BTreeMap<String, Value>,
    pub(super) gantry: Value,
    pub(super) default_workspace: Option<String>,
}
impl Observation {
    pub(super) fn current(&self, s: &ShellState, query: &Query) -> bool {
        self.scope_current(s, Scope::Query(query)) && self.installed(s)
    }
    fn installed(&self, s: &ShellState) -> bool {
        document(s, self.visit)
            .and_then(|d| d.activity.member_names.as_ref())
            .is_some_and(|actual| Arc::ptr_eq(&actual.identity, &self.identity))
    }
    fn base_current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && visible(s, self.visit)
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| same_account(&self.owner, &h.owner))
            && document(s, self.visit).is_some_and(|d| d.id == self.document && d.activity.active)
            && self.revision == s.app_home.workflow_names_revision
            && s.core
                .session
                .as_ref()
                .is_some_and(|v| v.d_cookie == self.cookie)
            && self.authority.current(&s.core)
    }
    fn scope_current(&self, s: &ShellState, scope: Scope<'_>) -> bool {
        self.base_current(s)
            && scope.current(s)
            && same_account(&self.owner, scope.owner())
            && self.visit == scope.visit()
            && self.document == scope.document()
    }
}

/// Read-only render observation. Search/filter changes retire picker results,
/// not the already acquired owner-bound name preference facts.
pub(in crate::app_home::about::management::activity) fn observed_policy(
    s: &ShellState,
    visit: u64,
) -> Option<&policy::Snapshot> {
    let value = document(s, visit)?.activity.member_names.as_ref()?;
    (value.visit == visit && value.base_current(s) && value.installed(s)).then_some(&value.policy)
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    query: &Query,
) -> Result<Observation, Error> {
    capture_scope(state, Scope::Query(query)).await
}

pub(in crate::app_home::about::management::activity) async fn capture_detail(
    state: Signal<ShellState>,
    read: &super::super::reads::Read,
) -> Result<Observation, Error> {
    capture_scope(state, Scope::Detail(read)).await
}

pub(in crate::app_home::about::management::activity) async fn capture_refresh(
    state: Signal<ShellState>,
    refresh: &super::super::refresh::Attempt,
) -> Result<Observation, Error> {
    capture_scope(state, Scope::Refresh(refresh)).await
}

async fn capture_scope(
    mut state: Signal<ShellState>,
    scope: Scope<'_>,
) -> Result<Observation, Error> {
    {
        let s = state.peek();
        if !scope.current(&s) {
            return Err(unavailable());
        }
        if let Some(value) = document(&s, scope.visit())
            .and_then(|d| d.activity.member_names.as_ref())
            .filter(|value| value.scope_current(&s, scope) && value.installed(&s))
        {
            return Ok(value.clone());
        }
    }
    let fence = Fence::capture(&state.peek(), scope)?;
    let authority = authority(state, scope, &fence).await?;
    let (membership, enterprise, cookie) = {
        let s = state.peek();
        if !fence.current(&s, scope) {
            return Err(unavailable());
        }
        let (membership, enterprise) = authority
            .name_policy_membership(&s.core, policy::OrdinaryClientV2)
            .map_err(|_| unavailable())?;
        (
            membership,
            enterprise,
            s.core
                .session
                .as_ref()
                .ok_or_else(unavailable)?
                .d_cookie
                .clone(),
        )
    };
    let ctx = &scope.owner().ctx;
    let raw_gantry = owned_request(
        state,
        scope,
        &fence,
        &authority,
        grid::discovery(&ctx.client, &ctx.session),
    )
    .await?;
    let default_workspace = validate_gantry(&raw_gantry, &ctx.session, membership.workspace_ids())?;
    let raw_user = owned_request(
        state,
        scope,
        &fence,
        &authority,
        policy::request::user(&ctx.client, &ctx.session),
    )
    .await?;
    let user = policy::UserPrefs::decode(&raw_user).map_err(|_| unavailable())?;
    let history = match raw_user["prefs"].get("frecency") {
        Some(raw) => History::from_pref(raw).map_err(|_| unavailable())?,
        None => History::default(), // only after the complete successful prefs decode
    };
    let raw_enterprise = if enterprise.is_some() {
        Some(
            owned_request(
                state,
                scope,
                &fence,
                &authority,
                policy::request::enterprise(&ctx.client, &ctx.session),
            )
            .await?,
        )
    } else {
        None
    };
    let enterprise_prefs = enterprise
        .as_ref()
        .zip(raw_enterprise.as_ref())
        .map(|(id, raw)| policy::EnterprisePrefs::decode(id, raw))
        .transpose()
        .map_err(|_| unavailable())?;
    let mut policy =
        policy::Snapshot::new(policy::OrdinaryClientV2, membership, user, enterprise_prefs)
            .map_err(|_| unavailable())?;
    let mut raw_teams = BTreeMap::new();
    for row in raw_gantry["workspaces"]
        .as_array()
        .ok_or_else(unavailable)?
    {
        let id = row["id"].as_str().ok_or_else(unavailable)?;
        if !policy.workspace_ids().any(|key| key == id) {
            continue;
        }
        match row.get("prefs") {
            Some(Value::Object(prefs)) => {
                policy
                    .observe_team(
                        policy::TeamPrefs::from_complete(id, prefs).map_err(|_| unavailable())?,
                    )
                    .map_err(|_| unavailable())?;
                raw_teams.insert(id.to_owned(), Value::Object(prefs.clone()));
            }
            None | Some(Value::Null) => (),
            _ => return Err(unavailable()),
        }
    }
    for id in policy.missing_team_preferences() {
        if !authority.current(&state.peek().core) {
            return Err(unavailable());
        }
        let request =
            policy::request::team_for(&ctx.client, &ctx.session, &id).map_err(|_| unavailable())?;
        let raw = owned_request(state, scope, &fence, &authority, request).await?;
        policy
            .observe_team(policy::TeamPrefs::decode(&id, &raw).map_err(|_| unavailable())?)
            .map_err(|_| unavailable())?;
        raw_teams.insert(id, raw["prefs"].clone());
        if policy.should_display_real_names() == Ok(true) {
            break;
        }
    }
    policy
        .should_display_real_names()
        .map_err(|_| unavailable())?;
    let result = Observation {
        identity: Arc::new(()),
        owner: scope.owner().clone(),
        visit: scope.visit(),
        document: scope.document(),
        revision: fence.revision,
        cookie,
        authority,
        policy,
        history,
        enterprise,
        raw_user,
        raw_enterprise,
        raw_teams,
        gantry: raw_gantry,
        default_workspace,
    };
    if !fence.current(&state.peek(), scope) || !result.scope_current(&state.peek(), scope) {
        return Err(unavailable());
    }
    let _guard = scope.mutation_guard(state).await?;
    if !fence.current(&state.peek(), scope) || !result.scope_current(&state.peek(), scope) {
        return Err(unavailable());
    }
    let mut s = state.write();
    if !fence.current(&s, scope) || !result.scope_current(&s, scope) {
        return Err(unavailable());
    }
    document_mut(&mut s, scope.visit())
        .ok_or_else(unavailable)?
        .activity
        .member_names = Some(result.clone());
    Ok(result)
}

fn validate_gantry<'a>(
    raw: &Value,
    session: &super_platinum_core::config::WorkspaceSession,
    expected: impl Iterator<Item = &'a str>,
) -> Result<Option<String>, Error> {
    if raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    grid::Membership::decode(raw.clone(), session)?;
    let actual = raw["workspaces"]
        .as_array()
        .ok_or_else(unavailable)?
        .iter()
        .filter(|row| row["allow_external_users"] != true)
        .map(|row| row["id"].as_str().ok_or_else(unavailable))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual != expected.collect() {
        return Err(unavailable());
    }
    match raw.get("default_workspace") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if id.starts_with('T') && !id.is_empty() => Ok(Some(id.clone())),
        _ => Err(unavailable()),
    }
}

async fn owned_request(
    state: Signal<ShellState>,
    scope: Scope<'_>,
    fence: &Fence,
    authority: &Authority,
    request: super_platinum_core::slack::PreparedRequest,
) -> Result<Value, Error> {
    fence.request(state, scope, Some(authority), request).await
}

async fn authority(
    mut state: Signal<ShellState>,
    scope: Scope<'_>,
    fence: &Fence,
) -> Result<Authority, Error> {
    let ctx = &scope.owner().ctx;
    if !fence.current(&state.peek(), scope) {
        return Err(unavailable());
    }
    // A warm policy authority needs neither a native lock nor a notifying
    // shell write. Refreshing retained detail must preserve native buffers.
    if let Ok(value) = Authority::capture(&state.peek().core, &ctx.session.team_id) {
        return Ok(value);
    }
    let (read, request) = {
        let _guard = scope.mutation_guard(state).await?;
        if !fence.current(&state.peek(), scope) {
            return Err(unavailable());
        }
        let mut s = state.write();
        if !fence.current(&s, scope) {
            return Err(unavailable());
        }
        if let Ok(value) = Authority::capture(&s.core, &ctx.session.team_id) {
            return Ok(value);
        }
        let read = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session)
            .map_err(|_| unavailable())?;
        let request = match read.request(&s.core) {
            Ok(request) => request,
            Err(_) => {
                read.failed(&mut s.core);
                return Err(unavailable());
            }
        };
        (read, request)
    };
    let mut pending = PendingBoot::new(state, read);
    let result = fence.request(state, scope, None, request).await;
    if !fence.current(&state.peek(), scope) {
        return Err(unavailable());
    }
    let guard = scope.mutation_guard(state).await?;
    if !fence.current(&state.peek(), scope) {
        return Err(unavailable());
    }
    let mut s = state.write();
    if !fence.current(&s, scope) {
        return Err(unavailable());
    }
    let raw = match result {
        Ok(raw) => raw,
        Err(error) => {
            pending.take().failed(&mut s.core);
            return Err(error);
        }
    };
    if !pending.take().apply(&mut s.core, raw, false)? {
        return Err(unavailable());
    }
    let result = Authority::capture(&s.core, &ctx.session.team_id).map_err(|_| unavailable())?;
    drop(s);
    drop(guard);
    crate::bootstrap::persist_workspace_metadata(&state, &ctx.session.team_id);
    Ok(result)
}
