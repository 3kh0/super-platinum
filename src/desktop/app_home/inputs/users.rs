//! Home member selection. Directory policy is shared; form and query custody are Home-owned.
use super::*;
use crate::message_buttons::inputs::user::directory;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use super_platinum_core::block_actions::form::InputKind;

pub(crate) mod dispatch;
pub(crate) mod multiple;
mod search;
pub(crate) use search::search;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct State {
    widgets: BTreeMap<(FormOwner, InputKey), Widget>,
    multiples: BTreeMap<(FormOwner, InputKey), multiple::Widget>,
    picker: Option<Picker>,
    fixture: Option<fixture::Harness>,
}
struct Widget {
    visit: u64,
    instance: u64,
    parent: super_platinum_core::block_actions::form::home::UserParent,
    binding: super::static_select::Binding,
    revision: u64,
    initial: Option<Value>,
    visible: Option<String>,
    retained_label: Option<(String, String)>,
}
pub(super) fn hydrate(s: &mut ShellState) {
    multiple::hydrate(s);
    let Some(form_owner) = super::owner(s) else {
        return;
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return;
    };
    let visit = h.id;
    let fields: Vec<_> = super::form(s)
        .into_iter()
        .flat_map(|f| f.fields.iter())
        .filter(|(_, f)| matches!(f.kind, InputKind::User))
        .filter_map(|(k, field)| {
            let f = super::form(s)?;
            Some((
                k.clone(),
                field.raw.get("initial_user").cloned(),
                f.user_seed(k).ok()?,
                f.user_parent(k).ok()?,
                dispatch::binding(s, k)?,
            ))
        })
        .collect();
    s.app_home.users.widgets.retain(|(owner, key), _| {
        owner.epoch == s.core.account_epoch
            && s.app_home.forms.contains_key(owner)
            && (owner != &form_owner || fields.iter().any(|(k, _, _, _, _)| k == key))
    });
    for (key, initial, visible, parent, binding) in fields {
        let index = (form_owner.clone(), key);
        let initial_user = initial.as_ref().and_then(Value::as_str).map(str::to_owned);
        if let Some(w) = s
            .app_home
            .users
            .widgets
            .get_mut(&index)
            .filter(|w| w.visit == visit)
        {
            w.parent = parent;
            w.binding = binding;
            if w.initial != initial {
                // HvtU initialUser effect is display-only, including removal.
                w.initial = initial;
                w.visible = initial_user;
                w.retained_label = None;
                w.revision = next();
            }
        } else {
            if s.app_home.users.widgets.len() >= 256
                && !s.app_home.users.widgets.contains_key(&index)
            {
                continue;
            }
            s.app_home.users.widgets.insert(
                index,
                Widget {
                    visit,
                    instance: next(),
                    parent,
                    binding,
                    revision: next(),
                    initial,
                    visible,
                    retained_label: None,
                },
            );
        }
    }
}

#[derive(Clone)]
pub(crate) struct Ticket {
    owner: Owner,
    surface: actions::Key,
    form: FormOwner,
    key: InputKey,
    input_revision: u64,
    widget_revision: u64,
    raw: Value,
    value: InputValue,
    context: super::super::context::Witness,
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
        let form = super::owner(s)?;
        let field = super::form(s)?.user_field(key).ok()?;
        let widget_revision = if matches!(field.kind, InputKind::MultiUser { .. }) {
            s.app_home
                .users
                .multiples
                .get(&(form.clone(), key.clone()))
                .filter(|w| w.visit == h.id)?
                .revision
        } else {
            s.app_home
                .users
                .widgets
                .get(&(form.clone(), key.clone()))
                .filter(|w| w.visit == h.id)?
                .revision
        };
        Some(Self {
            owner: h.owner.clone(),
            surface,
            form,
            key: key.clone(),
            input_revision: super::revision(s)?,
            widget_revision,
            raw: field.raw.clone(),
            value: field.value.clone(),
            context: super::super::context::Witness::capture(s).ok()?,
        })
    }
    pub(crate) fn generation(&self) -> u64 {
        self.surface.visit
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && self.context.current(s)
            && Self::capture(s, &self.key).is_some_and(|now| self.same(&now))
    }
    fn same(&self, other: &Self) -> bool {
        self.surface == other.surface
            && self.form == other.form
            && self.key == other.key
            && self.input_revision == other.input_revision
            && self.widget_revision == other.widget_revision
            && self.raw == other.raw
            && self.value == other.value
            && self.context.same_route(&other.context)
    }
    pub(crate) fn after_edit(&self, s: &ShellState) -> Option<Self> {
        let now = Self::capture(s, &self.key)?;
        (self.owner.current(s)
            && self.context.current(s)
            && self.surface == now.surface
            && self.form == now.form
            && self.raw == now.raw
            && self.context.same_route(&now.context))
        .then_some(now)
    }
}
struct Picker {
    ticket: Ticket,
    serial: u64,
    query_revision: u64,
    query: String,
    candidates: Vec<directory::Choice>,
    loading: bool,
    error: Option<String>,
    agent_unavailable: bool,
    cancel: Option<tokio::sync::watch::Sender<bool>>,
}
impl Picker {
    fn cancel(&mut self) {
        if let Some(sender) = self.cancel.take() {
            let _ = sender.send(true);
        }
    }
}
impl Drop for Picker {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub(crate) fn presentation(
    s: &ShellState,
    t: &Ticket,
) -> Option<crate::app_modal::users::SearchView> {
    let p = s.app_home.users.picker.as_ref()?;
    (p.ticket.same(t) && t.current(s)).then(|| crate::app_modal::users::SearchView {
        serial: p.serial,
        query_revision: p.query_revision,
        query: p.query.clone(),
        candidates: p.candidates.clone(),
        loading: p.loading,
        error: p.error.clone(),
        agent_unavailable: p.agent_unavailable,
    })
}
/// Read-only mounted choice. Interactive leases may be unavailable while an
/// overlay is open, without changing HvtU's local display or parent form.
pub(crate) fn display(s: &ShellState, key: &InputKey) -> Option<Vec<(String, String)>> {
    let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
    let owner = super::owner(s)?;
    let form = s.app_home.forms.get(&owner)?;
    if form.raw_view() != h.view.as_ref()?.raw() {
        return None;
    }
    let field = form.fields.get(key)?;
    if !matches!(field.kind, InputKind::User | InputKind::MultiUser { .. }) {
        return None;
    }
    let block = form.raw_view()["blocks"]
        .as_array()?
        .iter()
        .find(|b| b["block_id"] == key.block_id)?;
    if block["element"] != field.raw || field.raw["action_id"] != key.action_id {
        return None;
    }
    if matches!(field.kind, InputKind::MultiUser { .. }) {
        return multiple::display(s, &owner, key, h.id, &field.raw);
    }
    let widget = s.app_home.users.widgets.get(&(owner, key.clone()))?;
    if widget.visit != h.id || widget.initial != field.raw.get("initial_user").cloned() {
        return None;
    }
    let workspace = s.core.workspaces.get(&h.owner.ctx.session.team_id)?;
    Some(
        widget
            .visible
            .iter()
            .map(|id| {
                let label = workspace
                    .users
                    .get(id)
                    .map(|_| workspace.display_name(id))
                    .or_else(|| {
                        widget
                            .retained_label
                            .as_ref()
                            .filter(|(known, _)| known == id)
                            .map(|(_, label)| label.clone())
                    })
                    .unwrap_or_else(|| id.clone());
                (id.clone(), label)
            })
            .collect(),
    )
}
pub(crate) fn selected(s: &ShellState, t: &Ticket) -> Vec<String> {
    if !t.current(s) {
        return vec![];
    }
    if let Some(w) = s
        .app_home
        .users
        .multiples
        .get(&(t.form.clone(), t.key.clone()))
    {
        return w.visible.clone();
    }
    s.app_home
        .users
        .widgets
        .get(&(t.form.clone(), t.key.clone()))
        .and_then(|w| w.visible.clone())
        .into_iter()
        .collect()
}
pub(crate) fn labels(s: &ShellState, t: &Ticket, ids: &[String]) -> Vec<(String, String)> {
    let workspace = s.core.workspaces.get(&t.owner.ctx.session.team_id);
    ids.iter()
        .map(|id| {
            let label = workspace
                .filter(|w| w.users.contains_key(id))
                .map(|w| w.display_name(id))
                .or_else(|| {
                    s.app_home
                        .users
                        .picker
                        .as_ref()
                        .filter(|p| p.ticket.same(t))
                        .and_then(|p| {
                            p.candidates
                                .iter()
                                .find(|c| c.id == *id)
                                .map(|c| c.label.clone())
                        })
                })
                .or_else(|| {
                    s.app_home
                        .users
                        .widgets
                        .get(&(t.form.clone(), t.key.clone()))
                        .and_then(|w| w.retained_label.as_ref())
                        .filter(|(key, _)| key == id)
                        .map(|(_, label)| label.clone())
                })
                .or_else(|| {
                    s.app_home
                        .users
                        .multiples
                        .get(&(t.form.clone(), t.key.clone()))
                        .and_then(|w| w.retained_labels.get(id))
                        .cloned()
                })
                .unwrap_or_else(|| id.clone());
            (id.clone(), label)
        })
        .collect()
}
fn cached(s: &ShellState, t: &Ticket, query: &str) -> Vec<directory::Choice> {
    s.core
        .workspaces
        .get(&t.owner.ctx.session.team_id)
        .map_or_else(Vec::new, |w| {
            directory::candidates(w, w.users.values().map(|u| (u, false)), query)
        })
}
pub(crate) fn open(mut state: Signal<ShellState>, ticket: Ticket) {
    open_local(&mut state.write(), ticket);
}
fn open_local(s: &mut ShellState, t: Ticket) -> bool {
    if !t.current(s) || dispatch::disabled(s, &t.key) {
        return false;
    }
    if presentation(s, &t).is_some() {
        return true;
    }
    let candidates = cached(s, &t, "");
    let agent_unavailable = s
        .core
        .workspaces
        .get(&t.owner.ctx.session.team_id)
        .is_some_and(|w| {
            w.users
                .values()
                .any(|u| u.extra.get("is_agentforce_bot") == Some(&Value::Bool(true)))
        });
    s.app_home.users.picker = Some(Picker {
        ticket: t,
        serial: next(),
        query_revision: next(),
        query: String::new(),
        candidates,
        loading: false,
        error: None,
        agent_unavailable,
        cancel: None,
    });
    true
}
fn request_current(s: &ShellState, serial: u64, revision: u64) -> bool {
    s.app_home
        .users
        .picker
        .as_ref()
        .is_some_and(|p| p.serial == serial && p.query_revision == revision && p.ticket.current(s))
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    ticket: Ticket,
    serial: u64,
    revision: u64,
    id: String,
) -> bool {
    let mut event = None;
    let chosen = choose_impl(
        &mut state.write(),
        &ticket,
        serial,
        revision,
        &id,
        &mut event,
    );
    dispatch::schedule(state, event);
    chosen
}
#[cfg(test)]
fn choose_local(s: &mut ShellState, t: &Ticket, serial: u64, revision: u64, id: &str) -> bool {
    choose_impl(s, t, serial, revision, id, &mut None)
}
fn choose_impl(
    s: &mut ShellState,
    t: &Ticket,
    serial: u64,
    revision: u64,
    id: &str,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if !t.current(s) || !request_current(s, serial, revision) {
        return false;
    }
    let p = s.app_home.users.picker.as_ref().unwrap();
    if !p.ticket.same(t) || !p.candidates.iter().any(|c| c.id == id) {
        return false;
    }
    let Some(w) = s.core.workspaces.get(&t.owner.ctx.session.team_id) else {
        return false;
    };
    if w.users.get(id).is_some_and(|u| !directory::eligible(w, u)) {
        return false;
    }
    let label = p
        .candidates
        .iter()
        .find(|c| c.id == id)
        .unwrap()
        .label
        .clone();
    if matches!(t.value, InputValue::Users(_)) {
        return multiple::toggle(s, t, id, label, event);
    }
    if !edit(s, t, Some(id.into()), event) {
        return false;
    }
    s.app_home
        .users
        .widgets
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap()
        .retained_label = Some((id.into(), label));
    true
}
fn edit(
    s: &mut ShellState,
    t: &Ticket,
    value: Option<String>,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if !t.current(s) || !super::editable(s) {
        return false;
    }
    let Some(form) = s.app_home.forms.get_mut(&t.form) else {
        return false;
    };
    let Ok(action) = form.user_input(&t.key, value.clone()) else {
        return false;
    };
    let Ok(parent) = form.user_parent(&t.key) else {
        return false;
    };
    super::bump(s, &t.form);
    let w = s
        .app_home
        .users
        .widgets
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap();
    w.parent = parent;
    *event = action.map(|action| dispatch::Event {
        action,
        binding: w.binding.clone(),
        instance: w.instance,
    });
    w.visible = value.clone();
    w.retained_label = None;
    w.revision = next();
    s.app_home.users.picker = None;
    super::static_select::fixture::event(s, &json!({"selectedUser":value}), &t.key);
    true
}
pub(crate) fn remove(mut state: Signal<ShellState>, ticket: Ticket, id: String) -> bool {
    let mut event = None;
    let removed = remove_impl(&mut state.write(), &ticket, &id, &mut event);
    dispatch::schedule(state, event);
    removed
}
#[cfg(test)]
fn remove_local(s: &mut ShellState, t: &Ticket, id: &str) -> bool {
    remove_impl(s, t, id, &mut None)
}
fn remove_impl(
    s: &mut ShellState,
    t: &Ticket,
    id: &str,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if matches!(t.value, InputValue::Users(_)) {
        return multiple::remove(s, t, id, event);
    }
    t.current(s)
        && super::form(s).is_some_and(|f| f.fields[&t.key].optional)
        && selected(s, t) == vec![id.to_owned()]
        && edit(s, t, None, event)
}
pub(crate) fn collapse(mut state: Signal<ShellState>, ticket: Ticket) {
    let mut s = state.write();
    if s.app_home
        .users
        .picker
        .as_ref()
        .is_some_and(|p| p.ticket.same(&ticket))
    {
        s.app_home.users.picker = None;
    }
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    if s.app_home
        .users
        .picker
        .as_ref()
        .is_some_and(|p| !p.ticket.current(s))
    {
        s.app_home.users.picker = None;
        true
    } else {
        false
    }
}
