//! Only capture/execution differ for the explicitly opted-in isolated fixture.
use super::{Account, Fault, ShellState, fixture};
use crate::drafts::network::{self, Binding, Block};
use std::sync::Arc;
use super_platinum_core::drafts::sync::Operation;

pub(super) enum Source {
    Network(Binding),
    Fixture(Arc<fixture::FixtureBinding>),
}
impl Source {
    pub fn capture(shell: &ShellState, team: &str) -> Result<Self, Block> {
        if crate::fixture::is_fixture() {
            return fixture::binding(shell)
                .filter(|binding| fixture::key(binding.target()).team == team)
                .map(Self::Fixture)
                .ok_or(Block::Session);
        }
        Binding::capture(shell, team).map(Self::Network)
    }
    pub fn account(&self, id: &str) -> Result<Arc<Account>, Fault> {
        match self {
            Self::Network(_) => Account::production(id),
            Self::Fixture(binding) if id == fixture::ACCOUNT => Ok(binding.account.clone()),
            Self::Fixture(_) => Err(Fault::Stale),
        }
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Network(binding) => binding.current(shell),
            Self::Fixture(binding) => binding.current(shell),
        }
    }
    pub async fn execute(&self, operation: &Operation, now_ms: u64) -> network::Attempt {
        match self {
            Self::Network(binding) => binding.execute(operation, now_ms).await,
            Self::Fixture(binding) => binding.execute(operation, now_ms).await,
        }
    }
}
