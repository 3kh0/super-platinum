//! One Home datetime epoch owns two native child buffers and one event sequence.
use super::*;
use crate::app_home::inputs::static_select::Binding;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::home::{DatetimeParent, StaticEvent};
mod control;
pub(crate) mod editing_fixture;
pub(crate) mod fixture;
pub(crate) mod popup_fixture;
#[cfg(test)]
mod tests;
pub(crate) use control::Control;

pub(super) struct Widget {
    visit: u64,
    initial: Option<Value>,
    visible: i64,
    parent: DatetimeParent,
    binding: Option<Binding>,
    initialized: bool,
    locale: String,
    time24: bool,
    local_invalid: bool,
    expanded: bool,
    date_raw: String,
    time_raw: String,
    picker: String,
    date_parsed: Option<String>,
    date_focused: bool,
    date_raw_dirty: bool,
    time_raw_dirty: bool,
    date_buffer_valid: bool,
    date_resolved_raw: Option<String>,
    date_resolved_selected: Option<String>,
    picker_status: String,
}

pub(crate) fn active(s: &ShellState) -> bool {
    form(s).is_some_and(|f| f.fields.keys().any(|k| f.datetime_field(k).is_ok()))
}
pub(crate) fn parents(s: &ShellState) -> Vec<DatetimeParent> {
    let Some(o) = owner(s) else { return vec![] };
    s.app_home
        .native
        .datetimes
        .iter()
        .filter(|((f, key), w)| {
            f == &o
                && (survives(s, key) || protects_buffer(s, f, key, w))
                && w.initialized
                && w.visit == s.app_home.surface.as_ref().unwrap().id
                && w.binding.as_ref().is_some_and(|b| {
                    b.owner.current(s) && form(s).is_some_and(|f| f.raw_view() == &b.raw)
                })
        })
        .map(|(_, w)| w.parent.clone())
        .collect()
}
// Only an absent/replaced pristine field may release its initialized token.
// Received draft state keeps the token so core refuses removal atomically.
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
                    && b["element"]["type"] == "datetimepicker"
            })
        })
}
fn protects_buffer(s: &ShellState, o: &FormOwner, key: &InputKey, w: &Widget) -> bool {
    w.date_raw_dirty
        || w.time_raw_dirty
        || !w.date_buffer_valid
        || s.app_home
            .native
            .entries
            .get(&(o.clone(), key.clone()))
            .is_none_or(|e| {
                e.pending
                    || e.invalid
                    || e.value != json!([w.date_raw, w.picker, w.time_raw]).to_string()
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
            let field = f.datetime_field(k).ok()?;
            let retained = s
                .app_home
                .native
                .datetimes
                .get(&(o.clone(), k.clone()))
                .filter(|_| f.native_error_text(k).is_some());
            let seed = f
                .datetime_seed(k)
                .ok()
                .or_else(|| retained.map(|w| w.visible.clone()));
            let parent = f
                .datetime_parent(k)
                .ok()
                .or_else(|| retained.map(|w| w.parent.clone()));
            Some((
                k.clone(),
                field.raw.get("initial_date_time").cloned(),
                seed,
                parent,
            ))
        })
        .collect::<Vec<_>>();
    s.app_home.native.datetimes.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || fields.iter().any(|(k, _, _, _)| k == key))
    });
    for (key, initial, seed, parent) in fields {
        let index = (o.clone(), key);
        if s.app_home
            .native
            .datetimes
            .get(&index)
            .is_some_and(|w| w.visit == visit)
        {
            // 7XR/ has no mounted prop synchronization effect. Both child drafts
            // and the selected epoch survive initial/saved source refresh.
            continue;
        }
        if s.app_home.native.datetimes.len() < 256
            || s.app_home.native.datetimes.contains_key(&index)
        {
            if let (Some(seed), Some(parent)) = (seed, parent) {
                s.app_home.native.datetimes.insert(
                    index,
                    Widget {
                        visit,
                        initial,
                        visible: seed,
                        parent,
                        binding: None,
                        initialized: false,
                        locale: String::new(),
                        time24: false,
                        local_invalid: false,
                        expanded: false,
                        date_raw: String::new(),
                        time_raw: String::new(),
                        picker: String::new(),
                        date_parsed: None,
                        date_focused: false,
                        date_raw_dirty: false,
                        time_raw_dirty: false,
                        date_buffer_valid: false,
                        date_resolved_raw: None,
                        date_resolved_selected: None,
                        picker_status: "idle".into(),
                    },
                );
            }
        }
    }
}
use super::time::preferences;
pub(super) fn configure(s: &mut ShellState, b: &Binding) -> Option<(i64, bool)> {
    let (locale, time24) = preferences(s);
    let optional = form(s)?.datetime_field(&b.key).ok()?.optional;
    let w = s
        .app_home
        .native
        .datetimes
        .get_mut(&(b.form.clone(), b.key.clone()))?;
    if w.visit != b.visit.visit {
        return None;
    }
    w.binding = Some(b.clone());
    w.locale = locale;
    w.time24 = time24;
    Some((w.visible.clone(), optional))
}
pub(crate) fn mounted(s: &ShellState, b: &Binding, mount: u64) -> bool {
    form(s).is_some_and(|f| f.datetime_field(&b.key).is_ok())
        && b.current(s)
        && s.app_home
            .native
            .entries
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|e| e.mount == mount)
        && s.app_home
            .native
            .datetimes
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|w| {
                w.initialized
                    && w.visit == b.visit.visit
                    && (w.locale.clone(), w.time24) == preferences(s)
                    && w.binding.as_ref().is_some_and(|b| b.current(s))
            })
}
pub(super) fn settled(s: &ShellState, o: &FormOwner, key: &InputKey, _e: &Entry) -> bool {
    s.app_home
        .native
        .datetimes
        .get(&(o.clone(), key.clone()))
        // Acknowledged valid draft input does not commit or cancel the earlier
        // time action. The final JS lease still checks its exact native buffer.
        .is_some_and(|w| {
            w.initialized
                && w.date_buffer_valid
                && (w.locale.clone(), w.time24) == preferences(s)
                && w.binding.as_ref().is_some_and(|b| b.current(s))
        })
}
#[derive(Deserialize, Serialize)]
pub(super) struct Event {
    mount: u64,
    seq: u64,
    kind: String,
    value: String,
    valid: bool,
    bad: bool,
    selected: Option<i64>,
    date: String,
    time: String,
    picker: String,
    date_parsed: Option<String>,
    date_focused: bool,
    date_raw_dirty: bool,
    time_raw_dirty: bool,
    date_buffer_valid: bool,
    date_resolved_raw: Option<String>,
    date_resolved_selected: Option<String>,
    picker_status: String,
    child: String,
    caption: String,
    local_invalid: bool,
    expanded: bool,
}
pub(super) fn receive(
    s: &mut ShellState,
    b: &Binding,
    mount: u64,
    event: &Event,
) -> Option<StaticEvent> {
    if b.form.epoch != s.core.account_epoch
        || event.mount != mount
        || event.value.len() > 8192
        || event.date.len() > 4096
        || event.picker.len() > 128
        || event.time.len() > 4096
        || serde_json::from_str::<Vec<String>>(&event.value)
            .ok()
            .as_deref()
            != Some(&[event.date.clone(), event.picker.clone(), event.time.clone()])
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
    e.invalid = !event.valid || event.bad || !event.date_buffer_valid;
    if !b.current(s) || event.kind == "closed" {
        return None;
    }
    let current_prefs = preferences(s);
    let w = s.app_home.native.datetimes.get_mut(&index)?;
    if (w.locale.clone(), w.time24) != current_prefs {
        return None;
    }
    if event.kind == "mounted" {
        w.initialized = true;
    }
    w.local_invalid = event.local_invalid;
    w.expanded = event.expanded;
    w.date_raw = event.date.clone();
    w.time_raw = event.time.clone();
    w.picker = event.picker.clone();
    w.date_parsed = event.date_parsed.clone();
    w.date_focused = event.date_focused;
    w.date_raw_dirty = event.date_raw_dirty;
    w.time_raw_dirty = event.time_raw_dirty;
    w.date_buffer_valid = event.date_buffer_valid;
    w.date_resolved_raw = event.date_resolved_raw.clone();
    w.date_resolved_selected = event.date_resolved_selected.clone();
    w.picker_status = event.picker_status.clone();
    if !event.valid || event.bad {
        let _ = s.app_home.forms.get_mut(&b.form)?.set_native_error(
            &b.key,
            event.value.clone(),
            "This date and time input is not confirmed. Finish editing or reset the inputs.".into(),
        );
        bump(s, &b.form);
        return None;
    }
    // A received Time callback remains a real parent edit even if the other
    // child has an unresolved Date draft. Admission stays blocked by e.invalid;
    // do not silently desynchronize the composite's selected epoch from core.
    if !event.date_buffer_valid && !(event.kind == "commit" && event.child == "time") {
        return None;
    }
    let f = s.app_home.forms.get_mut(&b.form)?;
    let had_error = f.native_error_text(&b.key).is_some();
    // Parsed canonical value is used only to retire a prior native safety marker.
    // Invalid-but-observed text is local widget state, never parent validation.
    f.datetime_acknowledge_native_buffer(&b.key, event.selected)
        .ok()?;
    if had_error {
        bump(s, &b.form);
    }
    if event.kind != "commit" || !b.interactive(s) {
        return None;
    }
    if event.selected.is_none() && !form(s)?.datetime_field(&b.key).ok()?.optional {
        return None;
    }
    let action = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .datetime_input(&b.key, event.selected)
        .ok()?;
    s.app_home.native.datetimes.get_mut(&index)?.visible = event.selected.unwrap_or(0);
    crate::app_home::inputs::static_select::fixture::event(s, event, &b.key);
    bump(s, &b.form);
    action
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let o = owner(s);
    json!({"datetimeWidgets":o.as_ref().map(|o|s.app_home.native.datetimes.iter().filter(|((f,_),_)|f==o).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"initial":w.initial,"visible":w.visible,"initialized":w.initialized,"localInvalid":w.local_invalid,"expanded":w.expanded,"locale":w.locale,"time24":w.time24,"dateRaw":w.date_raw,"timeRaw":w.time_raw,"picker":w.picker,"dateParsed":w.date_parsed,"dateFocused":w.date_focused,"dateRawDirty":w.date_raw_dirty,"timeRawDirty":w.time_raw_dirty,"dateBufferValid":w.date_buffer_valid,"dateResolvedRaw":w.date_resolved_raw,"dateResolvedSelected":w.date_resolved_selected,"pickerStatus":w.picker_status,"current":w.binding.as_ref().is_some_and(|b|mounted(s,b,s.app_home.native.entries.get(&(o.clone(),k.clone())).map_or(0,|e|e.mount)))})).collect::<Vec<_>>()),
    "nativeMounts":o.as_ref().map(|o|s.app_home.native.entries.iter().filter(|((f,_),_)|f==o).map(|((_,k),e)|json!({"block":k.block_id,"action":k.action_id,"id":e.id,"mount":e.mount,"seq":e.seq,"value":e.value,"pending":e.pending,"invalid":e.invalid})).collect::<Vec<_>>())})
}

// Test-only received-state setup; fixtures must mount real acknowledged children.
#[cfg(test)]
pub(super) fn seed_value(s: &mut ShellState, key: &InputKey) {
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
    let (locale, time24) = preferences(s);
    if let Some(w) = s.app_home.native.datetimes.get_mut(&(o, key.clone())) {
        w.binding = Some(b);
        w.initialized = true;
        w.date_buffer_valid = true;
        w.locale = locale;
        w.time24 = time24;
    }
}
