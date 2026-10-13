//! Shared bounded directory lifetime for one ordinary entity-cell draft.
use super::*;
use super_platinum_core::{
    slack::{
        api,
        models::{Channel, User},
    },
    state::Workspace,
};
pub(super) const LIMIT: usize = 30;
pub(crate) type Choice = EditorChoice;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Users,
    Channels,
}
impl Kind {
    fn of(editor: EditorKind) -> Option<Self> {
        match editor {
            EditorKind::User | EditorKind::MultiUser => Some(Self::Users),
            EditorKind::Channel | EditorKind::MultiChannel => Some(Self::Channels),
            _ => None,
        }
    }
    fn plural(self) -> &'static str {
        match self {
            Self::Users => "people",
            Self::Channels => "channels",
        }
    }
}
enum Results {
    Users(Vec<User>),
    Channels(Vec<Channel>),
}
impl Results {
    fn kind(&self) -> Kind {
        match self {
            Self::Users(_) => Kind::Users,
            Self::Channels(_) => Kind::Channels,
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct State {
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
impl State {
    pub(super) fn new(shell: &ShellState, kind: Kind) -> Self {
        Self {
            candidates: Arc::new(cached(shell, kind, "")),
            ..Self::default()
        }
    }
    pub(super) fn presentation(&self) -> Presentation {
        Presentation {
            query: self.query.clone(),
            candidates: self.candidates.clone(),
            loading: self.loading,
            error: self.error.clone(),
        }
    }
}
pub(super) fn workspace(shell: &ShellState) -> Option<&Workspace> {
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
}
fn cached(shell: &ShellState, kind: Kind, query: &str) -> Vec<EditorChoice> {
    workspace(shell).map_or_else(Vec::new, |workspace| match kind {
        Kind::Users => super::users::candidates(
            workspace,
            workspace.users.values().map(|user| (user, false)),
            query,
        ),
        Kind::Channels => super::channels::candidates(
            workspace,
            workspace.channels.values().map(|channel| (channel, false)),
            query,
        ),
    })
}
#[derive(Clone)]
struct Request {
    kind: Kind,
    scope: Scope,
    visit: u64,
    draft: u64,
    revision: u64,
    query: String,
}
impl Request {
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && checkbox_visit_current(shell, self.visit)
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                !view.stale
                    && !view.unavailable
                    && view.editor.as_ref().is_some_and(|draft| {
                        Kind::of(draft.kind) == Some(self.kind)
                            && draft.generation == self.draft
                            && draft.visit == self.visit
                            && draft.revision == view.revision
                            && draft.directory.revision == self.revision
                            && draft.attempt.is_none()
                            && draft
                                .authority
                                .admits(view, &draft.row_id, &draft.column_id)
                    })
            })
    }
}
fn begin(shell: &mut ShellState, generation: u64, query: String) -> Option<Request> {
    let scope = shell.slack_lists.scope.clone()?;
    if !scope.current(shell) || shell.overlay != Some(Overlay::SlackList) {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    let draft = view.editor.as_ref()?;
    if draft.generation != generation
        || draft.attempt.is_some()
        || Kind::of(draft.kind).is_none()
        || view.stale
        || view.loading
        || view.unavailable
        || view.revision != draft.revision
        || !draft
            .authority
            .admits(view, &draft.row_id, &draft.column_id)
    {
        return None;
    }
    let query = query.chars().take(256).collect::<String>();
    let kind = Kind::of(draft.kind)?;
    let choices = cached(shell, kind, &query);
    let revision = next_id();
    let request = Request {
        kind,
        scope,
        visit: view.generation,
        draft: generation,
        revision,
        query: query.clone(),
    };
    let fixture = super::super::fixture::enabled(shell);
    let users = &mut shell.slack_lists.view.as_mut()?.editor.as_mut()?.directory;
    users.query = query;
    users.revision = revision;
    users.candidates = Arc::new(choices);
    users.error = None;
    users.loading = users.retry_at <= now() && !fixture;
    if users.retry_at > now() {
        users.error = Some(format!(
            "Slack asked us to wait before searching again. Cached {} are shown.",
            kind.plural()
        ));
        return None;
    }
    Some(request)
}
fn finish(shell: &mut ShellState, request: &Request, result: Result<Results, Error>) {
    if !request.current(shell)
        || result
            .as_ref()
            .is_ok_and(|result| result.kind() != request.kind)
    {
        return;
    }
    let choices = result.as_ref().ok().and_then(|remote| {
        workspace(shell).map(|workspace| match remote {
            Results::Users(remote) => super::users::candidates(
                workspace,
                remote
                    .iter()
                    .map(|user| (user, true))
                    .chain(workspace.users.values().map(|user| (user, false))),
                &request.query,
            ),
            Results::Channels(remote) => super::channels::candidates(
                workspace,
                remote
                    .iter()
                    .map(|channel| (channel, true))
                    .chain(workspace.channels.values().map(|channel| (channel, false))),
                &request.query,
            ),
        })
    });
    let directory = &mut shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .editor
        .as_mut()
        .unwrap()
        .directory;
    directory.loading = false;
    match result {
        Ok(_) => {
            directory.candidates = Arc::new(choices.unwrap_or_default());
            directory.error = None;
            directory.retry_at = 0;
        }
        Err(error) => {
            directory.retry_at = retry_at(&error);
            directory.error = Some(format!(
                "Could not search Slack. Cached {} are shown; change the search to try again.",
                request.kind.plural()
            ));
        }
    }
}
pub(crate) fn search(mut state: Signal<ShellState>, generation: u64, query: String) {
    let request = begin(&mut state.write(), generation, query);
    let Some(request) = request else {
        return;
    };
    if super::super::fixture::enabled(&state.peek()) {
        return;
    }
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if !request.current(&state.peek()) {
            return;
        }
        let Some(context) = request.scope.request(&state.peek()) else {
            finish(
                &mut state.write(),
                &request,
                Err(Error::TransportNotConfigured),
            );
            return;
        };
        let result = match request.kind {
            Kind::Users => api::fetch_users_search(
                &context.transport,
                &context.client,
                &context.session,
                request.query.clone(),
            )
            .await
            .map(Results::Users),
            Kind::Channels => api::fetch_channels_search(
                &context.transport,
                &context.client,
                &context.session,
                request.query.clone(),
            )
            .await
            .map(Results::Channels),
        };
        finish(&mut state.write(), &request, result);
    });
}

#[cfg(test)]
mod channel_tests;
#[cfg(test)]
mod users_tests;
