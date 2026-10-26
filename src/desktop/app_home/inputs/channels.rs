//! Home public channel selection with independent context and source custody.
use super::*;
use crate::message_buttons::inputs::user::{conversations, directory};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use super_platinum_core::block_actions::form::{InputKind, home::ChannelTeamPolicy};
pub(crate) mod conversation;
pub(crate) mod dispatch;
mod mode;
pub(crate) mod multiple;
mod policy;
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
    dispatch: Option<dispatch::Custody>,
    conversation_dispatch: Option<conversation::dispatch::Custody>,
    revision: u64,
    initial: Option<Value>,
    conversation: bool,
    visible: Option<String>,
    retained_choice: Option<directory::Choice>,
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
        .filter(|(_, f)| matches!(f.kind, InputKind::Channel | InputKind::Conversation))
        .filter_map(|(k, f)| {
            let single = matches!(f.kind, InputKind::Channel);
            let custody = if single {
                Some((
                    super::form(s)?.channel_parent(k).ok()?,
                    dispatch::binding(s, k)?,
                ))
            } else {
                None
            };
            let conversation_custody = if !single {
                Some((
                    super::form(s)?.conversation_parent(k).ok()?,
                    conversation::dispatch::binding(s, k)?,
                ))
            } else {
                None
            };
            let value = if single {
                InputValue::Channel(super::form(s)?.channel_seed(k).ok()?)
            } else {
                InputValue::Conversation(super::form(s)?.conversation_seed(k).ok()?)
            };
            Some((
                k.clone(),
                f.raw.get(conversation::initial_key(&f.raw)).cloned(),
                matches!(f.kind, InputKind::Conversation),
                value,
                custody,
                conversation_custody,
            ))
        })
        .collect();
    s.app_home.channels.widgets.retain(|(owner, key), _| {
        owner.epoch == s.core.account_epoch
            && s.app_home.forms.contains_key(owner)
            && (owner != &form_owner || fields.iter().any(|(k, _, _, _, _, _)| k == key))
    });
    for (key, initial, is_conversation, value, custody, conversation_custody) in fields {
        let index = (form_owner.clone(), key);
        if let Some(w) = s
            .app_home
            .channels
            .widgets
            .get_mut(&index)
            .filter(|w| w.visit == visit && w.conversation == is_conversation)
        {
            if let (Some(current), Some((parent, binding))) = (&mut w.dispatch, custody) {
                current.parent = parent;
                current.binding = binding;
            }
            if let (Some(current), Some((parent, binding))) =
                (&mut w.conversation_dispatch, conversation_custody)
            {
                current.parent = parent;
                current.binding = binding;
            }
            if w.initial != initial {
                w.visible = initial.as_ref().and_then(Value::as_str).map(str::to_owned);
                w.initial = initial;
                w.revision = next();
            }
        } else {
            if s.app_home.channels.widgets.len() >= 256
                && !s.app_home.channels.widgets.contains_key(&index)
            {
                continue;
            }
            let (InputValue::Channel(visible) | InputValue::Conversation(visible)) = value else {
                continue;
            };
            // This is display evidence only, never a new query/choice ticket.
            let retained_choice = s
                .app_home
                .channels
                .widgets
                .get(&index)
                .filter(|w| w.conversation == is_conversation)
                .and_then(|w| w.retained_choice.as_ref())
                .filter(|c| visible.as_ref() == Some(&c.id))
                .cloned();
            s.app_home.channels.widgets.insert(
                index,
                Widget {
                    visit,
                    dispatch: custody.map(|(parent, binding)| dispatch::Custody {
                        instance: next(),
                        parent,
                        binding,
                    }),
                    conversation_dispatch: conversation_custody.map(|(parent, binding)| {
                        conversation::dispatch::Custody {
                            instance: next(),
                            parent,
                            binding,
                        }
                    }),
                    revision: next(),
                    initial,
                    conversation: is_conversation,
                    visible,
                    retained_choice,
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
    mode: Option<mode::Observation>,
    conversation: Option<conversations::Filter>,
    policy: ChannelTeamPolicy,
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
        let field = super::form(s)?.fields.get(key)?;
        let is_conversation = matches!(
            field.kind,
            InputKind::Conversation | InputKind::MultiConversation { .. }
        );
        if is_conversation {
            super::form(s)?.conversation_field(key).ok()?;
        } else {
            super::form(s)?.channel_field(key).ok()?;
        }
        let index = (form.clone(), key.clone());
        let multi = matches!(
            field.kind,
            InputKind::MultiChannel { .. } | InputKind::MultiConversation { .. }
        );
        let widget_revision = if multi {
            s.app_home
                .channels
                .multiples
                .get(&index)
                .filter(|w| w.visit == h.id)?
                .revision
        } else {
            s.app_home
                .channels
                .widgets
                .get(&index)
                .filter(|w| w.visit == h.id)?
                .revision
        };
        let context = super::super::context::Witness::capture(s).ok()?;
        // z+fW has constant public/nonarchived options; tpNE is single-only.
        let filter = if is_conversation {
            Some(conversations::workspace_filter(
                s,
                &h.owner.ctx.session.team_id,
                &field.raw,
            )?)
        } else {
            None
        };
        let mode = if multi || is_conversation {
            None
        } else {
            Some(mode::Observation::capture(s, &h.owner)?)
        };
        let service = context
            .team_id()
            .unwrap_or(&h.view.as_ref()?.home.installed_team);
        let policy =
            ChannelTeamPolicy::new(mode.as_ref().is_some_and(|v| v.enterprise()), service).ok()?;
        Some(Self {
            owner: h.owner.clone(),
            surface,
            form,
            key: key.clone(),
            input_revision: super::revision(s)?,
            widget_revision,
            raw: field.raw.clone(),
            value: field.value.clone(),
            context,
            mode,
            conversation: filter,
            policy,
        })
    }
    pub(crate) fn generation(&self) -> u64 {
        self.surface.visit
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && self.context.current(s)
            && self.mode.as_ref().is_none_or(|v| v.current(s))
            && Self::capture(s, &self.key).is_some_and(|v| self.same(&v))
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
            && self.conversation == other.conversation
            && match (&self.mode, &other.mode) {
                (None, None) => true,
                (Some(a), Some(b)) => a.same(b),
                _ => false,
            }
    }
    pub(crate) fn after_edit(&self, s: &ShellState) -> Option<Self> {
        let now = Self::capture(s, &self.key)?;
        (self.owner.current(s)
            && self.context.current(s)
            && self.mode.as_ref().is_none_or(|v| v.current(s))
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
        if let Some(s) = self.cancel.take() {
            let _ = s.send(true);
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
    let p = s.app_home.channels.picker.as_ref()?;
    (p.ticket.same(t) && t.current(s)).then(|| crate::app_modal::users::SearchView {
        serial: p.serial,
        query_revision: p.query_revision,
        query: p.query.clone(),
        candidates: p.candidates.clone(),
        loading: p.loading,
        error: p.error.clone(),
        agent_unavailable: false,
    })
}
/// Read custody is independent from interactive availability, including overlays.
pub(crate) fn display(s: &ShellState, key: &InputKey) -> Option<Vec<(String, String)>> {
    let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
    let owner = super::owner(s)?;
    let form = s.app_home.forms.get(&owner)?;
    if form.raw_view() != h.view.as_ref()?.raw() {
        return None;
    }
    let field = form.fields.get(key)?;
    if !matches!(
        field.kind,
        InputKind::Channel
            | InputKind::Conversation
            | InputKind::MultiChannel { .. }
            | InputKind::MultiConversation { .. }
    ) {
        return None;
    }
    let block = form.raw_view()["blocks"]
        .as_array()?
        .iter()
        .find(|b| b["block_id"] == key.block_id)?;
    if block["element"] != field.raw || field.raw["action_id"] != key.action_id {
        return None;
    }
    if matches!(
        field.kind,
        InputKind::MultiChannel { .. } | InputKind::MultiConversation { .. }
    ) {
        return multiple::display(s, &owner, key, h.id, &field.raw);
    }
    let widget = s.app_home.channels.widgets.get(&(owner, key.clone()))?;
    if widget.visit != h.id
        || widget.conversation
            != matches!(
                field.kind,
                InputKind::Conversation | InputKind::MultiConversation { .. }
            )
        || widget.initial
            != field
                .raw
                .get(conversation::initial_key(&field.raw))
                .cloned()
    {
        return None;
    }
    if widget.conversation {
        return Some(conversation::labels(
            s,
            &h.owner.ctx.session.team_id,
            widget.visible.iter(),
            widget.retained_choice.as_ref(),
        ));
    }
    let workspace = s.core.workspaces.get(&h.owner.ctx.session.team_id)?;
    Some(
        widget
            .visible
            .iter()
            .map(|id| {
                let label = workspace
                    .channels
                    .get(id)
                    .map(|c| conversations::channel_label(workspace, c))
                    .or_else(|| {
                        widget
                            .retained_choice
                            .as_ref()
                            .filter(|c| c.id == *id)
                            .map(|c| c.label.clone())
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
        .channels
        .multiples
        .get(&(t.form.clone(), t.key.clone()))
    {
        return w.visible.clone();
    }
    s.app_home
        .channels
        .widgets
        .get(&(t.form.clone(), t.key.clone()))
        .and_then(|w| w.visible.clone())
        .into_iter()
        .collect()
}
pub(crate) fn labels(s: &ShellState, t: &Ticket, ids: &[String]) -> Vec<(String, String)> {
    if t.conversation.is_some() && matches!(t.value, InputValue::Conversations(_)) {
        return multiple::display(s, &t.form, &t.key, t.surface.visit, &t.raw).unwrap_or_default();
    }
    if t.conversation.is_some() {
        let choice = s
            .app_home
            .channels
            .widgets
            .get(&(t.form.clone(), t.key.clone()))
            .and_then(|w| w.retained_choice.as_ref());
        return conversation::labels(s, &t.owner.ctx.session.team_id, ids.iter(), choice);
    }
    let w = s.core.workspaces.get(&t.owner.ctx.session.team_id);
    ids.iter()
        .map(|id| {
            let label = w
                .and_then(|w| {
                    w.channels.get(id).map(|c| {
                        crate::message_buttons::inputs::user::conversations::channel_label(w, c)
                    })
                })
                .or_else(|| {
                    s.app_home
                        .channels
                        .multiples
                        .get(&(t.form.clone(), t.key.clone()))
                        .and_then(|w| w.retained_choices.get(id))
                        .map(|c| c.label.clone())
                })
                .or_else(|| {
                    s.app_home
                        .channels
                        .widgets
                        .get(&(t.form.clone(), t.key.clone()))
                        .and_then(|v| v.retained_choice.as_ref())
                        .filter(|c| c.id == *id)
                        .map(|c| c.label.clone())
                })
                .unwrap_or_else(|| id.clone());
            (id.clone(), label)
        })
        .collect()
}
fn cached(s: &ShellState, t: &Ticket, query: &str) -> Vec<directory::Choice> {
    if t.conversation.is_some() {
        conversation::candidates(s, t, &[], &[], query)
    } else {
        policy::candidates(s, t, &[], query)
    }
}
pub(crate) fn open(mut state: Signal<ShellState>, ticket: Ticket) {
    open_local(&mut state.write(), ticket);
}
fn open_local(s: &mut ShellState, t: Ticket) -> bool {
    if !t.current(s) {
        return false;
    }
    if presentation(s, &t).is_some() {
        return true;
    }
    let candidates = cached(s, &t, "");
    let agent_unavailable = false;
    s.app_home.channels.picker = Some(Picker {
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
        .channels
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
    let p = s.app_home.channels.picker.as_ref().unwrap();
    if !p.ticket.same(t) || !p.candidates.iter().any(|c| c.id == id) {
        return false;
    }
    let choice = p.candidates.iter().find(|c| c.id == id).unwrap().clone();
    if !(if t.conversation.is_some() {
        conversation::choice_current(s, t, &choice)
    } else {
        policy::choice_current(s, t, &choice)
    }) {
        return false;
    }
    if matches!(
        t.value,
        InputValue::Channels(_) | InputValue::Conversations(_)
    ) {
        return multiple::toggle(s, t, choice, event);
    }
    if !edit(s, t, Some(id.into()), event) {
        return false;
    }
    s.app_home
        .channels
        .widgets
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap()
        .retained_choice = Some(choice);
    true
}
fn edit(
    s: &mut ShellState,
    t: &Ticket,
    value: Option<String>,
    event: &mut Option<dispatch::Event>,
) -> bool {
    if t.conversation.is_some() {
        if !conversation::dispatch::input(s, t, value.clone(), event) {
            return false;
        }
    } else {
        if !t.current(s) || !super::editable(s) {
            return false;
        }
        if s.app_home
            .channels
            .widgets
            .get(&(t.form.clone(), t.key.clone()))
            .and_then(|w| w.dispatch.as_ref())
            .is_none()
        {
            return false;
        }
        let Some(form) = s.app_home.forms.get_mut(&t.form) else {
            return false;
        };
        let Ok(action) = form.channel_input(&t.key, value.clone()) else {
            return false;
        };
        let Ok(parent) = form.channel_parent(&t.key) else {
            return false;
        };
        super::bump(s, &t.form);
        let w = s
            .app_home
            .channels
            .widgets
            .get_mut(&(t.form.clone(), t.key.clone()))
            .unwrap();
        let Some(custody) = &mut w.dispatch else {
            return false;
        };
        custody.parent = parent;
        *event = action.map(|action| dispatch::Event {
            action,
            binding: custody.binding.clone(),
            instance: custody.instance,
        });
    }
    let w = s
        .app_home
        .channels
        .widgets
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap();
    w.visible = value.clone();
    w.retained_choice = None;
    w.revision = next();
    s.app_home.channels.picker = None;
    let callback = if t.conversation.is_some() {
        json!({"selectedConversation":value})
    } else {
        json!({"selectedChannel":value})
    };
    super::static_select::fixture::event(s, &callback, &t.key);
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
    if matches!(
        t.value,
        InputValue::Channels(_) | InputValue::Conversations(_)
    ) {
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
        .channels
        .picker
        .as_ref()
        .is_some_and(|p| p.ticket.same(&ticket))
    {
        s.app_home.channels.picker = None;
    }
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    if s.app_home
        .channels
        .picker
        .as_ref()
        .is_some_and(|p| !p.ticket.current(s))
    {
        s.app_home.channels.picker = None;
        true
    } else {
        false
    }
}

pub(crate) fn unavailable(s: &ShellState, multi: bool) -> Option<&'static str> {
    (!multi)
        .then(|| mode::unavailable(s))
        .flatten()
        .or_else(|| {
            super::super::context::Witness::capture(s)
            .is_err()
            .then_some(
                "Channel lookup is unavailable for this Home context. Your selection is retained.",
            )
        })
}
