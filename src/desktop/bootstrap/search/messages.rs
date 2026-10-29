//! Shared message search response, hydration and projection.
use super::*;
use std::{future::Future, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, Transport},
};

pub(super) async fn load(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
) {
    let owned =
        super::message_pages::initial(&state.peek(), &request, &transport, &client, &session);
    #[cfg(test)]
    if let Some(io) = try_consume_context::<std::rc::Rc<super::message_scope_io::Io>>() {
        let execute = io.clone();
        load_owned_with_profiles(
            state,
            request,
            &transport,
            &client,
            &session,
            None,
            owned,
            move |request| async move { execute.execute(request).await },
            move |ids| io.lookup(ids),
        )
        .await;
        return;
    }
    load_owned_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        owned,
        |request| transport.execute(request),
        |ids| api::fetch_users_info(&transport, &client, &session, ids),
    )
    .await;
}

pub(super) fn current_for(
    shell: &ShellState,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    scope: Option<&super::recovery::Scope>,
) -> bool {
    super::current(shell, request, transport)
        && scope.is_none_or(|scope| scope.current(shell, request))
}
fn signal_current(
    state: Signal<ShellState>,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    scope: Option<&super::recovery::Scope>,
    identity: Option<&super::recovery::Identity>,
    owned: Option<&super::message_pages::Read>,
) -> bool {
    state.try_read().ok().is_some_and(|shell| {
        current_for(&shell, request, transport, scope)
            && identity.is_none_or(|identity| identity.account_current(&shell))
            && owned.is_none_or(|read| read.current(&shell, request))
    })
}
fn retire(mut state: Signal<ShellState>, request: &SearchRequest) {
    if let Ok(mut shell) = state.try_write() {
        shell.search_pager.fail(request);
    }
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
    load_owned_with_profiles(
        state, request, transport, client, session, scope, None, execute, lookup,
    )
    .await;
}

pub(super) async fn load_owned_with_profiles<F, Fut, U, UFut>(
    mut state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    scope: Option<super::recovery::Scope>,
    owned: Option<super::message_pages::Read>,
    execute: F,
    mut lookup: U,
) where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    U: FnMut(Vec<String>) -> UFut,
    UFut: Future<Output = Result<Vec<super_platinum_core::slack::models::User>, Error>>,
{
    let _owned_lease = owned
        .clone()
        .map(|read| super::message_pages::Lease::new(state, read));
    let identity = state
        .try_read()
        .ok()
        .and_then(|shell| super::recovery::Identity::capture_account(&shell))
        .filter(|identity| {
            identity.matches_session(transport, session)
                && owned
                    .as_ref()
                    .is_none_or(|read| read.matches_arguments(transport, client, session))
        });
    if identity.is_none()
        || !signal_current(
            state,
            &request,
            transport,
            scope.as_ref(),
            identity.as_ref(),
            owned.as_ref(),
        )
    {
        retire(state, &request);
        return;
    }
    let team = request.team.clone();
    let args = api::SearchArgs {
        sort: request.sort,
        page: request.page,
        ..api::SearchArgs::new(request.query.clone())
    };
    match api::fetch_search_messages_with(client, session, args, execute).await {
        Ok(page) => {
            if !signal_current(
                state,
                &request,
                transport,
                scope.as_ref(),
                identity.as_ref(),
                owned.as_ref(),
            ) {
                retire(state, &request);
                return;
            }
            if owned
                .as_ref()
                .is_some_and(|read| !read.valid_metadata(page.pagination.as_ref()))
            {
                if let Ok(mut shell) = state.try_write() {
                    if let Some(read) = &owned {
                        read.retire(&mut shell);
                    }
                    shell.show_toast("This search page is unavailable. Try again.");
                }
                return;
            }
            let requested_users = {
                let Ok(shell) = state.try_read() else {
                    return;
                };
                shell
                    .core
                    .workspaces
                    .get(&team)
                    .map(|workspace| {
                        let mut ids = std::collections::BTreeSet::new();
                        for item in &page.items {
                            if !item.messages.is_empty()
                                && let Some(peer) = item.channel.as_ref().and_then(|channel| {
                                    super::conversation::unresolved_peer(workspace, channel)
                                })
                            {
                                ids.insert(peer);
                            }
                            for message in &item.messages {
                                if let Some(id) = message.user.as_ref()
                                    && super::user_names::needs_name(workspace, id)
                                    && super_platinum_core::state::message_author_name(
                                        workspace, message,
                                    ) == *id
                                {
                                    ids.insert(id.clone());
                                }
                            }
                        }
                        ids
                    })
                    .unwrap_or_default()
            };
            let ids = requested_users.iter().cloned().collect::<Vec<_>>();
            for chunk in ids.chunks(100) {
                if !signal_current(
                    state,
                    &request,
                    transport,
                    scope.as_ref(),
                    identity.as_ref(),
                    owned.as_ref(),
                ) {
                    retire(state, &request);
                    return;
                }
                let snapshots = {
                    let Ok(shell) = state.try_read() else {
                        return;
                    };
                    if !identity
                        .as_ref()
                        .is_some_and(|identity| identity.current(&shell))
                    {
                        break;
                    }
                    let Some(workspace) = shell.core.workspaces.get(&team) else {
                        break;
                    };
                    super::user_names::capture(workspace, chunk)
                };
                if snapshots.is_empty() {
                    continue;
                }
                let result = lookup(snapshots.keys().cloned().collect()).await;
                if !signal_current(
                    state,
                    &request,
                    transport,
                    scope.as_ref(),
                    identity.as_ref(),
                    owned.as_ref(),
                ) {
                    retire(state, &request);
                    return;
                }
                // Optional metadata failure must not discard a decoded page.
                let Ok(users) = result else {
                    break;
                };
                let Ok(mut shell) = state.try_write() else {
                    return;
                };
                if owned
                    .as_ref()
                    .is_some_and(|read| !read.current(&shell, &request))
                    || !current_for(&shell, &request, transport, scope.as_ref())
                    || identity
                        .as_ref()
                        .is_some_and(|identity| !identity.account_current(&shell))
                {
                    shell.search_pager.fail(&request);
                    return;
                }
                if !identity
                    .as_ref()
                    .is_some_and(|identity| identity.current(&shell))
                {
                    break;
                }
                let Some(workspace) = shell.core.workspaces.get_mut(&team) else {
                    break;
                };
                super::user_names::apply(workspace, &snapshots, &requested_users, users);
            }
            if !signal_current(
                state,
                &request,
                transport,
                scope.as_ref(),
                identity.as_ref(),
                owned.as_ref(),
            ) {
                retire(state, &request);
                return;
            }
            let results = {
                let Ok(shell) = state.try_read() else {
                    return;
                };
                let workspace = shell.core.workspaces.get(&team);
                let media = &shell.media;
                page.items
                    .into_iter()
                    .flat_map(|item| {
                        let channel_id = item
                            .channel
                            .as_ref()
                            .map(|channel| channel.id.clone())
                            .unwrap_or_default();
                        let (channel_name, direct_conversation) = item
                            .channel
                            .as_ref()
                            .map(|channel| {
                                workspace
                                    .map(|workspace| super::conversation::label(workspace, channel))
                                    .unwrap_or_else(|| {
                                        (
                                            super_platinum_core::state::channel_name(channel),
                                            channel.is_im || channel.is_mpim,
                                        )
                                    })
                            })
                            .unwrap_or_else(|| ("conversation".into(), false));
                        item.messages.into_iter().map(move |message| {
                            let author = workspace
                                .map(|workspace| {
                                    super_platinum_core::state::message_author_name(
                                        workspace, &message,
                                    )
                                })
                                .unwrap_or_else(|| "Slack".into());
                            let text = workspace
                                .map(|workspace| {
                                    let vm =
                                        crate::message_vm::message_vm(workspace, &message, media);
                                    super::preview::text(&vm)
                                })
                                .unwrap_or_else(|| {
                                    super_platinum_core::state::message_text(&message)
                                })
                                .replace(['\u{e000}', '\u{e001}'], "");
                            crate::state::SearchResultVm {
                                direct_conversation,
                                thread_ts: message
                                    .thread_ts
                                    .clone()
                                    .filter(|root| Some(root) != message.ts.as_ref()),
                                channel_id: channel_id.clone(),
                                channel_name: channel_name.clone(),
                                ts: message.ts.clone().unwrap_or_default(),
                                author,
                                text,
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            };
            let Ok(mut shell) = state.try_write() else {
                return;
            };
            if owned
                .as_ref()
                .is_some_and(|read| !read.current(&shell, &request))
            {
                return;
            }
            // Keep the decoded optional metadata before complete applies its
            // legacy defaults. Each numbered page uses the existing local
            // filtering/deduplication, without concatenating different pages.
            if shell
                .search_pager
                .complete(&request, page.pagination.as_ref())
            {
                if let Some(read) = &owned {
                    let mut rows = Vec::new();
                    merge_results(&mut rows, results);
                    read.publish(&mut shell, rows, page.pagination.as_ref());
                } else {
                    merge_results(&mut shell.search_results, results);
                }
            }
        }
        Err(error) => {
            let Ok(mut shell) = state.try_write() else {
                return;
            };
            if owned
                .as_ref()
                .is_some_and(|read| !read.current(&shell, &request))
                || !current_for(&shell, &request, transport, scope.as_ref())
                || identity
                    .as_ref()
                    .is_some_and(|identity| !identity.account_current(&shell))
            {
                shell.search_pager.fail(&request);
                return;
            }
            if let Some(read) = &owned {
                read.retire(&mut shell);
            } else {
                shell.search_pager.fail(&request);
            }
            shell.report_failure(&error, "Search needs a connection", || {
                if scope.is_some() || owned.is_some() {
                    "Could not search. Please try again.".into()
                } else {
                    format!("Search failed: {error}")
                }
            });
        }
    }
}

#[cfg(test)]
mod tests;
