//! Shared bounded directory data/transport. Owners admit and commit separately.
use super::*;
use super_platinum_core::{
    slack::{
        api,
        models::{Channel, User},
    },
    state::Workspace,
};

pub(crate) const LIMIT: usize = 30;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Users,
    Channels,
}
impl Kind {
    fn plural(self) -> &'static str {
        match self {
            Self::Users => "people",
            Self::Channels => "channels",
        }
    }
}
pub(crate) enum Results {
    Users(Vec<User>),
    Channels(Vec<Channel>),
}
impl Results {
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Self::Users(_) => Kind::Users,
            Self::Channels(_) => Kind::Channels,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Query {
    pub(crate) kind: Kind,
    revision: u64,
    text: String,
}
#[derive(Clone, Default)]
pub(crate) struct State {
    query: String,
    revision: u64,
    candidates: Arc<Vec<EditorChoice>>,
    loading: bool,
    error: Option<String>,
    retry_at: i64,
}
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Presentation {
    pub query: String,
    pub candidates: Arc<Vec<EditorChoice>>,
    pub loading: bool,
    pub error: Option<String>,
}
pub(crate) enum Update {
    Ready(Vec<EditorChoice>),
    Failed { retry_at: i64 },
    WrongKind,
}
impl State {
    pub(crate) fn new(shell: &ShellState, kind: Kind) -> Self {
        Self {
            candidates: Arc::new(cached(shell, kind, "")),
            ..Self::default()
        }
    }
    pub(crate) fn presentation(&self) -> Presentation {
        Presentation {
            query: self.query.clone(),
            candidates: self.candidates.clone(),
            loading: self.loading,
            error: self.error.clone(),
        }
    }
    pub(crate) fn current(&self, query: &Query) -> bool {
        self.revision == query.revision && self.query == query.text
    }
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.candidates.iter().any(|candidate| candidate.id == id)
    }
    /// Call only after owner admission. Query updates still show cached results
    /// during cooldown, exactly as the original cell adapter did.
    pub(crate) fn begin(
        &mut self,
        query: &Query,
        choices: Vec<EditorChoice>,
        fixture: bool,
    ) -> bool {
        self.query = query.text.clone();
        self.revision = query.revision;
        self.candidates = Arc::new(choices);
        self.error = None;
        self.loading = self.retry_at <= now() && !fixture;
        if self.retry_at > now() {
            self.error = Some(format!(
                "Slack asked us to wait before searching again. Cached {} are shown.",
                query.kind.plural()
            ));
            return false;
        }
        true
    }
    /// Owner retirement stops spinner and obsolete completion without changing
    /// the query, cached candidates or cooldown. It never starts another request.
    pub(crate) fn retire(&mut self) {
        self.revision = next_id();
        self.loading = false;
    }
    pub(crate) fn finish(&mut self, query: &Query, update: Update) {
        if !self.current(query) || matches!(update, Update::WrongKind) {
            return;
        }
        self.loading = false;
        match update {
            Update::Ready(choices) => {
                self.candidates = Arc::new(choices);
                self.error = None;
                self.retry_at = 0;
            }
            Update::Failed { retry_at } => {
                self.retry_at = retry_at;
                self.error = Some(format!(
                    "Could not search Slack. Cached {} are shown; change the search to try again.",
                    query.kind.plural()
                ));
            }
            Update::WrongKind => unreachable!(),
        }
    }
}
pub(crate) fn workspace(shell: &ShellState) -> Option<&Workspace> {
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
}
pub(crate) fn cached(shell: &ShellState, kind: Kind, query: &str) -> Vec<EditorChoice> {
    workspace(shell).map_or_else(Vec::new, |workspace| match kind {
        Kind::Users => super::super::users::candidates(
            workspace,
            workspace.users.values().map(|user| (user, false)),
            query,
        ),
        Kind::Channels => super::super::channels::candidates(
            workspace,
            workspace.channels.values().map(|channel| (channel, false)),
            query,
        ),
    })
}
/// A previously admitted remote candidate remains usable while uncached, but a
/// current cached tombstone/ineligible record vetoes it. This is not discovery:
/// callers must separately prove the ID came from their current owned results.
pub(crate) fn cache_allows(shell: &ShellState, kind: Kind, id: &str) -> bool {
    let Some(w) = workspace(shell) else {
        return false;
    };
    match kind {
        Kind::Users => w.users.get(id).is_none_or(super::super::users::eligible),
        Kind::Channels => w
            .channels
            .get(id)
            .is_none_or(super::super::channels::eligible),
    }
}
pub(crate) fn prepare(
    shell: &ShellState,
    kind: Kind,
    text: String,
) -> (Query, Vec<EditorChoice>, bool) {
    let text = text.chars().take(256).collect::<String>();
    let choices = cached(shell, kind, &text);
    (
        Query {
            kind,
            revision: next_id(),
            text,
        },
        choices,
        crate::slack_lists::fixture::enabled(shell),
    )
}
/// Compute outside the owning registry lock. Uses the current workspace cache
/// veto and the unchanged per-domain bounded ranking helpers.
pub(crate) fn resolved(
    shell: &ShellState,
    query: &Query,
    result: Result<Results, Error>,
) -> Update {
    let remote = match result {
        Ok(remote) if remote.kind() == query.kind => remote,
        Ok(_) => return Update::WrongKind,
        Err(error) => {
            return Update::Failed {
                retry_at: retry_at(&error),
            };
        }
    };
    let choices = workspace(shell)
        .map(|workspace| match remote {
            Results::Users(remote) => super::super::users::candidates(
                workspace,
                remote
                    .iter()
                    .map(|user| (user, true))
                    .chain(workspace.users.values().map(|user| (user, false))),
                &query.text,
            ),
            Results::Channels(remote) => super::super::channels::candidates(
                workspace,
                remote
                    .iter()
                    .map(|channel| (channel, true))
                    .chain(workspace.channels.values().map(|channel| (channel, false))),
                &query.text,
            ),
        })
        .unwrap_or_default();
    Update::Ready(choices)
}
pub(crate) async fn fetch(
    context: &crate::scheduled::Context,
    query: &Query,
) -> Result<Results, Error> {
    match query.kind {
        Kind::Users => api::fetch_users_search(
            &context.transport,
            &context.client,
            &context.session,
            query.text.clone(),
        )
        .await
        .map(Results::Users),
        Kind::Channels => api::fetch_channels_search(
            &context.transport,
            &context.client,
            &context.session,
            query.text.clone(),
        )
        .await
        .map(Results::Channels),
    }
}
