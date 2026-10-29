//! Existing cell-owner adapter over the shared entity directory service.
use super::*;
pub(crate) mod directory;
pub(crate) use directory::{Kind, Presentation, State};
pub(super) use directory::{LIMIT, workspace};
use directory::{Query, Results, cached};
pub(crate) type Choice = EditorChoice;
impl Kind {
    fn of(editor: EditorKind) -> Option<Self> {
        match editor {
            EditorKind::User | EditorKind::MultiUser => Some(Self::Users),
            EditorKind::Channel | EditorKind::MultiChannel => Some(Self::Channels),
            _ => None,
        }
    }
}
#[cfg(test)]
use super_platinum_core::slack::models::{Channel, User};
#[derive(Clone)]
struct Request {
    kind: Kind,
    scope: Scope,
    visit: u64,
    draft: u64,
    query: Query,
}
impl Request {
    fn current(&self, shell: &ShellState) -> bool {
        !assignment::blocks_mutation(shell)
            && self.scope.current(shell)
            && checkbox_visit_current(shell, self.visit)
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                !view.stale
                    && !view.unavailable
                    && view.editor.as_ref().is_some_and(|draft| {
                        Kind::of(draft.kind) == Some(self.kind)
                            && draft.generation == self.draft
                            && draft.visit == self.visit
                            && draft.revision == view.revision
                            && draft.directory.current(&self.query)
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
    if assignment::blocks_mutation(shell)
        || !scope.current(shell)
        || shell.overlay != Some(Overlay::SlackList)
    {
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
    let kind = Kind::of(draft.kind)?;
    let (query, choices, fixture) = directory::prepare(shell, kind, query);
    let request = Request {
        kind,
        scope,
        visit: view.generation,
        draft: generation,
        query,
    };
    let users = &mut shell.slack_lists.view.as_mut()?.editor.as_mut()?.directory;
    if !users.begin(&request.query, choices, fixture) {
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
    let update = directory::resolved(shell, &request.query, result);
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .editor
        .as_mut()
        .unwrap()
        .directory
        .finish(&request.query, update);
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
        let posting = crate::bootstrap::posting_ingress::capture(
            &state,
            &context.transport,
            &context.session,
        );
        complete_with(
            state,
            request,
            &context,
            |request| context.transport.execute(request),
            |state, channels| posting.channels(state, channels),
        )
        .await;
    });
}

fn retire_owned(mut state: Signal<ShellState>, request: &Request) {
    if !request.current(&state.peek()) {
        return;
    }
    let mut shell = state.write();
    if request.current(&shell) {
        shell
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .editor
            .as_mut()
            .unwrap()
            .directory
            .cancel(&request.query);
    }
}

async fn complete_with<F, Fut, I>(
    state: Signal<ShellState>,
    request: Request,
    context: &crate::scheduled::Context,
    execute: F,
    mut ingress: I,
) where
    F: FnMut(super_platinum_core::slack::PreparedRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Value, Error>>,
    I: FnMut(&Signal<ShellState>, &[super_platinum_core::slack::models::Channel]),
{
    let current =
        |shell: &ShellState| request.current(shell) && directory::client_current(shell, context);
    let received =
        directory::fetch_with(context, &request.query, execute, || current(&state.peek())).await;
    if !current(&state.peek()) {
        retire_owned(state, &request);
        return;
    }
    let result = match received {
        Ok(Some(rows)) => Ok(rows),
        Err(error) => Err(error),
        Ok(None) => {
            retire_owned(state, &request);
            return;
        }
    };
    if request.kind == Kind::Channels
        && current(&state.peek())
        && let Ok(Results::Channels(channels)) = &result
    {
        ingress(&state, channels);
    }
    if current(&state.peek()) {
        let mut state = state;
        finish(&mut state.write(), &request, result);
    } else {
        retire_owned(state, &request);
    }
}

#[cfg(test)]
mod channel_tests;
#[cfg(test)]
mod continuation_tests;
#[cfg(test)]
mod owner_tests;
#[cfg(test)]
mod users_tests;
