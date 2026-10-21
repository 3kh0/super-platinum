//! Query-owned Profile member eligibility observations. Raw member intake and
//! name policy remain separate; this module never synthesizes Boot or adopts rows.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::app_eligibility,
    slack::{api::custom::texty::member_policy as policy, models::User},
    state::directory_members::Subset,
};
mod agents;
mod caller;
#[cfg(test)]
mod tests;
const GENERATION: &str = "profile-member-eligibility";

struct Witness {
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    account: String,
    context: Arc<crate::scheduled::Context>,
    source: Subset,
    preferences: Arc<ranking::Preferences>,
}
impl Witness {
    fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        (self.owner)(shell)
            && search.current(shell)
            && self.preferences.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|ws| {
                    self.source.current(
                        &ws.directory_members,
                        &self.account,
                        &self.context.session,
                        self.context.epoch,
                        GENERATION,
                        &ws.users,
                    )
                })
    }
}
pub(super) struct Observation {
    witness: Witness,
    rows: BTreeMap<String, (Value, bool)>,
}
impl Observation {
    pub(super) fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        self.witness.current(shell, search)
    }
    /// Only the exact original raw source row can reuse an eligibility result.
    pub(super) fn allows(&self, row: &Value) -> Result<bool, Error> {
        observed(&self.rows, row)
    }
}
fn observed(rows: &BTreeMap<String, (Value, bool)>, row: &Value) -> Result<bool, Error> {
    let id = row
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(unavailable)?;
    let (original, allowed) = rows.get(id).ok_or_else(unavailable)?;
    if original != row {
        return Err(unavailable());
    }
    Ok(*allowed)
}
fn selected(source: &Subset, rows: &[Value]) -> Result<Vec<(User, Option<bool>)>, Error> {
    let original: BTreeMap<_, _> = source
        .rows()
        .iter()
        .filter_map(|r| r.source["id"].as_str().map(|id| (id, r)))
        .collect();
    select_rows(&original, rows)
}
fn select_rows(
    source: &BTreeMap<
        &str,
        &super_platinum_core::block_actions::workflow_management::member_hydration::Current,
    >,
    rows: &[Value],
) -> Result<Vec<(User, Option<bool>)>, Error> {
    let mut seen = BTreeSet::new();
    rows.iter()
        .map(|row| {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(unavailable)?;
            let original = source.get(id).ok_or_else(unavailable)?;
            if !seen.insert(id) || original.source != *row {
                return Err(unavailable());
            }
            let user: User = serde_json::from_value(row.clone()).map_err(|_| unavailable())?;
            if !policy::member_id(&user.id) {
                return Err(unavailable());
            }
            Ok((user, original.interaction))
        })
        .collect()
}
async fn request(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    witness: &Witness,
    request: PreparedRequest,
) -> Result<Value, Error> {
    if !witness.current(&state.peek(), search) {
        return Err(unavailable());
    }
    let raw = search.request(state, request).await?;
    if !witness.current(&state.peek(), search) || !policy::positive(&raw) {
        return Err(unavailable());
    }
    Ok(raw)
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    rows: &[Value],
    preferences: Arc<ranking::Preferences>,
) -> Result<Observation, Error> {
    let (witness, users) = {
        let shell = state.peek();
        if !search.current(&shell) || !preferences.current(&shell) {
            return Err(unavailable());
        }
        let ws = shell
            .core
            .workspaces
            .get(search.lease.team())
            .ok_or_else(unavailable)?;
        let ids: Result<BTreeSet<String>, Error> = rows
            .iter()
            .map(|r| {
                r["id"]
                    .as_str()
                    .filter(|id| policy::member_id(id))
                    .map(str::to_owned)
                    .ok_or_else(unavailable)
            })
            .collect();
        let source = ws.directory_members.subset(
            search.lease.account(),
            &search.context.session,
            search.context.epoch,
            GENERATION,
            &ids?,
            &ws.users,
        )?;
        let users = selected(&source, rows)?;
        (
            Witness {
                owner: {
                    let lease = search.lease.clone();
                    Arc::new(move |shell| lease.current(shell))
                },
                account: search.lease.account().into(),
                context: search.context.clone(),
                source,
                preferences,
            },
            users,
        )
    };
    let flags = caller::Policy::capture(search, &state.peek(), witness.preferences.assignments())?;
    let mut apps = BTreeSet::new();
    let mut agent_users = Vec::new();
    let mut environment = false;
    for (user, interaction) in &users {
        if *interaction == Some(false) {
            continue;
        }
        let needs = flags.requirements(user)?;
        environment |= needs.salesforce_environment;
        if needs.app_profile {
            apps.insert(
                app_eligibility::app_id(user)
                    .ok_or_else(unavailable)?
                    .to_owned(),
            );
        }
        if needs.agent_access {
            agent_users.push(user);
        }
    }
    let salesforce = if environment {
        Some(policy::environment::salesforce_app(
            &search.context.client,
            &search.context.session,
        )?)
    } else {
        None
    };
    let mut profiles = BTreeMap::new();
    let mut restricted = BTreeMap::new();
    for app in apps {
        let raw = request(
            state,
            search,
            &witness,
            app_eligibility::profile_request(
                &search.context.client,
                &search.context.session,
                &app,
            )?,
        )
        .await?;
        app_eligibility::AppProfile::decode(&app, &raw)?;
        // Decode validates identity/shape; absent is_interactable is the source
        // not-restricted case, not an unavailable fetch turned into false.
        restricted.insert(app.clone(), raw["app_profile"]["is_interactable"] == false);
        profiles.insert(app, raw);
    }
    let access = agents::capture(state, search, &witness, &agent_users, &profiles).await?;
    let mut admitted = BTreeMap::new();
    for (row, (user, interaction)) in rows.iter().zip(users) {
        let allowed = if interaction == Some(false) {
            false
        } else {
            flags.allows(
                row,
                &user,
                policy::Facts {
                    restricted_app: app_eligibility::app_id(&user)
                        .and_then(|id| restricted.get(id))
                        .copied(),
                    agent_allowed: access.as_ref().map(|access| access.allows(&user, None)),
                    salesforce_app: salesforce,
                },
            )?
        };
        admitted.insert(user.id, (row.clone(), allowed));
    }
    if !witness.current(&state.peek(), search) {
        return Err(unavailable());
    }
    Ok(Observation {
        witness,
        rows: admitted,
    })
}
