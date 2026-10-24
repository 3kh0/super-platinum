//! Navigation-only transport choice. Live authority remains the existing Context.
use super::super::{Context as Live, message_fixture as fixture};
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::{
    later::SavedKey,
    slack::{
        Error, api,
        models::{Channel, MessagesListPage},
    },
};
pub(super) enum Context {
    Live(Live),
    Fixture(fixture::Visit),
}
impl Context {
    pub(super) fn get(state: &Signal<ShellState>) -> Option<Self> {
        if fixture::enabled(&state.peek()) {
            return fixture::Visit::capture(&state.peek()).map(Self::Fixture);
        }
        Live::get(state).map(Self::Live)
    }
    pub(super) fn team(&self) -> &str {
        match self {
            Self::Live(c) => &c.session.team_id,
            Self::Fixture(c) => &c.team,
        }
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        match self {
            Self::Live(c) => c.current(s),
            Self::Fixture(c) => c.current(s),
        }
    }
    pub(super) async fn messages(
        &self,
        key: &SavedKey,
    ) -> (Option<usize>, Result<MessagesListPage, Error>) {
        match self {
            Self::Live(c) => (
                None,
                api::fetch_messages_list(
                    &c.transport,
                    &c.client,
                    &c.session,
                    vec![(key.item_id.clone(), vec![key.ts.clone()])],
                )
                .await,
            ),
            Self::Fixture(_) => fixture::messages(key).await,
        }
    }
    pub(super) fn posting(
        &self,
        state: &Signal<ShellState>,
    ) -> Option<crate::bootstrap::posting_ingress::Ingress> {
        match self {
            Self::Live(c) => Some(crate::bootstrap::posting_ingress::capture(
                state,
                &c.transport,
                &c.session,
            )),
            Self::Fixture(_) => None,
        }
    }
    pub(super) async fn channels(
        &self,
        key: &SavedKey,
    ) -> (Option<usize>, Result<Vec<Channel>, Error>) {
        match self {
            Self::Live(c) => (
                None,
                api::fetch_channels_info(
                    &c.transport,
                    &c.client,
                    &c.session,
                    vec![key.item_id.clone()],
                )
                .await,
            ),
            Self::Fixture(_) => fixture::channels(key).await,
        }
    }
    pub(super) async fn history_barrier(&self, key: &SavedKey) -> Option<usize> {
        match self {
            Self::Live(_) => None,
            Self::Fixture(_) => fixture::history_barrier(key).await,
        }
    }
    pub(super) fn accepted(&self, id: Option<usize>, current: bool) {
        if let Some(id) = id {
            fixture::accepted(id, current);
        }
    }
}
