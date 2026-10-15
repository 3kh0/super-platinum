//! Request execution only; the caller owns all scope and generation checks.
use super::Context;
use serde_json::Value;
use std::sync::Arc;
use super_platinum_core::slack::{
    Error, PreparedRequest, Transport, api,
    models::{ActivityFeedPage, CountsPage, HistoryPage, ThreadsViewPage},
};

pub(super) trait Executor {
    async fn execute(&self, request: PreparedRequest) -> Result<Value, Error>;
}

pub(super) struct Network(pub Arc<Transport>);
impl Executor for Network {
    async fn execute(&self, request: PreparedRequest) -> Result<Value, Error> {
        self.0.execute(request).await
    }
}

pub(super) async fn counts<E: Executor>(e: &E, c: &Context) -> Result<CountsPage, Error> {
    api::fetch_counts_with(&c.client, &c.session, |request| e.execute(request)).await
}
pub(super) async fn threads<E: Executor>(
    e: &E,
    c: &Context,
    cursor: Option<String>,
) -> Result<ThreadsViewPage, Error> {
    api::fetch_threads_view_with(&c.client, &c.session, 30, cursor, false, |request| {
        e.execute(request)
    })
    .await
}
pub(super) async fn activity<E: Executor>(
    e: &E,
    c: &Context,
    cursor: Option<String>,
) -> Result<ActivityFeedPage, Error> {
    api::fetch_activity_feed_with(&c.client, &c.session, 20, cursor, false, |request| {
        e.execute(request)
    })
    .await
}
pub(super) async fn history<E: Executor>(
    e: &E,
    c: &Context,
    args: api::HistoryArgs,
) -> Result<HistoryPage, Error> {
    api::fetch_history_with(&c.client, &c.session, args, |request| e.execute(request)).await
}
pub(super) async fn replies<E: Executor>(
    e: &E,
    c: &Context,
    args: api::RepliesArgs,
) -> Result<HistoryPage, Error> {
    api::fetch_replies_with(&c.client, &c.session, args, |request| e.execute(request)).await
}
