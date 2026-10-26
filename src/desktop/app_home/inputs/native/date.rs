//! Home date display, native edit buffer and committed parent are separate.
use super::*;
use crate::app_home::inputs::static_select::Binding;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::home::{DateParent, StaticEvent};
mod control;
pub(crate) mod fixture;
pub(crate) mod focus_fixture;
pub(crate) mod popup_fixture;
pub(crate) use control::Control;
#[cfg(test)]
mod tests;

pub(super) struct Widget {
    visit: u64,
    initial: Option<Value>,
    visible: Option<String>,
    parent: DateParent,
    binding: Option<Binding>,
    initialized: bool,
    buffer_valid: bool,
    raw: String,
    raw_dirty: bool,
    parsed: Option<String>,
    picker: String,
    focused: bool,
    picker_status: String,
    resolved_raw: Option<String>,
    resolved_selected: Option<String>,
}

pub(crate) fn active(s: &ShellState) -> bool {
    form(s).is_some_and(|f| f.fields.keys().any(|k| f.date_field(k).is_ok()))
}
pub(crate) fn parents(s: &ShellState) -> Vec<DateParent> {
    let Some(o) = owner(s) else { return vec![] };
    s.app_home
        .native
        .dates
        .iter()
        .filter(|((f, key), w)| {
            f == &o
                && w.initialized
                && w.visit == s.app_home.surface.as_ref().unwrap().id
                && w.binding.as_ref().is_some_and(|b| {
                    b.owner.current(s) && form(s).is_some_and(|f| f.raw_view() == &b.raw)
                })
                && (survives(s, key) || protects_buffer(s, f, key, w))
        })
        .map(|(_, w)| w.parent.clone())
        .collect()
}
fn survives(s: &ShellState, key: &InputKey) -> bool {
    s.app_home
        .surface
        .as_ref()
        .and_then(|h| h.view.as_ref())
        .and_then(|v| v.raw()["blocks"].as_array())
        .is_some_and(|blocks| {
            blocks.iter().any(|b| {
                b["type"] == "input"
                    && b["block_id"] == key.block_id
                    && b["element"]["action_id"] == key.action_id
                    && b["element"]["type"] == "datepicker"
            })
        })
}
fn protects_buffer(s: &ShellState, o: &FormOwner, key: &InputKey, w: &Widget) -> bool {
    // A pristine formatted display may disappear with its source field. A
    // received literal edit must not disappear merely because parent is pristine.
    // Keep its token so core's existing conflict path refuses removal atomically.
    w.raw_dirty
        || !w.buffer_valid
        || w.picker != w.visible.as_deref().unwrap_or_default()
        || s.app_home
            .native
            .entries
            .get(&(o.clone(), key.clone()))
            .is_none_or(|e| {
                e.pending || e.invalid || e.value != json!([w.raw, w.picker]).to_string()
            })
}
pub(crate) fn hydrate(s: &mut ShellState) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let fields = f
        .fields
        .keys()
        .filter_map(|k| {
            let field = f.date_field(k).ok()?;
            let retained = s
                .app_home
                .native
                .dates
                .get(&(o.clone(), k.clone()))
                .filter(|_| f.native_error_text(k).is_some());
            let seed = f
                .date_seed(k)
                .ok()
                .or_else(|| retained.map(|w| w.visible.clone()));
            let parent = f
                .date_parent(k)
                .ok()
                .or_else(|| retained.map(|w| w.parent.clone()));
            Some((
                k.clone(),
                field.raw.get("initial_date").cloned(),
                seed,
                parent,
            ))
        })
        .collect::<Vec<_>>();
    s.app_home.native.dates.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || fields.iter().any(|(k, _, _, _)| k == key))
    });
    for (key, initial, seed, parent) in fields {
        let index = (o.clone(), key);
        if let Some(w) = s
            .app_home
            .native
            .dates
            .get_mut(&index)
            .filter(|w| w.visit == visit)
        {
            if w.initial != initial {
                w.visible = initial.as_ref().and_then(Value::as_str).map(str::to_owned);
                w.initial = initial;
            }
        } else if s.app_home.native.dates.len() < 256
            || s.app_home.native.dates.contains_key(&index)
        {
            if let (Some(seed), Some(parent)) = (seed, parent) {
                s.app_home.native.dates.insert(
                    index,
                    Widget {
                        visit,
                        initial,
                        visible: seed,
                        parent,
                        binding: None,
                        initialized: false,
                        buffer_valid: false,
                        raw: String::new(),
                        raw_dirty: false,
                        parsed: None,
                        picker: String::new(),
                        focused: false,
                        picker_status: "idle".into(),
                        resolved_raw: None,
                        resolved_selected: None,
                    },
                );
            }
        }
    }
}
pub(super) fn configure(s: &mut ShellState, b: &Binding) -> Option<(Option<String>, bool)> {
    let optional = form(s)?.date_field(&b.key).ok()?.optional;
    let w = s
        .app_home
        .native
        .dates
        .get_mut(&(b.form.clone(), b.key.clone()))?;
    if w.visit != b.visit.visit {
        return None;
    }
    w.binding = Some(b.clone());
    Some((w.visible.clone(), optional))
}
pub(crate) fn mounted(s: &ShellState, b: &Binding, mount: u64) -> bool {
    form(s).is_some_and(|f| f.date_field(&b.key).is_ok())
        && b.current(s)
        && s.app_home
            .native
            .entries
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|e| e.mount == mount)
        && s.app_home
            .native
            .dates
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|w| {
                w.visit == b.visit.visit && w.binding.as_ref().is_some_and(|b| b.current(s))
            })
}
pub(super) fn settled(s: &ShellState, o: &FormOwner, key: &InputKey, _e: &Entry) -> bool {
    s.app_home
        .native
        .dates
        .get(&(o.clone(), key.clone()))
        // Acknowledged valid draft input does not commit or cancel the earlier
        // date action. The final JS lease still checks its exact native buffer.
        .is_some_and(|w| w.binding.as_ref().is_some_and(|b| b.current(s)))
}
#[derive(Deserialize, Serialize)]
pub(super) struct Event {
    mount: u64,
    seq: u64,
    kind: String,
    value: String,
    raw: String,
    #[serde(default)]
    raw_dirty: bool,
    picker: String,
    selected: Option<String>,
    parsed: Option<String>,
    focused: bool,
    picker_status: String,
    resolved_raw: Option<String>,
    resolved_selected: Option<String>,
    buffer_valid: bool,
    valid: bool,
    bad: bool,
}
pub(super) fn receive(
    s: &mut ShellState,
    b: &Binding,
    mount: u64,
    event: &Event,
) -> Option<StaticEvent> {
    if b.form.epoch != s.core.account_epoch
        || event.mount != mount
        || event.raw.len() > 4096
        || event.value.len() > 8192
        || serde_json::to_string(&(&event.raw, &event.picker))
            .ok()
            .as_deref()
            != Some(event.value.as_str())
        || !matches!(
            event.kind.as_str(),
            "mounted" | "input" | "commit" | "closed"
        )
    {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let e = s.app_home.native.entries.get_mut(&index)?;
    if e.mount != mount || event.seq != e.seq + 1 {
        return None;
    }
    e.seq = event.seq;
    e.pending = event.kind == "closed";
    e.value = event.value.clone();
    e.invalid = !event.valid || event.bad || !event.buffer_valid;
    // A detached event may preserve its native buffer, never acquire callback authority.
    if !b.current(s) || event.kind == "closed" {
        return None;
    }
    let w = s.app_home.native.dates.get_mut(&index)?;
    if event.kind == "mounted" {
        w.initialized = true;
    }
    w.buffer_valid = event.buffer_valid;
    w.raw = event.raw.clone();
    w.raw_dirty = event.raw_dirty;
    w.parsed = event.parsed.clone();
    w.picker = event.picker.clone();
    w.focused = event.focused;
    w.picker_status = event.picker_status.clone();
    w.resolved_raw = event.resolved_raw.clone();
    w.resolved_selected = event.resolved_selected.clone();
    if !event.valid || event.bad {
        let _=s.app_home.forms.get_mut(&b.form)?.set_native_error(&b.key,String::new(),"This native date or time is not confirmed. Complete its picker, or explicitly reset the inputs; app actions remain blocked.".into());
        bump(s, &b.form);
        return None;
    }
    if !event.buffer_valid {
        return None;
    }
    let f = s.app_home.forms.get_mut(&b.form)?;
    let had_error = f.native_error_text(&b.key).is_some();
    f.date_acknowledge_native_buffer(&b.key, event.parsed.as_deref())
        .ok()?;
    if had_error {
        bump(s, &b.form);
    }
    if event.kind != "commit" {
        return None;
    }
    if !b.interactive(s) {
        return None;
    }
    let selected = event.selected.as_deref()?;
    let value = (!selected.is_empty()).then(|| selected.to_owned());
    let w = s.app_home.native.dates.get(&index)?;
    if w.visible.as_deref() == Some(selected) {
        return None;
    }
    // Required clearing is not a native affordance; the JS commit restores its
    // prior visible date, whereas direct null callbacks stay core-only evidence.
    if value.is_none() && !form(s)?.date_field(&b.key).ok()?.optional {
        return None;
    }
    let event_action = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .date_input(&b.key, value.clone())
        .ok()?;
    s.app_home.native.dates.get_mut(&index)?.visible = Some(selected.to_owned());
    crate::app_home::inputs::static_select::fixture::event(s, event, &b.key);
    bump(s, &b.form);
    event_action
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let o = owner(s);
    json!({"dateWidgets":o.as_ref().map(|o|s.app_home.native.dates.iter().filter(|((f,_),_)|f==o).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"initial":w.initial,"visible":w.visible,"raw":w.raw,"bufferValid":w.buffer_valid,"parsed":w.parsed,"picker":w.picker,"focused":w.focused,"pickerStatus":w.picker_status,"resolvedRaw":w.resolved_raw,"resolvedSelected":w.resolved_selected,"initialized":w.initialized,"current":w.binding.as_ref().is_some_and(|b|b.current(s))})).collect::<Vec<_>>()),
        "nativeMounts":o.as_ref().map(|o|s.app_home.native.entries.iter().filter(|((f,_),_)|f==o).map(|((_,k),e)|json!({"block":k.block_id,"action":k.action_id,"id":e.id,"mount":e.mount,"seq":e.seq,"value":e.value,"pending":e.pending,"invalid":e.invalid})).collect::<Vec<_>>())})
}

// Serial-state tests seed an already acknowledged native control. This is not
// used by fixtures or production mounting and grants no DOM occurrence claim.
#[cfg(test)]
pub(super) fn seed_value(s: &mut ShellState, key: &InputKey, value: &str) {
    let Some(o) = owner(s) else { return };
    let Some(h) = s.app_home.surface.as_ref() else {
        return;
    };
    let b = Binding {
        owner: h.owner.clone(),
        form: o.clone(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key.clone(),
        raw: form(s).unwrap().raw_view().clone(),
    };
    if let Some(w) = s.app_home.native.dates.get_mut(&(o, key.clone())) {
        w.visible = (!value.is_empty()).then(|| value.to_owned());
        w.buffer_valid = true;
        w.raw = value.to_owned();
        w.picker = value.to_owned();
        w.parsed = w.visible.clone();
        w.binding = Some(b);
        w.initialized = true;
    }
}
