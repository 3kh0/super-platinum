//! Query-owned, bounded directory acquisition. Results are not selection permits.
use super::*;
use super_platinum_core::block_actions::home::workflow::{
    Directory, DirectoryEndpoint, DirectoryPage, DirectoryResults, InputType,
};

#[derive(Clone)]
pub(super) struct Query {
    pub(super) dialog: u64,
    pub(super) generation: u64,
    pub(super) field: String,
    pub(super) custom: bool,
    pub(super) text: String,
    origin: Origin,
    review: Preflight,
    directories: Vec<Directory>,
    validation: Option<u64>,
}
impl Query {
    pub(super) fn capture(
        mut state: Signal<ShellState>,
        dialog: u64,
        field: String,
        custom: bool,
        text: String,
    ) -> Option<Self> {
        let mut s = state.write();
        if !dialog_current(&s, dialog)
            || super::busy(&s, dialog)
            || text.len() > 1024
            || text.chars().any(char::is_control)
        {
            return None;
        }
        let d = s.app_home.workflow.as_ref()?;
        if d.busy || cooling(&s, &d.origin.owner.ctx) {
            return None;
        }
        let input = (if custom {
            d.review.custom()
        } else {
            d.review.context()
        })
        .iter()
        .find(|f| f.name == field)?;
        let directories = match input.kind {
            InputType::User => vec![Directory::Users],
            InputType::Conversation => vec![Directory::Channels],
            _ => return None,
        };
        let query = Self {
            dialog,
            generation: next(),
            field,
            custom,
            text,
            origin: d.origin.clone(),
            review: d.review.clone(),
            directories,
            validation: None,
        };
        s.app_home.workflow.as_mut()?.entity_query = query.generation;
        Some(query)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && self.admitted(s)
            && s.app_home.workflow.as_ref().is_some_and(|d| {
                d.id == self.dialog
                    && d.entity_query == self.generation
                    && !d.busy
                    && d.review == self.review
            })
    }
    fn admitted(&self, s: &ShellState) -> bool {
        match self.validation {
            None => !super::busy(s, self.dialog),
            Some(token) => s.app_home.workflow.as_ref().is_some_and(|d| {
                d.id == self.dialog
                    && d.entity_validation == Some(token)
                    && !d.busy
                    && d.salesforce_view.is_none()
                    && !salesforce_dispatch::active(s, self.dialog)
            }),
        }
    }
    /// A Start recheck uses the same actual directory shapes and exact retained
    /// query. Its separate marker freezes edits without impersonating a picker.
    pub(super) fn validation(
        mut state: Signal<ShellState>,
        dialog: u64,
        token: u64,
        review: &Preflight,
        field: String,
        custom: bool,
        text: String,
    ) -> Option<Self> {
        let mut s = state.write();
        if !dialog_current(&s, dialog) {
            return None;
        }
        let d = s.app_home.workflow.as_ref()?;
        if d.entity_validation != Some(token)
            || d.review != *review
            || d.busy
            || d.salesforce_view.is_some()
            || salesforce_dispatch::active(&s, dialog)
            || cooling(&s, &d.origin.owner.ctx)
        {
            return None;
        }
        let input = (if custom {
            review.custom()
        } else {
            review.context()
        })
        .iter()
        .find(|f| f.name == field)?;
        let directories = match input.kind {
            InputType::User => vec![Directory::Users],
            InputType::Conversation => vec![Directory::Channels],
            _ => return None,
        };
        let query = Self {
            dialog,
            generation: next(),
            field,
            custom,
            text,
            origin: d.origin.clone(),
            review: review.clone(),
            directories,
            validation: Some(token),
        };
        s.app_home.workflow.as_mut()?.entity_query = query.generation;
        Some(query)
    }
    pub(super) fn owner(&self) -> &Origin {
        &self.origin
    }
    pub(super) fn is_validation(&self) -> bool {
        self.validation.is_some()
    }
    pub(super) fn members(&self) -> bool {
        self.directories == [Directory::Users]
    }
    pub(super) async fn read(
        &self,
        mut state: Signal<ShellState>,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, Error> {
        {
            let s = state.peek();
            if !self.current(&s) || cooling(&s, &self.origin.owner.ctx) {
                return Err(retired());
            }
        }
        let result = io::read(state, &self.origin, request).await;
        let mut s = state.write();
        if let Err(error) = &result {
            failure(&mut s, &self.origin.owner.ctx, error);
        }
        if !self.current(&s) {
            return Err(retired());
        }
        result
    }
    /// hXqI debounces queries by 300ms. Every marker request rechecks the same
    /// owner/query and cooldown; cancellation never becomes a partial success.
    pub(super) async fn load(
        &self,
        mut state: Signal<ShellState>,
        scope: &entity_external::Scope,
    ) -> Result<Vec<DirectoryResults>, Error> {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if !self.current(&state.peek()) || !scope.current(&state.peek()) {
            return Err(retired());
        }
        // 8Bbq's native Slack-client branch (sSvv) uses local acquisition for
        // an empty query when allowEmptyQueryRemoteSearch is absent. hXqI does
        // not opt in. A whitespace query is still nonempty at this boundary.
        if self.text.is_empty() {
            return Ok(Vec::new());
        }
        let mut all = Vec::new();
        let mut endpoint: Option<DirectoryEndpoint> = None;
        for directory in &self.directories {
            let mut results = DirectoryResults::new(*directory);
            loop {
                if !endpoint.as_ref().is_some_and(|e| {
                    e.current(&self.origin.owner.ctx.session, std::time::Instant::now())
                }) {
                    let request = {
                        let s = state.peek();
                        if !self.current(&s)
                            || !scope.current(&s)
                            || cooling(&s, &self.origin.owner.ctx)
                        {
                            return Err(retired());
                        }
                        DirectoryEndpoint::discovery(
                            &self.origin.owner.ctx.client,
                            &self.origin.owner.ctx.session,
                        )?
                    };
                    // Include transit time in the TTL, conservatively.
                    let observed = std::time::Instant::now();
                    let response = io::read(state, &self.origin, request).await;
                    {
                        let mut s = state.write();
                        if let Err(error) = &response {
                            failure(&mut s, &self.origin.owner.ctx, error);
                        }
                        if !self.current(&s) || !scope.current(&s) {
                            return Err(retired());
                        }
                    }
                    endpoint = Some(DirectoryEndpoint::decode(
                        &self.origin.owner.ctx.session,
                        &response?,
                        observed,
                    )?);
                }
                let request = {
                    let s = state.peek();
                    if !self.current(&s)
                        || !scope.current(&s)
                        || cooling(&s, &self.origin.owner.ctx)
                    {
                        return Err(retired());
                    }
                    self.review.directory_page_request(
                        &self.origin.owner.ctx.client,
                        &self.origin.owner.ctx.session,
                        s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap(),
                        endpoint.as_ref().ok_or_else(retired)?,
                        std::time::Instant::now(),
                        &self.field,
                        self.custom,
                        *directory,
                        &self.text,
                        results.request_marker(),
                        scope.only_team(),
                    )?
                };
                let response = io::read(state, &self.origin, request).await;
                {
                    let mut s = state.write();
                    // Account health/cooldown is retained even if a newer query
                    // replaced this one; a foreign account cannot receive it.
                    if let Err(error) = &response {
                        failure(&mut s, &self.origin.owner.ctx, error);
                    }
                    if !self.current(&s) || !scope.current(&s) {
                        return Err(retired());
                    }
                }
                results.append(DirectoryPage::decode(response?)?)?;
                if results.complete() {
                    break;
                }
            }
            all.push(results);
        }
        if !self.current(&state.peek()) || !scope.current(&state.peek()) {
            return Err(retired());
        }
        Ok(all)
    }
}
fn retired() -> Error {
    Error::Api("home_workflow_query_retired".into())
}
