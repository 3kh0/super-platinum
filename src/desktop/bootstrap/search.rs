use super::common::credentials;
use crate::state::{SearchResultVm, ShellState};
use dioxus::prelude::*;
use super_platinum_core::{search::SearchRequest, slack::api};

pub async fn search(state: Signal<ShellState>) {
    search_page(state, false).await;
}

pub async fn search_more(state: Signal<ShellState>) {
    search_page(state, true).await;
}

async fn search_page(mut state: Signal<ShellState>, append: bool) {
    let query = state.read().search_query.trim().to_owned();
    if query.is_empty() {
        let mut shell = state.write();
        shell.search_pager.reset();
        shell.search_results.clear();
        return;
    }
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    let request = {
        let mut shell = state.write();
        let epoch = shell.core.account_epoch;
        let Some(request) = shell.search_pager.begin(epoch, &team, &query, append) else {
            return;
        };
        if !append {
            shell.search_results.clear();
        }
        request
    };
    let args = api::SearchArgs {
        sort: request.sort,
        page: request.page,
        ..api::SearchArgs::new(query.clone())
    };
    match api::fetch_search_messages(&transport, &client, &workspace_session, args).await {
        Ok(page) => {
            if !current(&state.read(), &request, &transport) {
                state.write().search_pager.fail(&request);
                return;
            }
            let requested_users = page
                .items
                .iter()
                .flat_map(|item| &item.messages)
                .filter_map(|message| message.user.clone())
                .collect::<std::collections::HashSet<_>>();
            let missing_users = {
                let shell = state.read();
                let workspace = shell.core.workspaces.get(&team);
                requested_users
                    .into_iter()
                    .filter(|user| {
                        workspace.is_none_or(|workspace| !workspace.users.contains_key(user))
                    })
                    .collect::<Vec<_>>()
            };
            let mut loaded_users = Vec::new();
            for chunk in missing_users.chunks(100) {
                if let Ok(users) =
                    api::fetch_users_info(&transport, &client, &workspace_session, chunk.to_vec())
                        .await
                {
                    loaded_users.extend(users);
                }
            }
            if !current(&state.read(), &request, &transport) {
                state.write().search_pager.fail(&request);
                return;
            }
            if !loaded_users.is_empty() {
                let mut shell = state.write();
                if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                    for user in loaded_users {
                        workspace.users.insert(user.id.clone(), user);
                    }
                }
            }
            let results = {
                let shell = state.read();
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
                        let channel_name = item
                            .channel
                            .as_ref()
                            .map(|channel| {
                                workspace
                                    .map(|workspace| {
                                        super_platinum_core::state::channel_display_name(
                                            workspace, channel,
                                        )
                                    })
                                    .unwrap_or_else(|| {
                                        super_platinum_core::state::channel_name(channel)
                                    })
                            })
                            .unwrap_or_else(|| "conversation".into());
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
                                    crate::view::rich::message_plain_text(&vm)
                                })
                                .unwrap_or_else(|| {
                                    super_platinum_core::state::message_text(&message)
                                })
                                .replace(['\u{e000}', '\u{e001}'], "");
                            crate::state::SearchResultVm {
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
            let mut shell = state.write();
            if shell
                .search_pager
                .complete(&request, page.pagination.as_ref())
            {
                merge_results(&mut shell.search_results, results);
            }
        }
        Err(error) => {
            let mut shell = state.write();
            if !current(&shell, &request, &transport) {
                shell.search_pager.fail(&request);
                return;
            }
            shell.search_pager.fail(&request);
            shell.report_failure(&error, "Search needs a connection", || {
                format!("Search failed: {error}")
            });
        }
    }
}

fn current(
    shell: &ShellState,
    request: &SearchRequest,
    transport: &std::sync::Arc<super_platinum_core::slack::Transport>,
) -> bool {
    shell.core.account_epoch == request.epoch
        && shell.core.active_team.as_deref() == Some(request.team.as_str())
        && shell.search_query.trim() == request.query
        && shell.search_pager.accepts(request)
        && shell
            .core
            .transport
            .as_ref()
            .is_some_and(|active| std::sync::Arc::ptr_eq(active, transport))
}

fn merge_results(results: &mut Vec<SearchResultVm>, incoming: Vec<SearchResultVm>) {
    let mut seen = results
        .iter()
        .map(|r| (r.channel_id.clone(), r.ts.clone()))
        .collect::<std::collections::HashSet<_>>();
    results.extend(incoming.into_iter().filter(|r| {
        !r.channel_id.is_empty()
            && !r.ts.is_empty()
            && seen.insert((r.channel_id.clone(), r.ts.clone()))
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hit(channel: &str, ts: &str) -> SearchResultVm {
        SearchResultVm {
            thread_ts: None,
            channel_id: channel.into(),
            channel_name: channel.into(),
            ts: ts.into(),
            author: "author".into(),
            text: "result".into(),
        }
    }
    #[test]
    fn overlapping_pages_keep_order_and_channel_identity() {
        let mut hits = vec![hit("C1", "3")];
        merge_results(
            &mut hits,
            vec![
                hit("C1", "3"),
                hit("C2", "3"),
                hit("C1", "2"),
                hit("", "1"),
                hit("C1", ""),
            ],
        );
        assert_eq!(
            hits.iter()
                .map(|r| (r.channel_id.as_str(), r.ts.as_str()))
                .collect::<Vec<_>>(),
            vec![("C1", "3"), ("C2", "3"), ("C1", "2")]
        );
    }
    #[test]
    fn workspace_switch_clears_hits_and_invalidates_pending_search() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let mut workspace = shell.core.workspaces["T1"].clone();
        workspace.team_id = "OTHER".into();
        shell.core.workspaces.insert("OTHER".into(), workspace);
        shell.workspaces.push(crate::state::WorkspaceVm {
            id: "OTHER".into(),
            name: "Other".into(),
            initials: "O".into(),
        });
        shell.set_search_query("a".into());
        let request = shell
            .search_pager
            .begin(shell.core.account_epoch, "T1", "a", false)
            .unwrap();
        shell.search_results.push(hit("C", "1"));
        assert!(shell.select_workspace(shell.workspaces.len() - 1));
        assert!(shell.search_results.is_empty());
        assert!(!shell.search_pager.loading);
        assert!(!shell.search_pager.accepts(&request));
    }
    #[test]
    fn editing_query_cancels_loading_and_clears_results() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.set_search_query("a".into());
        let request = shell.search_pager.begin(1, "T", "a", false).unwrap();
        shell.search_results.push(hit("C", "1"));
        shell.set_search_query("b".into());
        assert!(!shell.search_pager.accepts(&request));
        assert!(!shell.search_pager.loading);
        assert!(shell.search_results.is_empty());
    }
    #[test]
    fn applying_has_filter_replaces_query_and_invalidates_old_search() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.set_search_query("launch has:pin".into());
        let request = shell
            .search_pager
            .begin(1, "T1", &shell.search_query, false)
            .unwrap();
        shell.search_results.push(hit("C1", "1"));
        let mut filters =
            super_platinum_core::search_query::SearchFilters::from_query(&shell.search_query);
        filters.has_file = true;
        filters.has_link = true;
        shell.search_filters = Some(filters);
        let revision = shell.search_input_revision;
        assert!(shell.apply_search_filters());
        assert_eq!(shell.search_query, "launch has:pin has:file has:link");
        assert!(shell.search_filters.is_none());
        assert!(shell.search_results.is_empty());
        assert!(!shell.search_pager.accepts(&request));
        assert_eq!(shell.search_input_revision, revision.wrapping_add(1));
    }
    #[test]
    fn search_filter_fixtures_project_default_and_combined_flags() {
        for (name, has_link) in [("search-filters", false), ("search-filters-narrow", true)] {
            let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
            search_fixture(&mut shell, name);
            let filters = shell.search_filters.as_ref().unwrap();
            assert!(filters.has_file);
            assert_eq!(filters.has_link, has_link);
            assert_eq!(
                filters.apply(&shell.search_query).unwrap(),
                shell.search_query
            );
        }
    }
}

pub(crate) fn search_fixture(shell: &mut ShellState, name: &str) {
    use super_platinum_core::slack::models::SearchPagination;
    shell.overlay = Some(crate::state::Overlay::Search);
    shell.set_search_query("in:ship launch".into());
    if name.starts_with("search-filters") {
        shell.set_search_query(if name.ends_with("-narrow") {
            "in:ship launch has:file has:link".into()
        } else {
            "in:ship launch has:file".into()
        });
        shell.search_filters =
            Some(super_platinum_core::search_query::SearchFilters::from_query(&shell.search_query));
    }
    let request = shell
        .search_pager
        .begin(1, "T1", &shell.search_query, false)
        .unwrap();
    if name == "search-loading" {
        return;
    }
    if name == "search-error" {
        shell.search_pager.fail(&request);
        return;
    }
    shell.search_pager.complete(
        &request,
        Some(&SearchPagination {
            page: Some(1),
            page_count: Some(if name == "search-empty" { 1 } else { 3 }),
            total_count: Some(if name == "search-empty" { 0 } else { 48 }),
            ..Default::default()
        }),
    );
    if name == "search-empty" {
        return;
    }
    shell.search_results = (0..6).map(|i| SearchResultVm {
        thread_ts: (i == 0).then(|| "1719800300.000100".into()),
        channel_id: "C2".into(), channel_name: "ship".into(),
        ts: format!("171980040{i}.000200"), author: "Jules".into(),
        text: if i == 0 { "The launch checklist is ready for desktop review. Search keeps enough context to distinguish similar messages, including long descriptions that wrap across multiple lines.".into() }
        else { format!("Launch review {}: keyboard navigation, loading states, and final checks are ready.", i+1) },
    }).collect();
    if name == "search-opening" {
        shell.search_opening = Some(("C2".into(), "1719800400.000200".into()));
    }
    if name == "search-more-error" {
        let request = shell
            .search_pager
            .begin(1, "T1", &shell.search_query, true)
            .unwrap();
        shell.search_pager.fail(&request);
    }
}
