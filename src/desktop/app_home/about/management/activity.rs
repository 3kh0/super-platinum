//! Activity observations belong to an editor document and never settle its writes.
use super::*;
use protocol::{
    ActivityDetail, ActivityDetailRequest, ActivityFilters, ActivityPage, ActivityQuery,
};
use serde::{Deserialize, Serialize};
pub(crate) mod channel_labels;
pub(crate) mod date_range;
pub(crate) mod execution;
pub(crate) mod export;
pub(crate) mod fixture;
pub(crate) mod members;
mod message;
mod native;
mod presentation;
mod reads;
pub(crate) mod refresh;
mod routes;
mod view;
pub(super) use view::panel;
#[cfg(test)]
mod tests;

pub(crate) fn drafts_changed(s: &mut ShellState, team: &str) {
    let queries: Vec<_> = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .filter(|d| d.team == team && d.activity.active)
        .filter_map(|d| d.activity.member_receipt.clone())
        .collect();
    for query in queries {
        members::lookup::refresh_drafts(s, &query);
    }
}

#[derive(Clone, Default)]
pub(super) struct State {
    pub(super) active: bool,
    channel_labels: channel_labels::State,
    files: message::files::State,
    calendar: date_range::Calendar,
    filters: Filters,
    page: Option<ActivityPage>,
    page_revision: u64,
    execution: Option<execution::Intent>,
    previous: Vec<ActivityQuery>,
    detail: Option<ActivityDetail>,
    detail_row: Option<Value>,
    detail_revision: u64,
    detail_read: Option<u64>,
    breadcrumbs: Vec<Value>,
    notice: Option<String>,
    user_search: String,
    member_generation: u64,
    member_composing: bool,
    member_pending: Option<std::sync::Arc<members::Query>>,
    member_receipt: Option<std::sync::Arc<members::Query>>,
    member_results: Option<std::sync::Arc<std::sync::Mutex<members::lookup::ResultSet>>>,
    member_names: Option<members::names::Observation>,
    member_notice: Option<String>,
    member_selection: Option<(String, String)>,
    name_refresh_attempted: Option<refresh::Key>,
    name_refresh_read: Option<u64>,
    name_notice: Option<String>,
}
#[derive(Clone, PartialEq, Eq, Serialize)]
struct Filters {
    revision: u64,
    status: String,
    from: String,
    to: String,
    limit: String,
    user: String,
    date_origin: String,
    default_from: String,
    default_to: String,
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            revision: next(),
            status: String::new(),
            from: String::new(),
            to: String::new(),
            limit: "100".into(),
            user: String::new(),
            date_origin: String::new(),
            default_from: String::new(),
            default_to: String::new(),
        }
    }
}
#[derive(Clone, Copy)]
enum Field {
    Status,
    From,
    To,
    Limit,
    User,
}
#[derive(Clone, Copy)]
enum Page {
    First,
    Next,
    Previous,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
struct Dates {
    from: Option<i64>,
    to: Option<i64>,
}
#[derive(Clone, Serialize)]
struct Expected {
    visit: u64,
    document: u64,
    filters: Filters,
}

pub(in crate::app_home::about::management) fn navigation_fields(
    s: &ShellState,
    visit: u64,
) -> Option<Value> {
    let a = &document(s, visit)?.activity;
    if !a.active {
        return None;
    }
    let mut filters = json!(a.filters);
    filters["revision"] = json!(a.filters.revision.to_string());
    Some(
        json!({"filters":filters,"search":a.user_search,"memberGeneration":a.member_generation.to_string(),"calendar":date_range::snapshot(a)}),
    )
}

pub(super) fn permitted(s: &ShellState, visit: u64) -> bool {
    !super::permissions::confirming(s, visit)
        && !super::permissions::owns(s, visit)
        && !super::unpublish::confirming(s, visit)
        && !super::unpublish::owns(s, visit)
        && !super::publish::owns(s, visit)
        && !super::delete::confirming(s, visit)
        && !super::delete::owns(s, visit)
        && admitted(s, visit)
        && s.app_home
            .about
            .management
            .surface
            .as_ref()
            .and_then(|h| h.policy.as_ref())
            .is_some_and(|p| p.current(s) && p.can_enter())
}
pub(super) fn show(mut state: Signal<ShellState>, visit: u64) {
    let captured = {
        let s = state.peek();
        if !permitted(&s, visit)
            || s.app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .read
                .is_some()
        {
            return;
        }
        let Some(d) = document(&s, visit).filter(|d| !d.activity.active) else {
            return;
        };
        let expected = json!({"visit":visit,"document":d.id,"revision":d.revision,"title":d.title,"description":d.description,"navigation":true});
        let (doc, revision) = (d.id, d.revision);
        (doc, revision, expected)
    };
    dioxus::core::spawn_forever(async move {
        let (doc, revision, expected) = captured;
        // Even a temporary busy/notice render can overwrite an uncommitted
        // native buffer. Refuse entirely in the renderer before mutating state.
        let Some(release) = super::native::lock_snapshot(&expected).await else {
            return;
        };
        let mut s = state.write();
        let same_document = document(&s, visit)
            .is_some_and(|d| d.id == doc && d.revision == revision && !d.activity.active);
        let same = same_document
            && permitted(&s, visit)
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .read
                .is_none();
        if same_document {
            let d = document_mut(&mut s, visit).unwrap();
            if same {
                d.activity.active = true;
            }
            // Keep the successful native lock until its transition/refusal
            // render lands. Do not reopen a one-frame editable stale buffer.
            d.revision = next();
        }
        drop(s);
        release.release();
        if same {
            dioxus::core::queue_effect(move || refresh::request(state));
        }
    });
}
fn back(s: &mut ShellState, visit: u64) {
    if !visible(s, visit) {
        return;
    }
    let Some(d) = document_mut(s, visit).filter(|d| d.activity.active) else {
        return;
    };
    d.activity.active = false;
    d.activity.calendar.reset();
    d.activity.execution = None;
    d.activity.member_generation = next();
    d.activity.member_pending = None;
    d.activity.member_results = None;
    d.activity.detail_revision = next();
    d.activity.name_refresh_read = None;
    s.app_home.about.management.surface.as_mut().unwrap().read = None;
}
fn edit(s: &mut ShellState, visit: u64, field: Field, value: String) {
    if !permitted(s, visit) {
        return;
    }
    let Some(d) = document_mut(s, visit).filter(|d| d.activity.active) else {
        return;
    };
    let a = &mut d.activity;
    let slot = match field {
        Field::Status => &mut a.filters.status,
        Field::From => &mut a.filters.from,
        Field::To => &mut a.filters.to,
        Field::Limit => &mut a.filters.limit,
        Field::User => &mut a.filters.user,
    };
    if *slot == value && !matches!(field, Field::From | Field::To) {
        return;
    }
    *slot = value;
    a.filters.revision = next();
    a.member_generation = next();
    a.member_pending = None;
    a.member_results = None;
    if matches!(field, Field::User) {
        a.member_selection = None;
    }
    a.page = None;
    a.previous.clear();
    a.detail = None;
    a.detail_row = None;
    a.detail_revision = next();
    a.detail_read = None;
    a.notice = None;
    s.app_home.about.management.surface.as_mut().unwrap().read = None;
}
fn query(workflow: &str, filters: &Filters, dates: Dates) -> Result<ActivityQuery, Error> {
    if ![
        "",
        "SUCCESS",
        "PENDING",
        "ERROR",
        "CANCELED",
        "PENDING_USER_ACTION",
    ]
    .contains(&filters.status.as_str())
        || dates.from.zip(dates.to).is_some_and(|(a, b)| a > b)
        || filters.from.is_empty() != dates.from.is_none()
        || filters.to.is_empty() != dates.to.is_none()
    {
        return Err(unavailable());
    }
    ActivityQuery::new(
        workflow,
        ActivityFilters {
            status: (!filters.status.is_empty()).then(|| filters.status.clone()),
            user_id: (!filters.user.is_empty()).then(|| filters.user.clone()),
            from_timestamp: dates.from,
            to_timestamp: dates.to,
        },
    )?
    .with_limit(filters.limit.parse().map_err(|_| unavailable())?)
}
fn adopt_page(a: &mut State, page: ActivityPage, direction: Page) {
    match direction {
        Page::First => a.previous.clear(),
        Page::Next => {
            if let Some(old) = &a.page {
                if a.previous.len() == 32 {
                    a.previous.remove(0);
                }
                a.previous.push(old.query().clone());
            }
        }
        Page::Previous => {
            a.previous.pop();
        }
    }
    a.page = Some(page);
    a.page_revision = next();
    a.notice = None;
}
use reads::{detail, load};
fn close_detail(s: &mut ShellState, visit: u64, revision: u64) {
    if !visible(s, visit) {
        return;
    }
    let Some(d) = document_mut(s, visit)
        .filter(|d| d.activity.active && d.activity.detail_revision == revision)
    else {
        return;
    };
    d.activity.detail = None;
    d.activity.detail_row = None;
    d.activity.detail_revision = next();
    d.activity.name_notice = None;
    d.activity.name_refresh_read = None;
    let pending = d.activity.detail_read.take();
    let h = s.app_home.about.management.surface.as_mut().unwrap();
    if pending.is_some() && h.read == pending {
        h.read = None;
    }
}
fn child(mut state: Signal<ShellState>, visit: u64, row: Value) {
    let mut s = state.write();
    if !permitted(&s, visit)
        || s.app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .read
            .is_some()
    {
        return;
    }
    let Some(d) = document_mut(&mut s, visit).filter(|d| d.activity.active) else {
        return;
    };
    let a = &mut d.activity;
    let Some(page) = &a.page else {
        return;
    };
    let candidates = routes::rows(page, &a.breadcrumbs);
    let Some(crumbs) = routes::child(&a.breadcrumbs, &candidates, &row) else {
        return;
    };
    a.execution = None;
    a.breadcrumbs = crumbs;
    a.page = None;
    a.previous.clear();
    a.notice = None;
    drop(s);
    load(state, visit, Page::First);
}
fn depth(mut state: Signal<ShellState>, visit: u64, expected: Vec<Value>, depth: usize) {
    let mut s = state.write();
    if !permitted(&s, visit)
        || s.app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .read
            .is_some()
    {
        return;
    }
    let Some(d) = document_mut(&mut s, visit).filter(|d| d.activity.active) else {
        return;
    };
    let a = &mut d.activity;
    let Some(crumbs) = routes::depth(&a.breadcrumbs, &expected, depth) else {
        return;
    };
    a.execution = None;
    a.breadcrumbs = crumbs;
    a.page = None;
    a.previous.clear();
    a.notice = None;
    drop(s);
    load(state, visit, Page::First);
}

/// Passive source/queue evidence for the protected Activity fixture. No secrets.
pub(crate) fn file_snapshot(s: &ShellState, visit: u64) -> Value {
    message::files::snapshot(s, visit)
}
