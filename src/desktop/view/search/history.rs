//! Local recent-query presentation and replay; never a Slack history endpoint.
use super::*;
use crate::state::Overlay;
use std::sync::Arc;
use super_platinum_core::{search::RecentSearch, slack::Transport};

pub(crate) fn entries(shell: &ShellState) -> &[RecentSearch] {
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
        .map(|workspace| workspace.recent_searches.entries())
        .unwrap_or_default()
}

pub(crate) fn detail(entry: &RecentSearch) -> String {
    if entry.kind == super_platinum_core::search::SearchKind::People {
        entry.kind.label().into()
    } else {
        format!("{} · {}", entry.kind.label(), entry.sort.label())
    }
}

pub(crate) fn toggle(shell: &mut ShellState) {
    shell.cancel_search_open();
    shell.search_history_open = !shell.search_history_open;
}

#[derive(Clone)]
pub(crate) struct Selection {
    account: Option<String>,
    epoch: u64,
    team: String,
    transport: Option<Arc<Transport>>,
    visit: u64,
    query: String,
    revision: u64,
    kind: SearchKind,
    sort: SearchSort,
    entry: RecentSearch,
}
impl Selection {
    pub(crate) fn capture(shell: &ShellState, entry: &RecentSearch) -> Option<Self> {
        if shell.overlay != Some(Overlay::Search)
            || !shell.search_history_open
            || !entries(shell).contains(entry)
        {
            return None;
        }
        Some(Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            team: shell.core.active_team.clone()?,
            transport: shell.core.transport.clone(),
            visit: shell.search_open_generation,
            query: shell.search_query.clone(),
            revision: shell.search_pager.revision(),
            kind: shell.search_pager.kind,
            sort: shell.search_pager.sort,
            entry: entry.clone(),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.overlay == Some(Overlay::Search)
            && shell.search_history_open
            && shell.core.active_account == self.account
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.search_open_generation == self.visit
            && shell.search_query == self.query
            && shell.search_pager.revision() == self.revision
            && shell.search_pager.kind == self.kind
            && shell.search_pager.sort == self.sort
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && entries(shell).contains(&self.entry)
    }
}

pub(crate) fn prepare(shell: &mut ShellState, selection: Selection) -> bool {
    if !selection.current(shell) {
        return false;
    }
    shell.search_history_open = false;
    shell.set_search_kind(selection.entry.kind);
    if shell.search_pager.change_sort(selection.entry.sort) {
        shell.clear_search_results();
    }
    shell.replace_search_query(selection.entry.query);
    shell.search_filters = None;
    shell.search_filter_error = None;
    true
}
fn replay(mut state: Signal<ShellState>, selection: Selection) {
    if prepare(&mut state.write(), selection) {
        spawn(crate::bootstrap::search(state));
    }
}
pub(crate) fn panel(state: Signal<ShellState>, shell: &ShellState) -> Element {
    let entries = entries(shell);
    rsx! {
        div { id:"search-history-panel", class:"results", "aria-label":"Recent searches",
            if entries.is_empty() { p { class:"results-empty", "Your recent searches will appear here." } }
            for (index,entry) in entries.iter().enumerate() {
                if let Some(selection) = Selection::capture(shell,entry) {
                    button { key:"recent-{index}", class:"search-result",
                        onclick:move |_|replay(state,selection.clone()),
                        strong { "{entry.query}" }
                        span { {detail(entry)} }
                    }
                }
            }
        }
    }
}
