//! Read custody is independent of the usage writer. The named offline reader
//! has no Transport and can only call its closed fixture dispatcher.
use super::*;
use crate::slack_lists::comments::fixture::{Model, reaction_additions as fixture};
use super_platinum_core::slack::{Error, PreparedRequest, SlackClient};

pub(super) enum ReadOwner {
    Native(Arc<Owner>),
    Fixture(Fixture),
}
pub(super) struct Fixture {
    model: Model,
    account: Option<String>,
    epoch: u64,
    session: WorkspaceSession,
    client: SlackClient,
}
impl ReadOwner {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        if fixture::enabled(shell) {
            return Some(Self::Fixture(Fixture {
                model: fixture::model(shell)?,
                account: shell.core.active_account.clone(),
                epoch: shell.core.account_epoch,
                session: crate::slack_lists::comments::fixture::compose::session(shell).ok()?,
                client: shell.core.client.clone(),
            }));
        }
        Owner::capture(shell).map(Arc::new).map(Self::Native)
    }
    pub fn team(&self) -> &str {
        match self {
            Self::Native(o) => &o.context.session.team_id,
            Self::Fixture(o) => &o.session.team_id,
        }
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Native(o) => o.current(shell),
            Self::Fixture(o) => {
                fixture::owns(shell, &o.model)
                    && shell.core.active_account == o.account
                    && shell.core.account_epoch == o.epoch
                    && shell.core.active_team.as_deref() == Some(&o.session.team_id)
                    && shell
                        .core
                        .workspaces
                        .get(&o.session.team_id)
                        .is_some_and(|w| w.self_user_id == o.session.user_id)
            }
        }
    }
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Native(a), Self::Native(b)) => a.same(b),
            (Self::Fixture(a), Self::Fixture(b)) => {
                fixture::same_model(&a.model, &b.model)
                    && a.account == b.account
                    && a.epoch == b.epoch
                    && same_session(&a.session, &b.session)
            }
            _ => false,
        }
    }
    pub fn request(&self) -> PreparedRequest {
        match self {
            Self::Native(o) => api::preferences_request(&o.context.client, &o.context.session),
            Self::Fixture(o) => api::preferences_request(&o.client, &o.session),
        }
    }
    pub async fn execute<B: Boundary>(
        &self,
        boundary: &B,
        state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        match self {
            Self::Native(o) => boundary.execute(o.clone(), request).await,
            Self::Fixture(o) => fixture::preferences_request(state, &o.model, &request).await,
        }
    }
    pub fn persist<B: Boundary>(&self, boundary: &B, state: Signal<ShellState>) {
        match self {
            Self::Native(_) => boundary.persist(state, self.team()),
            Self::Fixture(o) => fixture::preference_outcome(&state.peek(), &o.model, None),
        }
    }
    pub fn failure<B: Boundary>(&self, boundary: &B, state: Signal<ShellState>, error: Error) {
        match self {
            Self::Native(_) => boundary.failure(state, error, "Could not load emoji preferences"),
            Self::Fixture(o) => fixture::preference_outcome(&state.peek(), &o.model, Some(error)),
        }
    }
}
