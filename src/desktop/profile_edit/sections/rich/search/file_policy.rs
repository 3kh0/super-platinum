//! Captured host-owned file gate. No render-path IO or default-on experiment.
use super::{Search, channel_host::Host, ranking};
use crate::state::ShellState;
use std::sync::Arc;
use super_platinum_core::slack::{Error, api::custom::texty::files};
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("profile_file_policy_unavailable".into())
}

pub(super) struct Observation {
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    context: Arc<crate::scheduled::Context>,
    preferences: Arc<ranking::Preferences>,
    policy: files::Policy,
}
impl Observation {
    pub(super) fn capture(
        shell: &ShellState,
        search: &Search<impl Host>,
        preferences: Arc<ranking::Preferences>,
    ) -> Result<Self, Error> {
        if !search.current(shell) {
            return Err(unavailable());
        }
        Self::bound(
            shell,
            search.lease.clone(),
            search.context.clone(),
            preferences,
        )
    }
    fn bound(
        shell: &ShellState,
        lease: impl Host,
        context: Arc<crate::scheduled::Context>,
        preferences: Arc<ranking::Preferences>,
    ) -> Result<Self, Error> {
        let policy = files::Policy::capture(preferences.assignments())?;
        let value = Self {
            owner: Arc::new(move |shell| lease.current(shell)),
            context,
            preferences,
            policy,
        };
        value
            .owner_current(shell)
            .then_some(value)
            .ok_or_else(unavailable)
    }
    fn owner_current(&self, shell: &ShellState) -> bool {
        (self.owner)(shell) && self.context.current(shell) && self.preferences.current(shell)
    }
    pub(super) fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        self.owner_current(shell)
            && search.current(shell)
            && Arc::ptr_eq(&self.context, &search.context)
    }
    pub(super) fn policy(&self) -> &files::Policy {
        &self.policy
    }
}
