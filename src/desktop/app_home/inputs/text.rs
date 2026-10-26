//! Home text dispatch owns actual input occurrences; it is not a Button action.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use super_platinum_core::block_actions::{
    form::{
        InputKind,
        home::{Dispatch, TextTrigger},
    },
    home::TextAction,
};
mod bridge;
pub(crate) mod button;
mod delivery;
pub(crate) mod email;
pub(crate) mod fixture;
pub(crate) mod number;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct State {
    pub(super) focus: super::focus::State,
    mounts: BTreeMap<(FormOwner, InputKey), Mount>,
    numbers: BTreeMap<(FormOwner, InputKey), number::Widget>,
    emails: BTreeMap<(FormOwner, InputKey), email::Widget>,
    queued: BTreeMap<(FormOwner, InputKey), Queued>,
    activity: BTreeMap<(FormOwner, String), Activity>,
    was_cooling: bool,
    fixture: Option<fixture::Harness>,
    button_fixture: Option<button::fixture::Harness>,
}
struct Activity {
    binding: Binding,
    route: context::Witness,
    loading: bool,
    error: Option<String>,
}
struct Mount {
    id: u64,
    seq: u64,
    value: String,
    binding: Binding,
}
#[derive(Clone)]
struct Binding {
    owner: Owner,
    form: FormOwner,
    visit: actions::Key,
    key: InputKey,
    raw: serde_json::Value,
}
impl Binding {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && snapshot_current(s, self.visit)
            && owner(s).as_ref() == Some(&self.form)
            && form(s).is_some_and(|f| {
                f.raw_view() == &self.raw
                    && (f.text_field(&self.key).is_ok()
                        || f.number_field(&self.key).is_ok()
                        || f.email_field(&self.key).is_ok())
            })
    }
    fn interactive(&self, s: &ShellState) -> bool {
        self.current(s) && super::current(s, self.visit) && !cooling(s, &self.owner.ctx)
    }
}
#[derive(Clone)]
struct Queued {
    id: u64,
    at: Instant,
    binding: Binding,
    mount: u64,
    action: TextAction,
    route: context::Witness,
}
impl Queued {
    fn current(&self, s: &ShellState) -> bool {
        self.binding.interactive(s)
            && self.route.current(s)
            && s.app_home
                .text
                .mounts
                .get(&(self.binding.form.clone(), self.binding.key.clone()))
                .is_some_and(|m| m.id == self.mount && m.binding.current(s))
    }
}
pub(crate) fn dispatching(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| f.text_field(key).is_ok() && f.dispatch(key) != Some(Dispatch::Quiet))
}
pub(super) fn ready(s: &ShellState) -> bool {
    let Some(f) = form(s) else { return false };
    let Some(o) = owner(s) else { return false };
    f.fields
        .iter()
        .filter(|(k, f)| {
            (matches!(f.kind, InputKind::Text { .. }) && dispatching(s, k))
                || number::supported(s, k)
                || email::supported(s, k)
        })
        .all(|(k, f)| {
            s.app_home
                .text
                .mounts
                .get(&(o.clone(), k.clone()))
                .is_some_and(|m| {
                    m.seq > 0
                        && m.binding.current(s)
                        && (matches!(&f.value,InputValue::Text(v) if v==&m.value)
                            || number::settled(s, k, m)
                            || email::settled(s, k, m))
                })
        })
}
// Record only the render-visible cooldown state. Publication must seed this
// before a delayed runtime tick can observe an already-expired deadline.
pub(crate) fn observe_cooling(s: &mut ShellState) -> bool {
    let cooling_now = (!s.app_home.text.mounts.is_empty() || !s.app_home.text.activity.is_empty())
        && s.app_home
            .surface
            .as_ref()
            .is_some_and(|h| h.owner.current(s) && cooling(s, &h.owner.ctx));
    let cooling_changed = s.app_home.text.was_cooling != cooling_now;
    s.app_home.text.was_cooling = cooling_now;
    cooling_changed
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let email_changed = email::poll(s);
    let cooling_changed = observe_cooling(s);
    let stale = s
        .app_home
        .text
        .queued
        .iter()
        .filter(|(_, q)| !q.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in &stale {
        s.app_home.text.queued.remove(k);
    }
    let stale_mounts = s
        .app_home
        .text
        .mounts
        .iter()
        .filter(|(_, m)| !m.binding.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in &stale_mounts {
        s.app_home.text.mounts.remove(k);
    }
    let stale_activity = s
        .app_home
        .text
        .activity
        .iter()
        .filter(|(_, a)| !a.binding.current(s) || !a.route.current(s))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in &stale_activity {
        s.app_home.text.activity.remove(k);
    }
    email_changed
        || cooling_changed
        || !stale.is_empty()
        || !stale_mounts.is_empty()
        || !stale_activity.is_empty()
}
pub(crate) fn status(s: &ShellState, block: &str) -> Option<String> {
    let a = s.app_home.text.activity.get(&(owner(s)?, block.into()))?;
    if !a.binding.current(s) || !a.route.current(s) {
        return None;
    }
    if a.loading {
        Some("Sending to app…".into())
    } else {
        a.error.clone()
    }
}
pub(crate) use bridge::Control;

#[derive(Deserialize, Serialize, Clone)]
struct Occurrence {
    mount: u64,
    seq: u64,
    kind: String,
    value: String,
    key: Option<String>,
    code: Option<String>,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    repeat: bool,
    composing: bool,
    trusted: bool,
    prevented: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    number: Option<number::NumericOccurrence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<email::EmailOccurrence>,
}
fn receive(s: &mut ShellState, b: &Binding, e: &Occurrence) -> Option<u64> {
    if !b.current(s) || e.value.len() > 4 * 1024 * 1024 {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let m = s.app_home.text.mounts.get_mut(&index)?;
    if m.id != e.mount || e.seq != m.seq + 1 {
        return None;
    }
    m.seq = e.seq;
    m.value = e.value.clone();
    if email::supported(s, &b.key) {
        return email::receive(s, b, e);
    }
    if number::supported(s, &b.key) {
        return number::receive(s, b, e);
    }
    if e.kind == "mounted" {
        return None;
    }
    if !b.interactive(s) {
        return None;
    }
    let trigger = match e.kind.as_str() {
        "input" => TextTrigger::Change,
        "enter" if e.key.as_deref() == Some("Enter") && e.prevented => TextTrigger::Enter,
        _ => return None,
    };
    let event = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .text_input(&b.key, e.value.clone(), trigger)
        .ok()?;
    bump(s, &b.form);
    fixture::event(s, e, &b.key);
    let event = event?;
    queue(s, b, e.mount, event)
}
fn queue(
    s: &mut ShellState,
    b: &Binding,
    mount: u64,
    event: super_platinum_core::block_actions::form::home::TextEvent,
) -> Option<u64> {
    let q = capture(s, b, mount, event)?;
    let id = q.id;
    s.app_home
        .text
        .queued
        .insert((b.form.clone(), b.key.clone()), q);
    Some(id)
}
fn capture(
    s: &ShellState,
    b: &Binding,
    mount: u64,
    event: super_platinum_core::block_actions::form::home::TextEvent,
) -> Option<Queued> {
    let route = context::Witness::capture(s).ok()?;
    let h = s.app_home.surface.as_ref()?;
    let action = h
        .view
        .as_ref()?
        .text_action(&event, &b.owner.ctx.session.user_id)
        .ok()?;
    let id = next();
    Some(Queued {
        id,
        at: Instant::now(),
        binding: b.clone(),
        mount,
        action,
        route,
    })
}
