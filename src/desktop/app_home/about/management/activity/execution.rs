//! Once-only execution intent from an admitted workflow entry. Selection is
//! backed by an actual list row; absence never initiates a detail request.
use super::*;
pub(super) mod native;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct Intent {
    id: u64,
    owner: Owner,
    visit: u64,
    document: u64,
    credentials: String,
    execution: String,
    consumed: bool,
}

/// A fresh entry retains metadata edits and filter choices, not a prior entry's
/// selected execution, page, or pending asynchronous work.
pub(crate) fn arm(s: &mut ShellState, visit: u64, execution: Option<String>) -> bool {
    if !permitted(s, visit) {
        return false;
    }
    let Some(h) = s.app_home.about.management.surface.as_ref() else {
        return false;
    };
    if h.read.is_some() {
        return false;
    }
    let owner = h.owner.clone();
    let Some(credentials) = s
        .core
        .session
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok())
    else {
        return false;
    };
    let Some(d) = document_mut(s, visit) else {
        return false;
    };
    let a = &mut d.activity;
    a.execution = execution
        .filter(|id| !id.is_empty())
        .map(|execution| Intent {
            id: next(),
            owner,
            visit,
            document: d.id,
            credentials,
            execution,
            consumed: false,
        });
    a.active = false;
    a.page = None;
    a.previous.clear();
    a.breadcrumbs.clear();
    a.detail = None;
    a.detail_row = None;
    a.detail_revision = next();
    a.detail_read = None;
    a.notice = None;
    a.member_generation = next();
    a.member_pending = None;
    a.member_results = None;
    a.name_refresh_read = None;
    a.name_refresh_attempted = None;
    a.name_notice = None;
    true
}

#[derive(Clone)]
struct Ticket {
    intent: Intent,
    revision: u64,
    title: String,
    description: String,
    pending: Option<u64>,
    filters: Filters,
    page: ActivityPage,
    page_revision: u64,
    breadcrumbs: Vec<Value>,
    row: Value,
    search: String,
    member_generation: u64,
}
impl Ticket {
    fn capture(s: &ShellState, visit: u64) -> Option<Self> {
        if !permitted(s, visit) || s.app_home.about.management.surface.as_ref()?.read.is_some() {
            return None;
        }
        let d = document(s, visit).filter(|d| d.activity.active)?;
        let a = &d.activity;
        let intent = a.execution.as_ref()?;
        if intent.consumed
            || intent.visit != visit
            || intent.document != d.id
            || !intent.owner.current(s)
            || a.detail_row.is_some()
            || a.member_composing
            || a.member_pending.is_some()
            || s.core
                .session
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok())
                .as_ref()
                != Some(&intent.credentials)
        {
            return None;
        }
        let page = a.page.as_ref().filter(|p| !p.rows().is_empty())?;
        let row = routes::rows(page, &a.breadcrumbs)
            .into_iter()
            .find(|row| row["workflow_execution_id"].as_str() == Some(intent.execution.as_str()))?
            .clone();
        Some(Self {
            intent: intent.clone(),
            revision: d.revision,
            title: d.title.clone(),
            description: d.description.clone(),
            pending: d.pending,
            filters: a.filters.clone(),
            page: page.clone(),
            page_revision: a.page_revision,
            breadcrumbs: a.breadcrumbs.clone(),
            row,
            search: a.user_search.clone(),
            member_generation: a.member_generation,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        Self::capture(s, self.intent.visit).is_some_and(|actual| {
            actual.intent.id == self.intent.id
                && actual.revision == self.revision
                && actual.title == self.title
                && actual.description == self.description
                && actual.pending == self.pending
                && actual.filters == self.filters
                && actual.page == self.page
                && actual.page_revision == self.page_revision
                && actual.breadcrumbs == self.breadcrumbs
                && actual.row == self.row
                && actual.search == self.search
                && actual.member_generation == self.member_generation
        })
    }
    fn expected(&self, retired_read: u64) -> Value {
        let mut filters = json!(self.filters);
        filters["revision"] = json!(self.filters.revision.to_string());
        json!({"visit":self.intent.visit.to_string(),"document":self.intent.document.to_string(),
            "documentRevision":self.revision.to_string(),"pageRevision":self.page_revision.to_string(),
            "execution":self.intent.execution,"retiredRead":retired_read.to_string(),"filters":filters,"search":self.search,
            "memberGeneration":self.member_generation.to_string()})
    }
    fn consume(&self, s: &mut ShellState) -> Option<reads::DetailRead> {
        if !self.current(s) {
            return None;
        }
        let prepared = reads::prepare_detail(s, self.intent.visit, &self.intent.execution)?;
        document_mut(s, self.intent.visit)?
            .activity
            .execution
            .as_mut()?
            .consumed = true;
        Some(prepared)
    }
}

pub(super) fn request(mut state: Signal<ShellState>, visit: u64, retired_read: u64) {
    let Some(ticket) = Ticket::capture(&state.peek(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some((native, initial)) = native::capture(&ticket.expected(retired_read)).await else {
            return;
        };
        let prepared = {
            if !ticket.current(&state.peek()) {
                return;
            }
            let mut s = state.write();
            let Some(prepared) = ticket.consume(&mut s) else {
                return;
            };
            prepared
        };
        let activated = native.activate(prepared.id(), prepared.revision()).await;
        drop(initial);
        if !activated {
            #[allow(deprecated)]
            prepared.release(&mut state.write_silent());
            return;
        }
        reads::run_detail(state, prepared, Some(native)).await;
    });
}
