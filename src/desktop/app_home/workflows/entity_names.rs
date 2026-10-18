//! Name preferences acquired under the same actual boot/account/query owner.
//! These observations label/rank entities; they never authorize selection.
use super::*;
use std::collections::BTreeMap;
use super_platinum_core::{
    block_actions::name_policy as policy, composer_emoji::history::History,
    state::posting_metadata::authority::Authority,
};
#[derive(Clone)]
pub(in crate::app_home) struct Observation {
    identity: Arc<()>,
    revision: u64,
    origin: Origin,
    authority: Authority,
    pub(super) policy: policy::Snapshot,
    pub(super) history: History,
    pub(super) enterprise: Option<String>,
    // Complete responses remain separate from the narrow pure name policy.
    // They are not serialized, logged, or reused as unrelated permission.
    pub(super) raw_user: Value,
    pub(super) raw_enterprise: Option<Value>,
    pub(super) raw_teams: BTreeMap<String, Value>,
    pub(super) gantry: Gantry,
}
#[derive(Clone)]
pub(super) struct Gantry {
    pub(super) self_user: Value,
    pub(super) default_workspace: Option<String>,
}
fn gantry<'a>(
    raw: &Value,
    ctx: &Context,
    expected: impl Iterator<Item = &'a str>,
) -> Result<Gantry, Error> {
    use super_platinum_core::block_actions::shortcuts::grid;
    if raw.get("error").is_some_and(|e| !e.is_null()) {
        return Err(unavailable());
    }
    // Reuse the exact client.gantryBoot identity/envelope boundary. The full
    // response is an observation, not a credential or posting-role producer.
    grid::Membership::decode(raw.clone(), &ctx.session)?;
    let actual = raw["workspaces"]
        .as_array()
        .ok_or_else(unavailable)?
        .iter()
        .filter(|row| row["allow_external_users"] != true)
        .map(|row| row["id"].as_str().ok_or_else(unavailable))
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    if actual != expected.collect() {
        return Err(unavailable());
    }
    let default_workspace = match raw.get("default_workspace") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if id.starts_with('T') && !id.is_empty() => Some(id.clone()),
        _ => return Err(unavailable()),
    };
    Ok(Gantry {
        self_user: raw["self"].clone(),
        default_workspace,
    })
}
impl Observation {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && self.authority.current(&s.core)
            && self.revision == s.app_home.workflow_names_revision
            && s.app_home
                .workflow_names
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(&self.identity, &current.identity))
    }
}
pub(super) async fn capture(
    mut state: Signal<ShellState>,
    query: &entity_search::Query,
) -> Result<Observation, Error> {
    {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        if let Some(observed) = s.app_home.workflow_names.as_ref().filter(|v| v.current(&s)) {
            return Ok(observed.clone());
        }
    }
    let revision = state.peek().app_home.workflow_names_revision;
    let authority = entity_posting::authority(state, query).await?;
    let (membership, enterprise) = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        authority
            .name_policy_membership(&s.core, policy::OrdinaryClientV2)
            .map_err(|_| unavailable())?
    };
    let ctx = &query.owner().owner.ctx;
    let raw_gantry = query
        .read(
            state,
            super_platinum_core::block_actions::shortcuts::grid::discovery(
                &ctx.client,
                &ctx.session,
            ),
        )
        .await?;
    let gantry = gantry(&raw_gantry, ctx, membership.workspace_ids())?;
    let raw_user = query
        .read(state, policy::request::user(&ctx.client, &ctx.session))
        .await?;
    let user = policy::UserPrefs::decode(&raw_user).map_err(|_| unavailable())?;
    let history = match raw_user["prefs"].get("frecency") {
        Some(value) => History::from_pref(value).map_err(|_| unavailable())?,
        None => History::default(),
    };
    let raw_enterprise = if enterprise.is_some() {
        Some(
            query
                .read(
                    state,
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
    let mut names =
        policy::Snapshot::new(policy::OrdinaryClientV2, membership, user, enterprise_prefs)
            .map_err(|_| unavailable())?;
    let mut raw_teams = BTreeMap::new();
    for row in raw_gantry["workspaces"]
        .as_array()
        .ok_or_else(unavailable)?
    {
        let id = row["id"].as_str().ok_or_else(unavailable)?;
        if !names.workspace_ids().any(|key| key == id) {
            continue;
        }
        match row.get("prefs") {
            Some(Value::Object(prefs)) => {
                names
                    .observe_team(
                        policy::TeamPrefs::from_complete(id, prefs).map_err(|_| unavailable())?,
                    )
                    .map_err(|_| unavailable())?;
                raw_teams.insert(id.to_owned(), Value::Object(prefs.clone()));
            }
            None | Some(Value::Null) => {}
            _ => return Err(unavailable()),
        }
    }
    for id in names.missing_team_preferences() {
        if !query.current(&state.peek()) || !authority.current(&state.peek().core) {
            return Err(unavailable());
        }
        // mL3A/PAdb selects the exact missing member workspace with target_team;
        // IlU1 keeps the original current credential and route. Do not swap sessions.
        let request =
            policy::request::team_for(&ctx.client, &ctx.session, &id).map_err(|_| unavailable())?;
        let response = query.read(state, request).await?;
        names
            .observe_team(policy::TeamPrefs::decode(&id, &response).map_err(|_| unavailable())?)
            .map_err(|_| unavailable())?;
        raw_teams.insert(id, response["prefs"].clone());
        if names.should_display_real_names() == Ok(true) {
            break;
        }
    }
    names
        .should_display_real_names()
        .map_err(|_| unavailable())?;
    let result = Observation {
        identity: Arc::new(()),
        revision,
        origin: query.owner().clone(),
        authority,
        policy: names,
        history,
        enterprise,
        raw_user,
        raw_enterprise,
        raw_teams,
        gantry,
    };
    let mut s = state.write();
    if !query.current(&s)
        || !result.origin.current(&s)
        || !result.authority.current(&s.core)
        || revision != s.app_home.workflow_names_revision
    {
        return Err(unavailable());
    }
    s.app_home.workflow_names = Some(result.clone());
    Ok(result)
}
pub(in crate::app_home) fn invalidate(s: &mut ShellState, team: &str) {
    if s.core.active_team.as_deref() == Some(team) {
        s.app_home.workflow_names_revision = s.app_home.workflow_names_revision.wrapping_add(1);
        s.app_home.workflow_names = None;
    }
}
fn unavailable() -> Error {
    Error::Api("home_workflow_name_policy_unavailable".into())
}
