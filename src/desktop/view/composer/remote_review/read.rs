//! Request authority remains opaque. The disconnected native fixture has its
//! own exact owner and never manufactures an authenticated network Binding.
use super::controller::Failure;
use crate::{
    drafts::{
        Fault,
        network::{Binding, RemoteProof},
        remote_review_fixture::{self as fixture, FixtureBinding, FixtureProof},
    },
    state::ShellState,
};
use super_platinum_core::drafts::{
    DraftKey,
    store::sync::{LocalReview, RemoteReview},
};

#[derive(Clone)]
pub(super) enum Source {
    Live(Binding),
    Fixture(FixtureBinding),
}
impl Source {
    pub fn capture(shell: &ShellState, key: &DraftKey) -> Result<Self, Failure> {
        if crate::fixture::is_fixture() || fixture::enabled() {
            // A bad fixture owner must not fall through to a real session.
            return FixtureBinding::capture(shell, key)
                .map(Self::Fixture)
                .ok_or(Failure::Fault(Fault::Stale));
        }
        Binding::capture(shell, &key.team)
            .map(Self::Live)
            .map_err(|_| Failure::Read(None))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Live(binding) => binding.current(shell),
            Self::Fixture(binding) => binding.current(shell),
        }
    }
    pub async fn read(&self, review: LocalReview, now_ms: u64) -> Result<Proof, Failure> {
        match self {
            Self::Live(binding) => {
                let read = binding.read_remote_review(review, now_ms).await;
                read.proof.map(Proof::Live).ok_or_else(|| {
                    if read.cooldown_until_ms.is_some() {
                        Failure::Cooldown
                    } else {
                        Failure::Read(read.issue)
                    }
                })
            }
            Self::Fixture(binding) => binding
                .read(review)
                .await
                .map(Proof::Fixture)
                .map_err(|_| Failure::Read(None)),
        }
    }
    pub async fn checkpoint(&self, phase: &'static str) {
        if let Self::Fixture(binding) = self {
            binding.checkpoint(phase).await;
        }
    }
}

#[derive(Clone)]
pub(super) enum Proof {
    Live(RemoteProof),
    Fixture(FixtureProof),
}
impl Proof {
    pub fn review(&self) -> &RemoteReview {
        match self {
            Self::Live(proof) => proof.review(),
            Self::Fixture(proof) => proof.review(),
        }
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Live(proof) => proof.current(shell),
            Self::Fixture(proof) => proof.current(shell),
        }
    }
    pub fn into_current_review(self, shell: &ShellState) -> Result<RemoteReview, Fault> {
        match self {
            Self::Live(proof) => proof.into_current_review(shell).map_err(|_| Fault::Stale),
            Self::Fixture(proof) => proof.into_current_review(shell).map_err(|_| Fault::Stale),
        }
    }
}
