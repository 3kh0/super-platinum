//! Home options retain exact descriptors and independent query/action authority.
use super::*;
use crate::message_buttons::external::{self as shared, Owner as QueryOwner, Query, Row};
use serde_json::Value;
use super_platinum_core::block_actions::{external::ExternalOption, form::InputKind};
pub(crate) mod dispatch;
pub(crate) mod fixture;
mod multiple;
pub(crate) use multiple::remove;
pub(crate) mod multi_dispatch_fixture;
pub(crate) mod multi_fixture;
mod view;
pub(crate) use view::HomeExternal;

#[derive(Default)]
pub(crate) struct State {
    picker: Option<Picker>,
    catalogs: std::collections::BTreeMap<(FormOwner, InputKey), Catalog>,
    fixture: Option<fixture::Harness>,
}
struct Picker {
    ticket: Ticket,
    query: Query,
}
struct Catalog {
    context: Arc<Context>,
    rows: Vec<ExternalOption>,
    selected: Option<ExternalOption>,
    single: Option<dispatch::Widget>,
    multi: Option<multiple::Widget>,
}
#[derive(Clone)]
pub(crate) struct Ticket {
    pub(crate) context: Option<Arc<Context>>,
    pub(crate) route: api::SuggestionsContext,
    home: super::super::Owner,
    form: FormOwner,
    surface: actions::Key,
    inputs_revision: u64,
    key: InputKey,
    raw: Value,
    value: InputValue,
    visit: u64,
    configuration: super::super::context::Witness,
    widget_revision: u64,
}

impl Ticket {
    pub(crate) fn capture(s: &ShellState, key: &InputKey) -> Option<Self> {
        let h = s.app_home.surface.as_ref()?;
        let surface = actions::Key {
            visit: h.id,
            revision: h.revision,
        };
        if !super::current(s, surface) || !super::supported(s) {
            return None;
        }
        let configuration = super::super::context::Witness::capture(s).ok()?;
        let field = super::form(s)?.external_field(key).ok()?;
        if matches!(field.kind, InputKind::ExternalSelect { .. })
            && dispatch::display(s, key).is_none()
        {
            return None;
        }
        if matches!(field.kind, InputKind::MultiExternalSelect { .. })
            && multiple::visible(s, key).is_none()
        {
            return None;
        }
        let route = api::SuggestionsContext::home(
            &h.owner.ctx.session.team_id,
            &h.owner.ctx.session.user_id,
            configuration.team_id(),
            h.view.as_ref()?,
            key,
        )
        .ok()?;
        Some(Self {
            context: Some(h.owner.ctx.clone()),
            route,
            home: h.owner.clone(),
            form: super::owner(s)?,
            surface,
            inputs_revision: super::revision(s)?,
            key: key.clone(),
            raw: field.raw.clone(),
            value: field.value.clone(),
            visit: 0,
            configuration,
            widget_revision: dispatch::revision(s, key)
                .unwrap_or_else(|| multiple::revision(s, key)),
        })
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.home.current(s)
            && self.configuration.current(s)
            && Self::capture(s, &self.key).is_some_and(|now| {
                now.form == self.form
                    && now.surface == self.surface
                    && now.inputs_revision == self.inputs_revision
                    && now.raw == self.raw
                    && now.value == self.value
                    && now.route == self.route
                    && now.widget_revision == self.widget_revision
                    && now.configuration.same_route(&self.configuration)
            })
    }
    pub(crate) fn same_authority(&self, other: &Self) -> bool {
        self.visit == other.visit
            && self.surface == other.surface
            && self.form == other.form
            && self.inputs_revision == other.inputs_revision
            && self.key == other.key
            && self.raw == other.raw
            && self.value == other.value
            && self.route == other.route
            && self.widget_revision == other.widget_revision
            && self.configuration.same_route(&other.configuration)
    }
    pub(crate) fn id(&self) -> u64 {
        self.visit
    }
    pub(crate) fn action_disabled(&self, s: &ShellState) -> bool {
        dispatch::disabled(s, &self.key)
    }
    pub(crate) fn selected(&self, s: &ShellState) -> Option<Vec<ExternalOption>> {
        self.current(s)
            .then(|| multiple::visible(s, &self.key))
            .flatten()
    }
    pub(crate) fn query<'a>(&self, s: &'a ShellState) -> Option<&'a Query> {
        s.app_home
            .external
            .picker
            .as_ref()
            .filter(|p| p.ticket.same_authority(self))
            .map(|p| &p.query)
    }
    pub(crate) fn query_mut<'a>(&self, s: &'a mut ShellState) -> Option<&'a mut Query> {
        s.app_home
            .external
            .picker
            .as_mut()
            .filter(|p| p.ticket.same_authority(self))
            .map(|p| &mut p.query)
    }
    pub(crate) fn remember(&self, s: &mut ShellState, rows: &[Row]) {
        let catalog = s
            .app_home
            .external
            .catalogs
            .entry((self.form.clone(), self.key.clone()))
            .or_insert_with(|| Catalog {
                context: self.home.ctx.clone(),
                rows: Vec::new(),
                selected: None,
                single: None,
                multi: None,
            });
        // A response replaces current row authority; it does not rewrite the
        // independently retained selection, even when the same value is renamed.
        catalog.rows = rows.iter().map(|row| row.option.clone()).collect();
    }
}
pub(crate) fn owner(s: &ShellState, key: &InputKey) -> Option<QueryOwner> {
    let ticket = &s.app_home.external.picker.as_ref()?.ticket;
    (ticket.key == *key && ticket.current(s)).then(|| QueryOwner::Home(ticket.clone()))
}
pub(crate) fn open(mut state: Signal<ShellState>, expected: Ticket) {
    let owner = open_local(&mut state.write(), expected);
    if let Some(owner) = owner {
        shared::search(state, owner, String::new(), true, false);
    }
}
fn open_local(s: &mut ShellState, expected: Ticket) -> Option<QueryOwner> {
    if !expected.current(s) || expected.action_disabled(s) {
        return None;
    }
    let mut ticket = Ticket::capture(s, &expected.key)?;
    let field = super::form(s)?.external_field(&ticket.key).ok()?;
    let min_query_length = match field.kind {
        InputKind::ExternalSelect { min_query_length }
        | InputKind::MultiExternalSelect {
            min_query_length, ..
        } => min_query_length,
        _ => return None,
    };
    let dirty = field.dirty;
    let selected = match &field.value {
        InputValue::External(v) => v.clone(),
        InputValue::ExternalOptions(_) => None,
        _ => return None,
    };
    let index = (ticket.form.clone(), ticket.key.clone());
    let stale = s.app_home.external.catalogs.get(&index).is_some_and(|c| {
        !crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
    });
    if stale {
        s.app_home.external.catalogs.remove(&index);
    }
    // Catalogs follow the same bounded retained form set; never evict dirty provenance.
    s.app_home
        .external
        .catalogs
        .retain(|(o, _), _| s.app_home.forms.contains_key(o));
    let catalog = s
        .app_home
        .external
        .catalogs
        .entry(index)
        .or_insert_with(|| Catalog {
            context: ticket.home.ctx.clone(),
            rows: Vec::new(),
            selected: None,
            single: None,
            multi: None,
        });
    if !dirty {
        if let Some(option) = selected {
            catalog.selected = Some(option);
        }
    }
    ticket.visit = next();
    s.app_home.external.picker = Some(Picker {
        ticket: ticket.clone(),
        query: Query::new(min_query_length),
    });
    Some(QueryOwner::Home(ticket))
}
pub(crate) fn close(mut state: Signal<ShellState>, visit: u64) {
    close_local(&mut state.write(), visit);
}
fn close_local(s: &mut ShellState, visit: u64) {
    if s.app_home
        .external
        .picker
        .as_ref()
        .is_some_and(|p| p.ticket.visit == visit)
    {
        s.app_home.external.picker = None;
    }
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    owner: QueryOwner,
    revision: u64,
    index: usize,
) {
    let mut event = None;
    choose_impl(&mut state.write(), owner, revision, index, &mut event);
    dispatch::schedule(state, event);
}
#[cfg(test)]
fn choose_local(s: &mut ShellState, owner: QueryOwner, revision: u64, index: usize) -> bool {
    choose_impl(s, owner, revision, index, &mut None)
}
fn choose_impl(
    s: &mut ShellState,
    owner: QueryOwner,
    revision: u64,
    index: usize,
    event: &mut Option<dispatch::Event>,
) -> bool {
    let QueryOwner::Home(ticket) = owner else {
        return false;
    };
    if !ticket.current(s) {
        return false;
    }
    let Some(query) = ticket.query(s).filter(|q| q.revision == revision) else {
        return false;
    };
    let Some(option) = query.rows.get(index).map(|r| r.option.clone()) else {
        return false;
    };
    if !s
        .app_home
        .external
        .catalogs
        .get(&(ticket.form.clone(), ticket.key.clone()))
        .is_some_and(|c| {
            crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
                && c.rows.iter().any(|row| row == &option)
        })
    {
        ticket.query_mut(s).unwrap().error = Some("The app returned conflicting options. Choose a different value or ask the app owner to update this form.".into());
        return false;
    }
    if matches!(ticket.value, InputValue::ExternalOptions(_)) {
        return multiple::toggle(s, ticket, option, event);
    }
    dispatch::edit(s, &ticket, Some(option), event)
}
pub(crate) fn clear(mut state: Signal<ShellState>, ticket: Ticket) {
    let mut event = None;
    clear_impl(&mut state.write(), ticket, &mut event);
    dispatch::schedule(state, event);
}
#[cfg(test)]
fn clear_local(s: &mut ShellState, ticket: Ticket) -> bool {
    clear_impl(s, ticket, &mut None)
}
fn clear_impl(s: &mut ShellState, ticket: Ticket, event: &mut Option<dispatch::Event>) -> bool {
    if !ticket.current(s) || !super::form(s).unwrap().fields[&ticket.key].optional {
        return false;
    }
    dispatch::edit(s, &ticket, None, event)
}
pub(crate) fn hydrate(s: &mut ShellState) {
    hydrate_initialized(s, &[]);
}
pub(super) fn hydrate_initialized(s: &mut ShellState, initialized: &[InputKey]) {
    dispatch::hydrate(s, initialized);
    multiple::hydrate_initialized(s, initialized);
}
pub(crate) fn validated(s: &ShellState) -> bool {
    let Some(form) = super::form(s) else {
        return false;
    };
    let Some(owner) = super::owner(s) else {
        return false;
    };
    form.fields.iter().all(|(key, field)| {
        if matches!(field.value, InputValue::ExternalOptions(_)) {
            return multiple::validated(s, &owner, key, field);
        }
        let InputValue::External(Some(option)) = &field.value else {
            return true;
        };
        if !field.dirty {
            return true;
        }
        s.app_home
            .external
            .catalogs
            .get(&(owner.clone(), key.clone()))
            .is_some_and(|c| {
                crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
                    && c.selected.as_ref() == Some(option)
            })
    })
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    if s.app_home
        .external
        .picker
        .as_ref()
        .is_some_and(|p| !p.ticket.current(s))
    {
        s.app_home.external.picker = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests;
