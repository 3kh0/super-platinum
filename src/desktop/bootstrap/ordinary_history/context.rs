//! Ordinary reads retain credential authority across navigation and socket reconnect.
use super::fixture;
use crate::{realtime::activity_counts::CountsOwner, state::ShellState};
use std::sync::Arc;
use super_platinum_core::{
    CoreAppState,
    config::WorkspaceSession,
    slack::{self, api, models::HistoryPage},
};

pub(super) enum Owner {
    Live(CountsOwner),
    Fixture(fixture::Owner),
}
impl Owner {
    pub(super) fn current(&self, core: &CoreAppState) -> bool {
        match self {
            Self::Live(o) => o.current(core),
            Self::Fixture(o) => o.current(core),
        }
    }
}
pub(in crate::bootstrap) enum Context {
    Live {
        transport: Arc<slack::Transport>,
        client: slack::SlackClient,
        session: WorkspaceSession,
        owner: Arc<Owner>,
    },
    Fixture {
        owner: Arc<Owner>,
    },
}
impl Context {
    pub(in crate::bootstrap) fn live(
        s: &ShellState,
        transport: &slack::Transport,
        client: &slack::SlackClient,
        session: &WorkspaceSession,
    ) -> Option<Self> {
        let current = s.core.transport.as_ref()?;
        if !std::ptr::eq(current.as_ref(), transport) {
            return None;
        }
        let owner = CountsOwner::capture(&s.core, current, session)?;
        Some(Self::Live {
            transport: current.clone(),
            client: client.clone(),
            session: session.clone(),
            owner: Arc::new(Owner::Live(owner)),
        })
    }
    pub(in crate::bootstrap) fn fixture(s: &ShellState) -> Option<Self> {
        Some(Self::Fixture {
            owner: Arc::new(Owner::Fixture(fixture::Owner::capture(s)?)),
        })
    }
    pub(super) fn owner(&self) -> Arc<Owner> {
        match self {
            Self::Live { owner, .. } | Self::Fixture { owner } => owner.clone(),
        }
    }
    pub(super) async fn fetch(
        &self,
        args: api::HistoryArgs,
        pending: Option<fixture::Pending>,
    ) -> Result<HistoryPage, slack::Error> {
        match self {
            Self::Live {
                transport,
                client,
                session,
                ..
            } => api::fetch_history(transport, client, session, args).await,
            Self::Fixture { .. } => {
                pending
                    .ok_or_else(|| slack::Error::Api("missing_fixture_history_request".into()))?
                    .finish()
                    .await
            }
        }
    }
}
