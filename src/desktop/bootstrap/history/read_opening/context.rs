//! Read source choice only. Live authority remains the exact CursorOwner.
use super::fixture;
use crate::{realtime::activity_counts::CursorOwner, state::ShellState};
use std::sync::Arc;
use super_platinum_core::{
    CoreAppState,
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport, api, models::HistoryPage},
};

pub(crate) enum Owner {
    Live(Arc<CursorOwner>),
    Fixture(fixture::Owner),
}
impl Owner {
    pub(crate) fn current(&self, core: &CoreAppState) -> bool {
        match self {
            Self::Live(owner) => owner.current(core),
            Self::Fixture(owner) => owner.current(core),
        }
    }
}
pub(super) enum Context {
    Live {
        transport: Arc<Transport>,
        client: SlackClient,
        session: WorkspaceSession,
        owner: Arc<Owner>,
    },
    Fixture {
        owner: Arc<Owner>,
        team: String,
    },
}
pub(super) enum Pending {
    Live(api::HistoryArgs),
    Fixture(fixture::Pending),
}
impl Pending {
    pub(super) fn id(&self) -> Option<usize> {
        match self {
            Self::Live(_) => None,
            Self::Fixture(p) => Some(p.id),
        }
    }
}
impl Context {
    pub(super) fn live(
        s: &ShellState,
        transport: &Arc<Transport>,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Option<Self> {
        let owner = CursorOwner::capture(&s.core, transport, session)?;
        Some(Self::Live {
            transport: transport.clone(),
            client: client.clone(),
            session: session.clone(),
            owner: Arc::new(Owner::Live(Arc::new(owner))),
        })
    }
    pub(super) fn fixture(s: &ShellState) -> Option<Self> {
        let owner = fixture::Owner::capture(s)?;
        Some(Self::Fixture {
            owner: Arc::new(Owner::Fixture(owner)),
            team: "T1".into(),
        })
    }
    pub(super) fn owner(&self) -> Arc<Owner> {
        match self {
            Self::Live { owner, .. } | Self::Fixture { owner, .. } => owner.clone(),
        }
    }
    pub(super) fn team(&self) -> &str {
        match self {
            Self::Live { session, .. } => &session.team_id,
            Self::Fixture { team, .. } => team,
        }
    }
    pub(super) fn begin(&self, opening: Option<usize>, args: api::HistoryArgs) -> Pending {
        match self {
            Self::Live { .. } => Pending::Live(args),
            Self::Fixture { .. } => {
                Pending::Fixture(fixture::request(opening.expect("fixture opening"), args))
            }
        }
    }
    pub(super) async fn fetch(&self, pending: Pending) -> Result<HistoryPage, Error> {
        match (self, pending) {
            (
                Self::Live {
                    transport,
                    client,
                    session,
                    ..
                },
                Pending::Live(args),
            ) => api::fetch_history(transport, client, session, args).await,
            (Self::Fixture { .. }, Pending::Fixture(pending)) => pending.finish().await,
            _ => Err(Error::Api("history_source_mismatch".into())),
        }
    }
}
