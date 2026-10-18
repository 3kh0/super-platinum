//! Exact management policy producers; absent observations stay unknown.
use super::*;
use std::collections::BTreeMap;
use super_platinum_core::block_actions::workflow_management::ACTIVITY_EXPORT_EXPERIMENT;
#[cfg(test)]
mod tests;
use super_platinum_core::block_actions::workflow_management::capabilities::{
    Config, Decision, Facts, FeaturePolicies, MembershipRead, PermissionRead, PolicyReads,
    PreferenceFacts, RoleRead, Scope,
};
#[derive(Clone)]
pub(super) struct Snapshot {
    facts: Facts,
    inputs: Inputs,
    activity_export_group: Option<Value>,
}
#[derive(Clone, PartialEq, Eq)]
struct Inputs {
    cookie: String,
    unified: BTreeMap<String, bool>,
    credentials: BTreeMap<String, Credential>,
}
#[derive(Clone, PartialEq, Eq)]
struct Credential {
    team: String,
    enterprise: Option<String>,
    user: String,
    name: String,
    url: String,
    token: String,
}
impl Inputs {
    fn capture(s: &ShellState) -> Option<Self> {
        let session = s.core.session.as_ref()?;
        Some(Self {
            cookie: session.d_cookie.clone(),
            unified: session.unified_user_client_enabled.clone(),
            credentials: session
                .workspaces
                .iter()
                .map(|(key, w)| {
                    (
                        key.clone(),
                        Credential {
                            team: w.team_id.clone(),
                            enterprise: w.enterprise_id.clone(),
                            user: w.user_id.clone(),
                            name: w.name.clone(),
                            url: w.url.clone(),
                            token: w.token.clone(),
                        },
                    )
                })
                .collect(),
        })
    }
}

impl Snapshot {
    pub fn current(&self, s: &ShellState) -> bool {
        Inputs::capture(s).as_ref() == Some(&self.inputs)
    }
    pub fn can_create(&self) -> bool {
        self.facts.access().can_create().allowed()
    }
    pub fn can_enter(&self) -> bool {
        self.facts.access().can_enter_editor().allowed()
    }
    pub fn can_manage(&self) -> bool {
        self.facts.access().can_view_managed().allowed()
    }
    pub fn activity_export_assignment(&self) -> Option<&Value> {
        self.activity_export_group.as_ref()
    }
    pub fn can_export_activity(&self) -> bool {
        self.can_enter() && self.activity_export_assignment().and_then(Value::as_str) == Some("on")
    }
    pub fn same(&self, other: &Self) -> bool {
        self.inputs == other.inputs
            && self.facts.same_inputs(&other.facts)
            && self.activity_export_group == other.activity_export_group
    }
}
// Config decoding validates this experiments response before snapshot capture.
// Preserve the actual group, including unknown types; absence never enables export.
fn activity_export_group(assignments: &Value) -> Option<Value> {
    assignments
        .get("assignments")?
        .get(ACTIVITY_EXPORT_EXPERIMENT)?
        .get("group")
        .cloned()
}
#[cfg(test)]
pub(super) fn fixture_snapshot(s: &mut ShellState) -> Snapshot {
    let owner = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .owner
        .clone();
    let c = &owner.ctx.client;
    let w = &owner.ctx.session;
    let assignments = fixture::response(s, &PolicyReads::experiments(c, w)).unwrap();
    let boot = fixture::response(s, &PolicyReads::boot(c, w)).unwrap();
    let scope = Scope::decode(w, &boot).unwrap();
    let config = Config::ordinary_client_v2(&assignments).unwrap();
    let features = FeaturePolicies::decode(
        &scope,
        &fixture::response(s, &PolicyReads::features(c, w)).unwrap(),
    )
    .unwrap();
    let preferences = PreferenceFacts::decode(
        &scope,
        &fixture::response(s, &PolicyReads::team_preferences(c, w)).unwrap(),
        None,
        BTreeMap::new(),
        None,
    )
    .unwrap();
    Snapshot {
        facts: Facts::capture(scope, config, Some(features), Some(preferences), None).unwrap(),
        inputs: Inputs::capture(s).unwrap(),
        activity_export_group: activity_export_group(&assignments),
    }
}
#[cfg(test)]
pub(super) fn fixture_export_snapshot(s: &mut ShellState, group: Option<Value>) -> Snapshot {
    let mut snapshot = fixture_snapshot(s);
    snapshot.activity_export_group = group;
    snapshot
}
// Export preflight uses the same policy decoder with native custody around
// fixture/state writes. Ordinary reads retain their existing Lease implementation.
pub(super) trait Query {
    fn owner(&self) -> &Owner;
    fn current(&self, s: &ShellState) -> bool;
    async fn request(
        &self,
        state: Signal<ShellState>,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, Error>;
}
impl Query for read::Lease {
    fn owner(&self) -> &Owner {
        &self.owner
    }
    fn current(&self, s: &ShellState) -> bool {
        self.current(s)
    }
    async fn request(
        &self,
        state: Signal<ShellState>,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, Error> {
        self.request(state, request).await
    }
}
pub(super) async fn acquire(
    state: Signal<ShellState>,
    query: &impl Query,
) -> Result<Snapshot, Error> {
    let inputs = Inputs::capture(&state.peek()).ok_or_else(unavailable)?;
    let c = &query.owner().ctx.client;
    let w = &query.owner().ctx.session;
    let assignments = query.request(state, PolicyReads::experiments(c, w)).await?;
    let config = Config::ordinary_client_v2(&assignments)?;
    let boot = query.request(state, PolicyReads::boot(c, w)).await?;
    let scope = Scope::decode(w, &boot)?;
    let policies = FeaturePolicies::decode(
        &scope,
        &query.request(state, PolicyReads::features(c, w)).await?,
    )?;
    let extras = query
        .request(state, PolicyReads::team_preferences(c, w))
        .await?;
    let enterprise = if w.enterprise_id.as_ref().is_some_and(|id| !id.is_empty()) {
        Some(
            query
                .request(state, PolicyReads::enterprise_preferences(c, w))
                .await?,
        )
    } else {
        None
    };
    let mut roles = BTreeMap::new();
    let mut membership = None;
    let mut preferences = PreferenceFacts::decode(
        &scope,
        &extras,
        enterprise.as_ref(),
        roles.clone(),
        membership.clone(),
    )?;
    let mut permission = None;
    let facts = |preferences: PreferenceFacts, permission| {
        Facts::capture(
            scope.clone(),
            config,
            Some(policies.clone()),
            Some(preferences),
            permission,
        )
    };
    // Exact OR: an acquired positive preference does not require an unrelated
    // permission read. Otherwise its permission branch may independently allow.
    if !facts(preferences.clone(), None)?
        .access()
        .can_create()
        .allowed()
    {
        let read = PermissionRead::capture(&scope)?;
        let discovery = query.request(state, read.discovery(c, w)?).await?;
        let observed = Instant::now();
        let raw = query
            .request(
                state,
                read.request(c, w, &discovery, observed, Instant::now())?,
            )
            .await?;
        permission = Some(read.decode(&raw)?);
    }
    if facts(preferences.clone(), permission.clone())?
        .access()
        .can_create()
        == Decision::Unknown
    {
        let read = MembershipRead::capture(&scope);
        let raw = query.request(state, read.request(c, w)?).await?;
        membership = Some(read.decode(&raw)?);
        preferences = PreferenceFacts::decode(
            &scope,
            &extras,
            enterprise.as_ref(),
            roles.clone(),
            membership.clone(),
        )?;
        // Finite, actual internal inventory; no retry loop or global-role
        // shortcut. Stop once the source's any-workspace rule is decided.
        for team in scope.internal_teams().unwrap_or_default() {
            if facts(preferences.clone(), permission.clone())?
                .access()
                .can_create()
                != Decision::Unknown
            {
                break;
            }
            let read = RoleRead::capture(&scope, config, team)?;
            // Resolve the actual authenticated enterprise session. Query's
            // private Inputs snapshot fences every credential before dispatch,
            // after await and at final admission; no workspace token is relabeled.
            let enterprise_session = {
                let s = state.peek();
                if !query.current(&s) {
                    return Err(unavailable());
                }
                s.core
                    .session
                    .as_ref()
                    .and_then(|session| session.workspaces.get(read.enterprise_team()))
                    .cloned()
                    .ok_or_else(unavailable)?
            };
            let raw = query
                .request(state, read.request(c, w, &enterprise_session)?)
                .await?;
            if let Some(role) = read.decode(&raw)? {
                roles.insert(team.clone(), role);
            }
            preferences = PreferenceFacts::decode(
                &scope,
                &extras,
                enterprise.as_ref(),
                roles.clone(),
                membership.clone(),
            )?;
        }
    }
    if !query.current(&state.peek()) {
        return Err(unavailable());
    }

    if Inputs::capture(&state.peek()).as_ref() != Some(&inputs) {
        return Err(unavailable());
    }
    Ok(Snapshot {
        facts: facts(preferences, permission)?,
        inputs,
        activity_export_group: activity_export_group(&assignments),
    })
}
