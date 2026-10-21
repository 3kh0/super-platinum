//! List authority implementation for the channel-only shared pipeline.
use super::*;
use crate::profile_edit::sections::rich::search::channel_host::Host;
use super_platinum_core::slack::PreparedRequest;
impl Host for Lease {
    const LIST: bool = true;
    fn context(&self, shell: &ShellState) -> Option<Arc<crate::scheduled::Context>> {
        self.current(shell)
            .then(|| crate::scheduled::Context::capture(shell).map(Arc::new))
            .flatten()
    }
    fn account(&self) -> &str {
        &self.account
    }
    fn team(&self) -> &str {
        &self.team
    }
    fn member_generation(&self) -> String {
        format!(
            "list-comment-member-{}-{}-{}",
            self.binding.generation(),
            self.mount,
            self.sequence
        )
    }
    fn member_context(&self, shell: &ShellState) -> Result<Option<Value>, Error> {
        let q = shell
            .slack_lists
            .comments
            .completion
            .query
            .as_ref()
            .filter(|q| Arc::ptr_eq(&q.lease.identity, &self.identity))
            .ok_or_else(unavailable)?;
        let context = q
            .member_context
            .as_ref()
            .filter(|c| c.current(shell))
            .ok_or_else(unavailable)?;
        Ok(Some(context.value().clone()))
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.current(shell)
    }
    fn can_dispatch(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell
                .slack_lists
                .comments
                .completion
                .retry
                .as_ref()
                .filter(|(account, team, _)| account == &self.account && team == &self.team)
                .is_none_or(|(_, _, deadline)| *deadline <= now())
    }
    fn retain_failure(&self, shell: &mut ShellState, error: &Error) {
        if !self.current(shell) {
            return;
        }
        let retry = match error {
            Error::RateLimited { retry_after_secs }
            | Error::HttpStatus {
                status: 429,
                retry_after_secs,
                ..
            } => Some(retry_after_secs.unwrap_or(60)),
            _ => None,
        };
        if let Some(seconds) = retry {
            let deadline = now().saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX));
            let previous = shell
                .slack_lists
                .comments
                .completion
                .retry
                .as_ref()
                .filter(|(account, team, _)| account == &self.account && team == &self.team)
                .map_or(0, |(_, _, d)| *d);
            shell.slack_lists.comments.completion.retry = Some((
                self.account.clone(),
                self.team.clone(),
                deadline.max(previous),
            ));
        }
    }
    async fn cancelled(&self) {
        self.cancelled().await
    }
    fn preferences(&self, shell: &ShellState) -> Option<Arc<Preferences>> {
        shell
            .slack_lists
            .comments
            .completion
            .preferences
            .clone()
            .filter(|p| p.current(shell))
    }
    fn retain_preferences(&self, shell: &mut ShellState, value: Arc<Preferences>) -> bool {
        if !self.current(shell) || !value.current(shell) {
            return false;
        }
        shell.slack_lists.comments.completion.preferences = Some(value);
        true
    }
    fn fixture_enabled(&self, shell: &ShellState) -> bool {
        fixture::completion::enabled(shell)
    }
    async fn fixture_request(
        &self,
        state: Signal<ShellState>,
        request: &PreparedRequest,
        _search: bool,
    ) -> Result<(Value, Option<Vec<String>>), Error> {
        fixture::completion::request(state, request).await
    }
    async fn fixture_file_request(
        &self,
        state: Signal<ShellState>,
        request: &PreparedRequest,
    ) -> Result<super_platinum_core::slack::api::custom::texty::files::OrderedResponse, Error> {
        fixture::completion::files::request(state, request).await
    }
}
