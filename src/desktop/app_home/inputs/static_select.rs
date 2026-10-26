//! Native Home single-static callbacks own their source and debounce window.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use super_platinum_core::block_actions::{
    form::{InputKind, home::Dispatch},
    home::StaticAction,
};
mod bridge;
#[cfg(test)]
mod channel_tests;
pub(crate) mod checkboxes;
#[cfg(test)]
mod conversation_tests;
mod delivery;
#[cfg(test)]
mod external_tests;
pub(crate) mod fixture;
#[cfg(test)]
mod multi_channel_tests;
#[cfg(test)]
mod multi_conversation_tests;
#[cfg(test)]
mod multi_external_tests;
#[cfg(test)]
mod multi_user_tests;
pub(crate) mod multiple;
pub(crate) mod radio;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod user_tests;
pub(crate) mod wrappers;
pub(crate) use bridge::Control;
pub(crate) use bridge::{Lease as ButtonLease, lock_for_button};

#[derive(Default)]
pub(crate) struct State {
    radios: BTreeMap<(FormOwner, InputKey), radio::Widget>,
    radio_mounts: BTreeMap<(FormOwner, InputKey), radio::Mount>,
    checks: BTreeMap<(FormOwner, InputKey), checkboxes::Widget>,
    check_mounts: BTreeMap<(FormOwner, InputKey), checkboxes::Mount>,
    multiples: BTreeMap<(FormOwner, InputKey), multiple::Widget>,
    multi_mounts: BTreeMap<(FormOwner, InputKey), multiple::Mount>,
    widgets: BTreeMap<(FormOwner, InputKey), Widget>,
    mounts: BTreeMap<(FormOwner, InputKey), Mount>,
    windows: BTreeMap<(FormOwner, InputKey), Window>,
    activity: BTreeMap<(FormOwner, String), Activity>,
    was_cooling: bool,
    fixture: Option<fixture::Harness>,
}
struct Widget {
    visit: u64,
    initial: Option<Value>,
    options: Vec<String>,
    visible: Option<String>,
}
struct Mount {
    id: u64,
    seq: u64,
    value: Option<String>,
    binding: Binding,
}
#[derive(Clone)]
pub(crate) struct Binding {
    pub(in crate::app_home::inputs) owner: Owner,
    pub(in crate::app_home::inputs) form: FormOwner,
    pub(in crate::app_home::inputs) visit: actions::Key,
    pub(in crate::app_home::inputs) key: InputKey,
    pub(in crate::app_home::inputs) raw: Value,
}
impl Binding {
    pub(in crate::app_home::inputs) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && snapshot_current(s, self.visit)
            && owner(s).as_ref() == Some(&self.form)
            && form(s).is_some_and(|f| {
                f.raw_view() == &self.raw
                    && (f.static_field(&self.key).is_ok()
                        || f.multi_static_field(&self.key).is_ok()
                        || f.checkbox_field(&self.key).is_ok()
                        || f.radio_field(&self.key).is_ok()
                        || f.date_field(&self.key).is_ok()
                        || f.time_field(&self.key).is_ok()
                        || f.datetime_field(&self.key).is_ok()
                        || f.external_field(&self.key).is_ok_and(|f| {
                            matches!(
                                f.kind,
                                InputKind::ExternalSelect { .. }
                                    | InputKind::MultiExternalSelect { .. }
                            )
                        })
                        || f.channel_field(&self.key).is_ok_and(|f| {
                            matches!(f.kind, InputKind::Channel | InputKind::MultiChannel { .. })
                        })
                        || f.conversation_field(&self.key).is_ok_and(|f| {
                            matches!(
                                f.kind,
                                InputKind::Conversation | InputKind::MultiConversation { .. }
                            )
                        })
                        || f.user_field(&self.key).is_ok_and(|f| {
                            matches!(f.kind, InputKind::User | InputKind::MultiUser { .. })
                        }))
            })
    }
    pub(in crate::app_home::inputs) fn interactive(&self, s: &ShellState) -> bool {
        self.current(s) && super::current(s, self.visit) && !cooling(s, &self.owner.ctx)
    }
}
#[derive(Clone)]
struct Queued {
    id: u64,
    at: Instant,
    binding: Binding,
    mount: u64,
    action: StaticAction,
    route: context::Witness,
    edge: &'static str,
}
impl Queued {
    fn current(&self, s: &ShellState) -> bool {
        self.binding.interactive(s)
            && self.route.current(s)
            && (super::users::dispatch::instance_current(s, &self.binding, self.mount)
                || super::channels::dispatch::instance_current(s, &self.binding, self.mount)
                || super::external::dispatch::instance_current(s, &self.binding, self.mount)
                || super::native::datetime::mounted(s, &self.binding, self.mount)
                || super::native::date::mounted(s, &self.binding, self.mount)
                || super::native::time::mounted(s, &self.binding, self.mount)
                || s.app_home
                    .static_select
                    .mounts
                    .get(&(self.binding.form.clone(), self.binding.key.clone()))
                    .is_some_and(|m| m.id == self.mount && m.binding.current(s))
                || s.app_home
                    .static_select
                    .multi_mounts
                    .get(&(self.binding.form.clone(), self.binding.key.clone()))
                    .is_some_and(|m| m.id == self.mount && m.binding.current(s))
                || s.app_home
                    .static_select
                    .radio_mounts
                    .get(&(self.binding.form.clone(), self.binding.key.clone()))
                    .is_some_and(|m| m.id == self.mount && m.binding.current(s))
                || s.app_home
                    .static_select
                    .check_mounts
                    .get(&(self.binding.form.clone(), self.binding.key.clone()))
                    .is_some_and(|m| m.id == self.mount && m.binding.current(s)))
    }
}
struct Window {
    id: u64,
    last: Instant,
    binding: Binding,
    trailing: Option<Queued>,
}
struct Activity {
    binding: Binding,
    route: context::Witness,
    loading: bool,
    error: Option<String>,
}
pub(crate) fn dispatching(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| {
        (f.static_field(key).is_ok()
            || f.multi_static_field(key).is_ok()
            || f.checkbox_field(key).is_ok()
            || f.radio_field(key).is_ok())
            && f.dispatch(key) == Some(Dispatch::Change)
    })
}
pub(super) fn reset(s: &mut ShellState, owner: &FormOwner) {
    let state = &mut s.app_home.static_select;
    state.radios.retain(|(o, _), _| o != owner);
    state.radio_mounts.retain(|(o, _), _| o != owner);
    state.checks.retain(|(o, _), _| o != owner);
    state.check_mounts.retain(|(o, _), _| o != owner);
    state.multiples.retain(|(o, _), _| o != owner);
    state.multi_mounts.retain(|(o, _), _| o != owner);
    state.widgets.retain(|(o, _), _| o != owner);
    state.mounts.retain(|(o, _), _| o != owner);
    state.windows.retain(|(o, _), _| o != owner);
    state.activity.retain(|(o, _), _| o != owner);
}
pub(super) fn hydrate(s: &mut ShellState) {
    multiple::hydrate(s);
    checkboxes::hydrate(s);
    radio::hydrate(s);
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let fields = f
        .fields
        .iter()
        .filter(|(k, _)| dispatching(s, k) && f.static_field(k).is_ok())
        .map(|(k, f)| {
            let InputKind::StaticSelect { options, .. } = &f.kind else {
                unreachable!()
            };
            (
                k.clone(),
                f.raw.get("initial_option").cloned(),
                options.iter().map(|o| o.value.clone()).collect::<Vec<_>>(),
                match &f.value {
                    InputValue::Selected(v) => v.first().cloned(),
                    _ => None,
                },
            )
        })
        .collect::<Vec<_>>();
    s.app_home.static_select.widgets.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || fields.iter().any(|(k, _, _, _)| k == key))
    });
    for (key, initial, options, value) in fields {
        let index = (o.clone(), key);
        if let Some(w) = s
            .app_home
            .static_select
            .widgets
            .get_mut(&index)
            .filter(|w| w.visit == visit)
        {
            if w.initial != initial {
                w.visible = initial
                    .as_ref()
                    .and_then(|v| v["value"].as_str())
                    .map(str::to_owned);
                w.initial = initial;
            }
            if w.options != options && w.visible.as_ref().is_some_and(|v| !options.contains(v)) {
                w.visible = None;
            }
            w.options = options;
        } else if s.app_home.static_select.widgets.len() < 256
            || s.app_home.static_select.widgets.contains_key(&index)
        {
            // iGIl maps a committed null to undefined savedOption. A fresh
            // z3Rn widget falls back to initialOption without a parent callback.
            let visible = value.or_else(|| {
                initial
                    .as_ref()
                    .and_then(|v| v["value"].as_str())
                    .map(str::to_owned)
            });
            s.app_home.static_select.widgets.insert(
                index,
                Widget {
                    visit,
                    initial,
                    options,
                    visible,
                },
            );
        }
    }
}
pub(super) fn ready(s: &ShellState) -> bool {
    let Some(f) = form(s) else { return false };
    let Some(o) = owner(s) else { return false };
    multiple::ready(s)
        && checkboxes::ready(s)
        && radio::ready(s)
        && f.fields
            .keys()
            .filter(|k| dispatching(s, k) && f.static_field(k).is_ok())
            .all(|k| {
                let index = (o.clone(), k.clone());
                s.app_home
                    .static_select
                    .mounts
                    .get(&index)
                    .is_some_and(|m| {
                        m.seq > 0
                            && m.binding.current(s)
                            && s.app_home
                                .static_select
                                .widgets
                                .get(&index)
                                .is_some_and(|w| w.visible == m.value)
                    })
            })
}
pub(crate) fn loading(s: &ShellState, block: &str) -> bool {
    owner(s)
        .and_then(|o| s.app_home.static_select.activity.get(&(o, block.into())))
        .is_some_and(|a| a.binding.current(s) && a.route.current(s) && a.loading)
}
pub(crate) fn status(s: &ShellState, block: &str) -> Option<String> {
    let a = s
        .app_home
        .static_select
        .activity
        .get(&(owner(s)?, block.into()))?;
    if !a.binding.current(s) || !a.route.current(s) {
        return None;
    }
    if a.loading {
        Some("Sending selection to app…".into())
    } else {
        a.error.clone()
    }
}
pub(crate) fn observe_cooling(s: &mut ShellState) -> bool {
    let now = (!s.app_home.static_select.mounts.is_empty()
        || !s.app_home.static_select.multi_mounts.is_empty()
        || !s.app_home.static_select.check_mounts.is_empty()
        || !s.app_home.static_select.radio_mounts.is_empty()
        || !s.app_home.static_select.activity.is_empty())
        && s.app_home
            .surface
            .as_ref()
            .is_some_and(|h| h.owner.current(s) && cooling(s, &h.owner.ctx));
    let changed = s.app_home.static_select.was_cooling != now;
    s.app_home.static_select.was_cooling = now;
    changed
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let mut changed = observe_cooling(s);
    changed |= multiple::poll(s);
    changed |= checkboxes::poll(s);
    changed |= radio::poll(s);
    let stale = s
        .app_home
        .static_select
        .mounts
        .iter()
        .filter(|(_, m)| !m.binding.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in stale {
        s.app_home.static_select.mounts.remove(&k);
        changed = true;
    }
    let stale = s
        .app_home
        .static_select
        .windows
        .iter()
        .filter(|(_, w)| {
            !w.binding.interactive(s) || w.trailing.as_ref().is_some_and(|q| !q.current(s))
        })
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in stale {
        s.app_home.static_select.windows.remove(&k);
        changed = true;
    }
    let stale = s
        .app_home
        .static_select
        .activity
        .iter()
        .filter(|(_, a)| !a.binding.current(s) || !a.route.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in stale {
        s.app_home.static_select.activity.remove(&k);
        changed = true;
    }
    changed
}
#[derive(Clone, Serialize, Deserialize)]
struct Occurrence {
    mount: u64,
    seq: u64,
    kind: String,
    index: Option<usize>,
    trusted: bool,
}
fn receive(s: &mut ShellState, b: &Binding, e: &Occurrence) -> Option<(u64, Option<Queued>)> {
    if !b.current(s) {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let field = form(s)?.static_field(&b.key).ok()?;
    let InputKind::StaticSelect { options, .. } = &field.kind else {
        return None;
    };
    let value = match e.index {
        Some(i) => {
            Some(options.get(i)?.value.clone()).filter(|v| e.kind != "change" || !v.is_empty())
        }
        None => None,
    };
    let allows_clear = form(s)?.static_allows_clear(&b.key).ok()?;
    let m = s.app_home.static_select.mounts.get_mut(&index)?;
    if m.id != e.mount || e.seq != m.seq + 1 {
        return None;
    }
    m.seq = e.seq;
    m.value = value.clone();
    if e.kind != "change" || !b.interactive(s) {
        return None;
    }
    // Required clear is absent from the native affordance. Invalid direct parent
    // callbacks are covered by the core contract, not forged native changes.
    if e.index.is_none() && !allows_clear {
        return None;
    }
    s.app_home.static_select.widgets.get_mut(&index)?.visible = value.clone();
    let event = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .static_input(&b.key, value)
        .ok()?;
    bump(s, &b.form);
    fixture::event(s, e, &b.key);
    let event = event?;
    let route = context::Witness::capture(s).ok()?;
    let action = s
        .app_home
        .surface
        .as_ref()?
        .view
        .as_ref()?
        .static_action(&event, &b.owner.ctx.session.user_id)
        .ok()?;
    let id = next();
    let at = Instant::now();
    let mut q = Queued {
        id,
        at,
        binding: b.clone(),
        mount: e.mount,
        action,
        route,
        edge: "leading",
    };
    if let Some(w) = s.app_home.static_select.windows.get_mut(&index) {
        w.id = id;
        w.last = at;
        q.edge = "trailing";
        w.trailing = Some(q);
        Some((id, None))
    } else {
        s.app_home.static_select.windows.insert(
            index,
            Window {
                id,
                last: at,
                binding: b.clone(),
                trailing: None,
            },
        );
        Some((id, Some(q)))
    }
}
async fn trailing(mut s: Signal<ShellState>, index: (FormOwner, InputKey), id: u64) {
    tokio::time::sleep(Duration::from_millis(500)).await;
    let q = take_trailing(&mut s.write(), &index, id);
    if let Some(q) = q {
        delivery::deliver(s, q).await;
    }
}

// Called only after the per-valid-event 500ms sleep. Replaced timer IDs
// cannot consume the current window, including repeated equal-array callbacks.
fn take_trailing(s: &mut ShellState, index: &(FormOwner, InputKey), id: u64) -> Option<Queued> {
    if s.app_home
        .static_select
        .windows
        .get(index)
        .is_none_or(|w| w.id != id)
    {
        return None;
    }
    s.app_home
        .static_select
        .windows
        .remove(index)
        .and_then(|w| w.trailing)
}

/// Calendar commits retain this entry point for existing native callers.
pub(in crate::app_home::inputs) fn date_event(
    state: Signal<ShellState>,
    b: Binding,
    mount: u64,
    event: super_platinum_core::block_actions::form::home::StaticEvent,
) {
    typed_event(state, b, mount, event);
}

/// Typed callbacks share source-owned windows; each family supplies its own custody.
pub(in crate::app_home::inputs) fn typed_event(
    state: Signal<ShellState>,
    b: Binding,
    mount: u64,
    event: super_platinum_core::block_actions::form::home::StaticEvent,
) {
    let queued = {
        let mut signal = state;
        let mut s = signal.write();
        enqueue_typed(&mut s, b, mount, event)
    };
    let Some((index, id, leading)) = queued else {
        return;
    };
    if let Some(q) = leading {
        dioxus::core::spawn_forever(delivery::deliver(state, q));
    }
    dioxus::core::spawn_forever(trailing(state, index, id));
}

fn enqueue_typed(
    s: &mut ShellState,
    b: Binding,
    mount: u64,
    event: super_platinum_core::block_actions::form::home::StaticEvent,
) -> Option<((FormOwner, InputKey), u64, Option<Queued>)> {
    if !b.interactive(&s) {
        return None;
    }
    let Ok(route) = context::Witness::capture(&s) else {
        return None;
    };
    let Some(view) = s.app_home.surface.as_ref().and_then(|h| h.view.as_ref()) else {
        return None;
    };
    let Ok(action) = view.static_action(&event, &b.owner.ctx.session.user_id) else {
        return None;
    };
    let id = next();
    let at = Instant::now();
    let index = (b.form.clone(), b.key.clone());
    // Original multi-lookup callbacks share the window machinery but have no
    // leading edge. The other accepted typed controls retain their cadence.
    let trailing_only = form(s).is_some_and(|f| {
        f.fields.get(&b.key).is_some_and(|f| {
            matches!(
                f.kind,
                InputKind::MultiUser { .. }
                    | InputKind::MultiChannel { .. }
                    | InputKind::MultiConversation { .. }
                    | InputKind::MultiExternalSelect { .. }
            )
        })
    });
    let mut q = Queued {
        id,
        at,
        binding: b.clone(),
        mount,
        action,
        route,
        edge: if trailing_only { "trailing" } else { "leading" },
    };
    let leading = if let Some(w) = s.app_home.static_select.windows.get_mut(&index) {
        q.edge = "trailing";
        w.id = id;
        w.last = at;
        w.trailing = Some(q);
        None
    } else {
        let (trailing, leading) = if trailing_only {
            (Some(q), None)
        } else {
            (None, Some(q))
        };
        s.app_home.static_select.windows.insert(
            index.clone(),
            Window {
                id,
                last: at,
                binding: b,
                trailing,
            },
        );
        leading
    };
    Some((index, id, leading))
}
