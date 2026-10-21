//! Profile group admission from the canonical self-member observation.
//! OrdinaryClientV2's unified default is true; session auth flags are unrelated.
use super::{Search, channel_host::Host};
use crate::state::ShellState;
use std::{collections::BTreeSet, sync::Arc};
use super_platinum_core::{
    slack::{Error, api::custom::texty::groups, models::User},
    state::directory_members::Subset,
};
#[cfg(test)]
mod tests;
const GENERATION: &str = "profile-group-eligibility";
fn unavailable() -> Error {
    Error::Api("profile_group_policy_unavailable".into())
}

pub(super) struct Observation {
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    account: String,
    team: String,
    context: Arc<crate::scheduled::Context>,
    source: Subset,
    policy: groups::Policy,
}
impl Observation {
    pub(super) fn capture(shell: &ShellState, search: &Search<impl Host>) -> Result<Self, Error> {
        if !search.current(shell) {
            return Err(unavailable());
        }
        let context = &search.context;
        let w = shell
            .core
            .workspaces
            .get(search.lease.team())
            .ok_or_else(unavailable)?;
        let source = w.directory_members.subset(
            search.lease.account(),
            &context.session,
            context.epoch,
            GENERATION,
            &BTreeSet::from([context.session.user_id.clone()]),
            &w.users,
        )?;
        let policy = selected(&source, &context.session.user_id)?;
        let lease = search.lease.clone();
        Ok(Self {
            owner: Arc::new(move |shell| lease.current(shell)),
            account: search.lease.account().to_owned(),
            team: search.lease.team().to_owned(),
            context: context.clone(),
            source,
            policy,
        })
    }
    pub(super) fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        (self.owner)(shell)
            && search.current(shell)
            && Arc::ptr_eq(&self.context, &search.context)
            && shell.core.workspaces.get(&self.team).is_some_and(|w| {
                self.source.current_rows(
                    &w.directory_members,
                    &self.account,
                    &self.context.session,
                    self.context.epoch,
                    GENERATION,
                    &w.users,
                )
            })
    }
    pub(super) fn policy(&self) -> &groups::Policy {
        &self.policy
    }
}
fn truth(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(false) => false,
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}
fn selected(source: &Subset, self_id: &str) -> Result<groups::Policy, Error> {
    let [row] = source.rows() else {
        return Err(unavailable());
    };
    if row.source["id"].as_str() != Some(self_id)
        || truth(&row.source["isUnknown"])
        || truth(&row.source["isNonExistent"])
    {
        return Err(unavailable());
    }
    let user: User = serde_json::from_value(row.source.clone()).map_err(|_| unavailable())?;
    // Settled aD6p/0dhJ + OrdinaryClientV2 contract, not an auth-mode guess.
    groups::Policy::capture(&user, true)
}
