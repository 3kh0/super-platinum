//! Host custody for shared completion acquisition and ranking.
//! Implementations retain their own UI authority; this grants no Profile token.
use super::*;
use std::future::Future;
use super_platinum_core::slack::api::custom::texty::files::OrderedResponse;

pub(crate) trait Host: Clone + Send + Sync + 'static {
    const LIST: bool;
    /// Explicit caller-owned presentation; generic List hosts remain conservative.
    fn canvas_title_locale(&self) -> Option<super::super::title_locale::CanvasTitle> {
        None
    }
    fn context(&self, shell: &ShellState) -> Option<Arc<crate::scheduled::Context>>;
    fn account(&self) -> &str;
    fn member_generation(&self) -> String;
    fn member_context(&self, shell: &ShellState) -> Result<Option<Value>, Error>;
    fn team(&self) -> &str;
    fn current(&self, shell: &ShellState) -> bool;
    fn can_dispatch(&self, shell: &ShellState) -> bool;
    fn retain_failure(&self, shell: &mut ShellState, error: &Error);
    fn cancelled(&self) -> impl Future<Output = ()>;
    fn preferences(&self, shell: &ShellState) -> Option<Arc<Preferences>>;
    fn retain_preferences(&self, shell: &mut ShellState, value: Arc<Preferences>) -> bool;
    fn fixture_enabled(&self, shell: &ShellState) -> bool;
    /// Keep the original-byte file ordering sidecar attached to its response.
    /// Hosts without this closed endpoint refuse rather than reserialize JSON.
    fn fixture_file_request(
        &self,
        _state: Signal<ShellState>,
        _request: &PreparedRequest,
    ) -> impl Future<Output = Result<OrderedResponse, Error>> {
        async { Err(Error::Api("file_completion_fixture_unavailable".into())) }
    }
    fn fixture_request(
        &self,
        state: Signal<ShellState>,
        request: &PreparedRequest,
        search: bool,
    ) -> impl Future<Output = Result<(Value, Option<Vec<String>>), Error>>;
}
impl Host for query::Lease {
    const LIST: bool = false;
    fn canvas_title_locale(&self) -> Option<super::super::title_locale::CanvasTitle> {
        Some(super::super::title_locale::CanvasTitle::english())
    }
    fn context(&self, s: &ShellState) -> Option<Arc<crate::scheduled::Context>> {
        self.context(s)
    }
    fn member_generation(&self) -> String {
        self.member_generation()
    }
    fn member_context(&self, _shell: &ShellState) -> Result<Option<Value>, Error> {
        Ok(None)
    }
    fn account(&self) -> &str {
        self.account()
    }
    fn team(&self) -> &str {
        self.team()
    }
    fn current(&self, s: &ShellState) -> bool {
        self.current(s)
    }
    fn can_dispatch(&self, s: &ShellState) -> bool {
        self.can_dispatch(s)
    }
    fn retain_failure(&self, s: &mut ShellState, e: &Error) {
        self.retain_failure(s, e)
    }
    async fn cancelled(&self) {
        self.cancelled().await
    }
    fn preferences(&self, s: &ShellState) -> Option<Arc<Preferences>> {
        s.profile_edit
            .sections
            .draft
            .as_ref()?
            .rich
            .as_ref()?
            .preferences
            .clone()
    }
    fn retain_preferences(&self, s: &mut ShellState, value: Arc<Preferences>) -> bool {
        let Some(session) = s
            .profile_edit
            .sections
            .draft
            .as_mut()
            .and_then(|d| d.rich.as_mut())
        else {
            return false;
        };
        session.preferences = Some(value);
        true
    }
    fn fixture_enabled(&self, s: &ShellState) -> bool {
        fixture::completion::enabled(s)
    }
    async fn fixture_file_request(
        &self,
        state: Signal<ShellState>,
        request: &PreparedRequest,
    ) -> Result<OrderedResponse, Error> {
        fixture::completion::file_search(state, request).await
    }
    async fn fixture_request(
        &self,
        state: Signal<ShellState>,
        request: &PreparedRequest,
        search: bool,
    ) -> Result<(Value, Option<Vec<String>>), Error> {
        if search {
            fixture::completion::search(state, request).await
        } else {
            fixture::completion::request(&state.peek(), request)
        }
    }
}

pub(super) fn ordinary<L: Host>(row: &Value) -> bool {
    if L::LIST {
        super_platinum_core::slack::api::custom::texty::matching::ordinary_list_channel(row)
    } else {
        super_platinum_core::slack::api::custom::texty::matching::ordinary_local(row)
    }
}
