//! Transient callbacks keep admission, account observation and publication separate.
use super::*;
use std::{future::Future, rc::Rc};
use super_platinum_core::slack::{PreparedRequest, api};

#[derive(Clone)]
pub(crate) struct ReadScope {
    current: Rc<dyn Fn() -> bool>,
    admit: Rc<dyn Fn() -> bool>,
    observe: Rc<dyn Fn(&Result<Value, Error>)>,
}

#[cfg(test)]
pub(crate) mod test_support;
impl ReadScope {
    pub(crate) fn new(
        current: impl Fn() -> bool + 'static,
        admit: impl Fn() -> bool + 'static,
        observe: impl Fn(&Result<Value, Error>) + 'static,
    ) -> Self {
        Self {
            current: Rc::new(current),
            admit: Rc::new(admit),
            observe: Rc::new(observe),
        }
    }
    pub(crate) fn current(&self) -> bool {
        (self.current)()
    }
    pub(crate) fn admitted(&self) -> bool {
        self.current() && (self.admit)()
    }
    /// A bundled fixture owns one attempt; cached family consumers only fence reads.
    pub(crate) fn observe_attempt(&self, result: &Result<Value, Error>) {
        (self.observe)(result);
    }
    pub(crate) async fn transport(
        &self,
        ctx: &Context,
        request: PreparedRequest,
    ) -> Result<Option<Value>, Error> {
        ctx.transport
            .execute_admitted(
                request,
                || self.current(),
                || (self.admit)(),
                |result| (self.observe)(result),
            )
            .await
    }
    /// The fixture read has already been admitted immediately before registration.
    pub(crate) async fn finish_injected<Fut>(&self, read: Fut) -> Result<Option<Value>, Error>
    where
        Fut: Future<Output = Result<Value, Error>>,
    {
        let result = read.await;
        (self.observe)(&result);
        if !self.current() {
            return Err(Error::Api("request_superseded".into()));
        }
        result.map(Some)
    }
    /// Same callbacks as production, with no link hold in an injected executor.
    pub(crate) async fn injected<F, Fut>(
        &self,
        request: PreparedRequest,
        execute: F,
    ) -> Result<Option<Value>, Error>
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, Error>>,
    {
        if !self.admitted() {
            return Ok(None);
        }
        self.finish_injected(execute(request)).await
    }
    pub(crate) async fn lookup<B, F, Fut>(
        &self,
        mut prepare: B,
        execute: F,
    ) -> Result<Option<Vec<User>>, Error>
    where
        B: FnMut(Option<&str>) -> Result<PreparedRequest, Error>,
        F: FnMut(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Option<Value>, Error>>,
    {
        api::fetch_users_selector_rows_with(
            |marker| {
                if !self.admitted() {
                    return Ok(None);
                }
                prepare(marker).map(Some)
            },
            execute,
            || self.current(),
        )
        .await
        .map(|rows| rows.map(|rows| super::decode_user_rows(&rows)))
    }
    pub(crate) async fn lookup_channels<B, F, Fut>(
        &self,
        mut prepare: B,
        execute: F,
    ) -> Result<Option<Vec<super_platinum_core::slack::models::Channel>>, Error>
    where
        B: FnMut(Option<&str>) -> Result<PreparedRequest, Error>,
        F: FnMut(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Option<Value>, Error>>,
    {
        api::fetch_channels_selector_rows_with(
            |marker| {
                if !self.admitted() {
                    return Ok(None);
                }
                prepare(marker).map(Some)
            },
            execute,
            || self.current(),
        )
        .await
        .map(|rows| rows.map(|rows| super::decode_channel_rows(&rows)))
    }
}
