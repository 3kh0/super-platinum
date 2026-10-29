//! Workspace file search; retained results never become canonical file facts.
use super::*;
use crate::state::Overlay;
use std::{future::Future, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    search::SearchKind,
    slack::{Error, PreparedRequest, SlackClient, Transport, models::File},
};

pub(super) async fn load(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
) {
    if super::file_pages::saved::try_load(state, &request, &transport, &client, &session).await {
        return;
    }
    #[cfg(test)]
    if let Some(io) = dioxus::prelude::try_consume_context::<std::rc::Rc<super::file_type_io::Io>>()
    {
        load_with_profiles(
            state,
            request,
            &transport,
            &client,
            &session,
            None,
            move |request| async move { io.execute(request).await },
            |_| std::future::ready(Ok(Vec::new())),
        )
        .await;
        return;
    }
    load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |request| transport.execute(request),
        |ids| api::fetch_users_info(&transport, &client, &session, ids),
    )
    .await;
}

#[cfg(test)]
async fn load_with<F, Fut>(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    execute: F,
) where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    load_scoped_with(state, request, transport, client, session, None, execute).await;
}

#[cfg(test)]
pub(super) async fn load_scoped_with<F, Fut>(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    scope: Option<super::recovery::Scope>,
    execute: F,
) where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    load_with_profiles(
        state,
        request,
        transport,
        client,
        session,
        scope,
        execute,
        |_| std::future::ready(Ok(Vec::new())),
    )
    .await;
}

pub(super) async fn load_with_profiles<F, Fut, U, UFut>(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    scope: Option<super::recovery::Scope>,
    execute: F,
    lookup: U,
) where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    U: FnMut(Vec<String>) -> UFut,
    UFut: Future<Output = Result<Vec<super_platinum_core::slack::models::User>, Error>>,
{
    let owner = state
        .try_read()
        .ok()
        .and_then(|shell| super::uploaders::Read::capture(&shell, &request, transport, session));
    let completed =
        load_page_with(state, request, transport, client, session, scope, execute).await;
    if let (Some(owner), Some(cursor)) = (owner, completed) {
        super::uploaders::hydrate_with(state, owner.complete(cursor), lookup).await;
    }
}

async fn load_page_with<F, Fut>(
    mut state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    scope: Option<super::recovery::Scope>,
    execute: F,
) -> Option<super::recovery::Cursor>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let display_read = {
        let Ok(mut shell) = state.try_write() else {
            return None;
        };
        if super::file_pages::has_scope(&shell)
            || !super::messages::current_for(&shell, &request, transport, scope.as_ref())
        {
            return None;
        }
        shell.core.begin_file_display_read(&request.team)
    };
    let args = api::SearchArgs {
        sort: request.sort,
        page: request.page,
        ..api::SearchArgs::new(request.query.clone())
    };
    let result = api::fetch_search_files_with(client, session, args, execute).await;
    let Ok(mut shell) = state.try_write() else {
        return None;
    };
    if super::file_pages::has_scope(&shell)
        || !super::messages::current_for(&shell, &request, transport, scope.as_ref())
    {
        return None;
    }
    let result = result.and_then(|mut page| {
        let overlay = shell
            .core
            .finish_file_display_read(&request.team, display_read)
            .map_err(|code| Error::Api(code.into()))?;
        page.items = page
            .items
            .into_iter()
            .map(|file| overlay.file(file))
            .collect();
        Ok(page)
    });
    finish(&mut shell, &request, transport, result)
        .then(|| super::recovery::Cursor::capture(&shell))
}

fn finish(
    shell: &mut ShellState,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    result: Result<api::SearchFilesPage, Error>,
) -> bool {
    if request.kind != SearchKind::Files || !super::current(shell, request, transport) {
        return false;
    }
    match result {
        Ok(page)
            if shell
                .search_pager
                .complete(request, page.pagination.as_ref()) =>
        {
            let deleted = shell
                .core
                .workspaces
                .get(&request.team)
                .map(|w| &w.deleted_files);
            let mut seen = shell
                .search_file_results
                .iter()
                .filter_map(|f| f.id.clone())
                .collect::<std::collections::HashSet<_>>();
            shell
                .search_file_results
                .extend(page.items.into_iter().filter(|file| {
                    file.id.as_ref().is_some_and(|id| {
                        !id.is_empty()
                            && !super_platinum_core::file_actions::deleted(file)
                            && !deleted.is_some_and(|ids| ids.contains(id))
                            && seen.insert(id.clone())
                    })
                }));
            true
        }
        Ok(_) => false,
        Err(error) => {
            shell.search_pager.fail(request);
            shell.report_failure(&error, "File search needs a connection", || {
                "Could not search files. Please try again.".into()
            });
            false
        }
    }
}

#[derive(Clone)]
pub(crate) struct Source {
    account: Option<String>,
    epoch: u64,
    team: String,
    transport: Option<Arc<Transport>>,
    revision: u64,
    visit: u64,
    id: String,
}
pub(crate) fn visible(shell: &ShellState, file: &File) -> bool {
    file.id.as_ref().is_some_and(|id| {
        !id.is_empty()
            && !super_platinum_core::file_actions::deleted(file)
            && !shell
                .core
                .active_team
                .as_ref()
                .and_then(|team| shell.core.workspaces.get(team))
                .is_some_and(|w| w.deleted_files.contains(id))
    })
}
impl Source {
    pub(crate) fn capture(shell: &ShellState, id: &str) -> Option<Self> {
        if super::file_pages::has_scope(shell)
            || shell.overlay != Some(Overlay::Search)
            || shell.search_pager.kind != SearchKind::Files
            || shell.search_pager.page == 0
            || shell.search_pager.query != shell.search_query.trim()
            || !shell
                .search_file_results
                .iter()
                .any(|f| f.id.as_deref() == Some(id) && visible(shell, f))
        {
            return None;
        }
        Some(Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            team: shell.core.active_team.clone()?,
            transport: shell.core.transport.clone(),
            revision: shell.search_pager.revision(),
            visit: shell.search_open_generation,
            id: id.into(),
        })
    }

    fn resolve(&self, shell: &ShellState) -> Option<File> {
        if super::file_pages::has_scope(shell)
            || shell.core.active_account != self.account
            || shell.core.account_epoch != self.epoch
            || shell.core.active_team.as_ref() != Some(&self.team)
            || shell.search_pager.revision() != self.revision
            || shell.search_open_generation != self.visit
            || shell.search_pager.kind != SearchKind::Files
            || shell.overlay != Some(Overlay::Search)
            || !match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
        {
            return None;
        }
        shell
            .search_file_results
            .iter()
            .find(|f| f.id.as_ref() == Some(&self.id) && visible(shell, f))
            .cloned()
    }
}

pub(crate) fn open(state: Signal<ShellState>, source: Source) {
    let file = source.resolve(&state.peek());
    if let Some(file) = file {
        crate::files::open(state, file);
    }
}

pub(crate) fn fixture(shell: &mut ShellState) {
    shell.overlay = Some(Overlay::Search);
    shell.replace_search_query("release in:ship".into());
    shell.set_search_kind(SearchKind::Files);
    let request = shell
        .search_pager
        .begin(shell.core.account_epoch, "T1", &shell.search_query, false)
        .unwrap();
    shell.search_pager.complete(
        &request,
        Some(&super_platinum_core::slack::models::SearchPagination {
            page: Some(1),
            page_count: Some(2),
            total_count: Some(24),
            ..Default::default()
        }),
    );
    shell.search_file_results = (0..3).map(|i| serde_json::from_value(serde_json::json!({
        "id":format!("FSEARCH{i}"),"title":format!("Release notes {}",i+1),"name":format!("release-{i}.txt"),
        "mimetype":"text/plain","filetype":"text","size":2048,"user":"U1","created":1719800300,
        "url_private":format!("https://files.slack.com/files-pri/T1-FSEARCH{i}/release-{i}.txt")
    })).unwrap()).collect();
}

#[cfg(test)]
mod tests;
