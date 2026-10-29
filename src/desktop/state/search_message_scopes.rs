//! Renderer-owned staged Messages identity filtering; no transport/permission proof.
use super::{ChannelTab, MainView, Overlay, ShellState};
use std::sync::Arc;
use super_platinum_core::{
    search::{SearchKind, SearchSort},
    search_query::{
        SearchFilters,
        message_scopes::{Scope, ScopeChoice, ScopeEdit},
    },
};
mod owner;
use owner::Owner;
mod choices;
use choices::Bank;
mod fixture;
pub(crate) use fixture::Fixture;
fn index(kind: Scope) -> usize {
    match kind {
        Scope::Channels => 0,
        Scope::Authors => 1,
    }
}
struct Origin {
    owner: Owner,
    key: String,
}
impl Origin {
    fn new(owner: Owner) -> Arc<Self> {
        Arc::new(Self {
            owner,
            key: uuid::Uuid::new_v4().to_string(),
        })
    }
}
#[derive(Clone)]
pub(crate) struct Draft {
    origin: Arc<Origin>,
    edit: ScopeEdit,
    bank: Bank,
    find: [String; 2],
    relevant: Vec<(Scope, String)>,
}
#[derive(Clone)]
pub(crate) struct Source {
    origin: Arc<Origin>,
}
impl Source {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let d = s.search_message_scope_filters.as_ref()?;
        let source = Self {
            origin: d.origin.clone(),
        };
        source.current(s).then_some(source)
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.origin.owner.current(s)
            && !s.search_history_open
            && s.search_filters.is_some()
            && s.search_message_scope_filters
                .as_ref()
                .is_some_and(|d| Arc::ptr_eq(&d.origin, &self.origin))
    }
    pub(crate) fn key(&self) -> &str {
        &self.origin.key
    }
}
pub(crate) fn has_draft(s: &ShellState) -> bool {
    s.search_message_scope_filters.is_some()
}
pub(crate) fn typed_panel(s: &ShellState) -> bool {
    s.search_pager.kind == SearchKind::Messages
        && (has_draft(s) || s.search_message_scope_fixture.is_some())
}
pub(crate) fn wants(s: &ShellState) -> bool {
    s.search_pager.kind == SearchKind::Messages
        && (typed_panel(s) || Owner::capture(s).is_some() && Bank::any(s))
}
pub(crate) fn generic_allowed(s: &ShellState) -> bool {
    !has_draft(s) && s.search_message_scope_fixture.is_none()
}
pub(crate) fn retire(s: &mut ShellState) {
    if s.search_message_scope_filters.take().is_some() {
        s.search_filters = None;
        s.search_filter_error = None;
    }
}
#[derive(Clone)]
pub(crate) struct Toggle {
    owner: Option<Owner>,
    query: String,
    input: u64,
    dialog: u64,
    revision: u64,
    page: u32,
    kind: SearchKind,
    sort: SearchSort,
    account: Option<String>,
    epoch: u64,
    team: Option<String>,
    history: bool,
    open: bool,
    observed: Option<Arc<Origin>>,
}
impl Toggle {
    pub(crate) fn capture(s: &ShellState) -> Self {
        Self {
            owner: Owner::capture(s),
            query: s.search_query.clone(),
            input: s.search_input_revision,
            dialog: s.search_open_generation,
            revision: s.search_pager.revision(),
            page: s.search_pager.page,
            kind: s.search_pager.kind,
            sort: s.search_pager.sort,
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone(),
            history: s.search_history_open,
            open: s.search_filters.is_some(),
            observed: s
                .search_message_scope_filters
                .as_ref()
                .map(|d| d.origin.clone()),
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        s.overlay == Some(Overlay::Search)
            && s.search_query == self.query
            && s.search_input_revision == self.input
            && s.search_open_generation == self.dialog
            && s.search_pager.revision() == self.revision
            && s.search_pager.page == self.page
            && s.search_pager.kind == self.kind
            && s.search_pager.sort == self.sort
            && s.core.active_account == self.account
            && s.core.account_epoch == self.epoch
            && s.core.active_team == self.team
            && s.search_history_open == self.history
            && s.search_filters.is_some() == self.open
            && self.owner.as_ref().is_none_or(|o| o.current(s))
            && match (&self.observed, s.search_message_scope_filters.as_ref()) {
                (None, None) => true,
                (Some(o), Some(d)) => Arc::ptr_eq(o, &d.origin),
                _ => false,
            }
    }
}
pub(crate) fn toggle(s: &mut ShellState, source: Toggle) {
    if source.kind != SearchKind::Messages || !source.current(s) {
        return;
    }
    let close = source.open && !source.history;
    s.cancel_search_open();
    s.search_history_open = false;
    s.search_filter_error = None;
    if close {
        s.search_filters = None;
        return;
    }
    if let Some(mut owner) = source.owner {
        owner.rebase_dialog(s);
        if !owner.current(s) {
            return;
        }
        let Some(bank) = Bank::capture(s) else {
            return;
        };
        match ScopeEdit::capture(&s.search_query, bank.choices()) {
            Ok(edit) => {
                let relevant = bank
                    .choices()
                    .into_iter()
                    .filter(|c| edit.selected(c.kind(), c.id()))
                    .map(|c| (c.kind(), c.id().to_owned()))
                    .collect();
                s.search_filters = Some(edit.fields());
                s.search_message_scope_filters = Some(Draft {
                    origin: Origin::new(owner),
                    edit,
                    bank,
                    find: Default::default(),
                    relevant,
                });
            }
            Err(error) => s.search_filter_error = Some(error),
        }
    } else if s.search_message_scope_fixture.is_some() {
        s.search_filters = Some(SearchFilters::default());
    }
    // An originally typed handler never creates a generic fallback.
}
#[derive(Clone)]
pub(crate) struct Row {
    pub id: String,
    pub label: String,
    pub username: String,
    pub selected: bool,
    pub current: bool,
}
pub(crate) fn rows(s: &ShellState, kind: Scope) -> Vec<Row> {
    if Source::capture(s).is_some() {
        let d = s.search_message_scope_filters.as_ref().unwrap();
        return d.bank.window(s, &d.edit, kind, &d.find[index(kind)]);
    }
    fixture::rows(s, kind)
}
pub(crate) fn count(s: &ShellState, kind: Scope) -> usize {
    s.search_message_scope_filters
        .as_ref()
        .filter(|_| Source::capture(s).is_some())
        .map(|d| d.edit.selected_count(kind))
        .unwrap_or_else(|| fixture::count(s, kind))
}
pub(crate) fn find_text(s: &ShellState, kind: Scope) -> &str {
    s.search_message_scope_filters
        .as_ref()
        .filter(|_| Source::capture(s).is_some())
        .map(|d| d.find[index(kind)].as_str())
        .unwrap_or("")
}
pub(crate) fn find(s: &mut ShellState, source: &Source, kind: Scope, value: String) {
    if source.current(s) {
        s.search_message_scope_filters.as_mut().unwrap().find[index(kind)] = value;
    }
}
pub(crate) fn set(s: &mut ShellState, source: &Source, kind: Scope, id: &str, checked: bool) {
    if !source.current(s) {
        return;
    }
    let d = s.search_message_scope_filters.as_mut().unwrap();
    // Resolve through the original bank, then compare current native records.
    // Temporarily leave the borrow before current-state projection below.
    if d.bank.get(kind, id).is_none() {
        return;
    }
    if !s
        .search_message_scope_filters
        .as_ref()
        .unwrap()
        .bank
        .entry_current(s, kind, id)
    {
        s.search_filter_error = Some("Search options changed. Reopen the filters.");
        return;
    }
    let d = s.search_message_scope_filters.as_mut().unwrap();
    match d.edit.set(kind, id, checked) {
        Ok(()) => {
            if !d.relevant.iter().any(|(k, v)| *k == kind && v == id) {
                d.relevant.push((kind, id.into()));
            }
            s.search_filter_error = None;
        }
        Err(error) => s.search_filter_error = Some(error),
    }
}
pub(crate) fn clear(s: &mut ShellState, source: &Source, kind: Option<Scope>, all_fields: bool) {
    if !source.current(s) {
        return;
    }
    let d = s.search_message_scope_filters.as_mut().unwrap();
    for k in [Scope::Channels, Scope::Authors] {
        if kind.is_none_or(|value| value == k) {
            d.edit.clear(k);
            d.find[index(k)].clear();
        }
    }
    d.origin = Origin::new(d.origin.owner.clone());
    if all_fields {
        s.search_filters = Some(SearchFilters::default());
    }
    s.search_filter_error = None;
}
pub(crate) struct Submission {
    owner: Owner,
    unchanged: bool,
}
impl Submission {
    pub(crate) fn unchanged(&self) -> bool {
        self.unchanged
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.search_filters.is_none()
            && !has_draft(s)
            && s.search_file_type_filters.is_none()
            && !s.search_history_open
    }
}
pub(crate) fn apply(s: &mut ShellState, source: &Source) -> Option<Submission> {
    if !source.current(s) {
        return None;
    }
    let d = s.search_message_scope_filters.as_ref().unwrap();
    if !d.bank.relevant_current(s, &d.relevant) {
        s.search_filter_error = Some("Search options changed. Reopen the filters.");
        return None;
    }
    let result = d
        .edit
        .apply(&s.search_query, s.search_filters.as_ref().unwrap());
    match result {
        Ok(query) => {
            let unchanged = query == s.search_query;
            let mut owner = source.origin.owner.clone();
            s.replace_search_query(query);
            s.search_message_scope_filters = None;
            s.search_filters = None;
            s.search_filter_error = None;
            owner.rebase_query(s);
            owner.current(s).then_some(Submission { owner, unchanged })
        }
        Err(error) => {
            s.search_filter_error = Some(error);
            None
        }
    }
}
pub(crate) fn fixture(s: &mut ShellState, name: &str) {
    fixture::install(s, name)
}
#[cfg(test)]
pub(crate) mod tests;
