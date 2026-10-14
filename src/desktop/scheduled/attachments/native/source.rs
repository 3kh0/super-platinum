//! One orchestration path with an exact, disconnected fixture executor.
//! Fixture credentials are request-builder metadata only, never shell sessions.
use super::super::{Lease, fixture};
use crate::{scheduled::Context, state::ShellState};
use serde_json::Value;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};

pub(super) enum Source {
    Live(Context),
    Fixture(fixture::Binding),
}

impl Source {
    pub fn capture(shell: &ShellState, lease: &Lease) -> Option<Self> {
        if crate::fixture::is_fixture() {
            // A failed fixture gate never falls through to live credentials.
            return fixture::Binding::capture(shell, lease).map(Self::Fixture);
        }
        let context = Context::capture(shell)?;
        lease
            .current(shell, &context)
            .then_some(Self::Live(context))
    }

    pub fn current(&self, shell: &ShellState, lease: &Lease) -> bool {
        match self {
            Self::Live(context) => lease.current(shell, context),
            Self::Fixture(binding) => binding.current(shell, lease),
        }
    }

    pub fn client(&self) -> &SlackClient {
        match self {
            Self::Live(context) => &context.client,
            Self::Fixture(binding) => binding.client(),
        }
    }

    pub fn session(&self) -> &WorkspaceSession {
        match self {
            Self::Live(context) => &context.session,
            Self::Fixture(binding) => binding.session(),
        }
    }

    pub fn epoch(&self) -> u64 {
        match self {
            Self::Live(context) => context.epoch,
            Self::Fixture(binding) => binding.epoch(),
        }
    }

    pub fn live(&self) -> Option<&Context> {
        match self {
            Self::Live(context) => Some(context),
            Self::Fixture(_) => None,
        }
    }

    pub async fn execute_read(&self, request: PreparedRequest) -> Result<Value, Error> {
        match self {
            Self::Live(context) => context.transport.execute(request).await,
            Self::Fixture(binding) => binding.execute_read(request).await,
        }
    }

    pub async fn execute_mutation(&self, request: PreparedRequest) -> Result<Value, Error> {
        match self {
            Self::Live(context) => context.transport.execute_draft_mutation(request).await,
            Self::Fixture(binding) => binding.execute_mutation(request).await,
        }
    }
}
