//! Home time display, native edit buffer and committed parent are separate.
use super::*;
use crate::app_home::inputs::static_select::Binding;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::home::{StaticEvent, TimeParent};
mod control;
pub(crate) mod fixture;
pub(crate) mod focus_fixture;
#[cfg(test)]
mod tests;
pub(crate) use control::Control;

pub(super) struct Widget {
    visit: u64,
    initial: Option<Value>,
    visible: Option<String>,
    parent: TimeParent,
    binding: Option<Binding>,
    initialized: bool,
    locale: String,
    time24: bool,
    local_invalid: bool,
    expanded: bool,
    raw_dirty: bool,
    raw: String,
}

pub(crate) fn active(s: &ShellState) -> bool {
    form(s).is_some_and(|f| f.fields.keys().any(|k| f.time_field(k).is_ok()))
}
pub(crate) fn parents(s: &ShellState) -> Vec<TimeParent> {
    let Some(o) = owner(s) else { return vec![] };
    s.app_home
        .native
        .times
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
                    && b["element"]["type"] == "timepicker"
            })
        })
}
fn protects_buffer(s: &ShellState, o: &FormOwner, key: &InputKey, w: &Widget) -> bool {
    w.raw_dirty
        || s.app_home
            .native
            .entries
            .get(&(o.clone(), key.clone()))
            .is_none_or(|e| e.pending || e.invalid || e.value != w.raw)
}
pub(crate) fn hydrate(s: &mut ShellState) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let fields = f
        .fields
        .keys()
        .filter_map(|k| {
            let field = f.time_field(k).ok()?;
            let retained = s
                .app_home
                .native
                .times
                .get(&(o.clone(), k.clone()))
                .filter(|_| f.native_error_text(k).is_some());
            let seed = f
                .time_seed(k)
                .ok()
                .or_else(|| retained.map(|w| w.visible.clone()));
            let parent = f
                .time_parent(k)
                .ok()
                .or_else(|| retained.map(|w| w.parent.clone()));
            Some((
                k.clone(),
                field.raw.get("initial_time").cloned(),
                seed,
                parent,
            ))
        })
        .collect::<Vec<_>>();
    s.app_home.native.times.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || fields.iter().any(|(k, _, _, _)| k == key))
    });
    for (key, initial, seed, parent) in fields {
        let index = (o.clone(), key);
        if let Some(w) = s
            .app_home
            .native
            .times
            .get_mut(&index)
            .filter(|w| w.visit == visit)
        {
            if w.initial != initial {
                w.visible = initial.as_ref().and_then(Value::as_str).map(str::to_owned);
                w.initial = initial;
            }
        } else if s.app_home.native.times.len() < 256
            || s.app_home.native.times.contains_key(&index)
        {
            if let (Some(seed), Some(parent)) = (seed, parent) {
                s.app_home.native.times.insert(
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
                        raw_dirty: false,
                        raw: String::new(),
                    },
                );
            }
        }
    }
}
pub(super) fn preferences(s: &ShellState) -> (String, bool) {
    let w = s
        .core
        .active_team
        .as_ref()
        .and_then(|id| s.core.workspaces.get(id));
    (
        // Retained Gantry provider validates configured user_locale before the
        // formatter. Native has no page query/HTML locale source: invalid or
        // missing configured values therefore take the provider's en-US default.
        w.and_then(|w| w.locale.as_deref())
            .filter(|locale| supported_locale(locale))
            .unwrap_or("en-US")
            .to_owned(),
        w.and_then(|w| w.notifications.user.get("time24"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}
pub(super) fn supported_locale(locale: &str) -> bool {
    matches!(
        locale,
        "en-US"
            | "en-GB"
            | "de-DE"
            | "es-ES"
            | "es-LA"
            | "fr-FR"
            | "it-IT"
            | "ja-JP"
            | "ko-KR"
            | "pt-BR"
            | "zh-CN"
            | "zh-TW"
    )
}
pub(super) fn configure(s: &mut ShellState, b: &Binding) -> Option<(Option<String>, bool)> {
    let (locale, time24) = preferences(s);
    let optional = form(s)?.time_field(&b.key).ok()?.optional;
    let w = s
        .app_home
        .native
        .times
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
    form(s).is_some_and(|f| f.time_field(&b.key).is_ok())
        && b.current(s)
        && s.app_home
            .native
            .entries
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|e| e.mount == mount)
        && s.app_home
            .native
            .times
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|w| {
                w.visit == b.visit.visit
                    && (w.locale.clone(), w.time24) == preferences(s)
                    && w.binding.as_ref().is_some_and(|b| b.current(s))
            })
}
pub(super) fn settled(s: &ShellState, o: &FormOwner, key: &InputKey, _e: &Entry) -> bool {
    s.app_home
        .native
        .times
        .get(&(o.clone(), key.clone()))
        // Acknowledged valid draft input does not commit or cancel the earlier
        // time action. The final JS lease still checks its exact native buffer.
        .is_some_and(|w| {
            (w.locale.clone(), w.time24) == preferences(s)
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
    selected: Option<String>,
    parsed: Option<String>,
    local_invalid: bool,
    expanded: bool,
    raw_dirty: bool,
}
pub(super) fn receive(
    s: &mut ShellState,
    b: &Binding,
    mount: u64,
    event: &Event,
) -> Option<StaticEvent> {
    if b.form.epoch != s.core.account_epoch
        || event.mount != mount
        || event.value.len() > 4096
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
    e.invalid = !event.valid || event.bad;
    if !b.current(s) || event.kind == "closed" {
        return None;
    }
    let current_prefs = preferences(s);
    let w = s.app_home.native.times.get_mut(&index)?;
    if (w.locale.clone(), w.time24) != current_prefs {
        return None;
    }
    if event.kind == "mounted" {
        w.initialized = true;
    }
    w.local_invalid = event.local_invalid;
    w.expanded = event.expanded;
    w.raw_dirty = event.raw_dirty;
    w.raw = event.value.clone();
    if !event.valid || event.bad {
        let _ = s.app_home.forms.get_mut(&b.form)?.set_native_error(
            &b.key,
            event.value.clone(),
            "This time input is not confirmed. Finish editing or reset the inputs.".into(),
        );
        bump(s, &b.form);
        return None;
    }
    let f = s.app_home.forms.get_mut(&b.form)?;
    let had_error = f.native_error_text(&b.key).is_some();
    // Parsed canonical value is used only to retire a prior native safety marker.
    // Invalid-but-observed text is local widget state, never parent validation.
    f.time_acknowledge_native_buffer(&b.key, event.parsed.as_deref())
        .ok()?;
    if had_error {
        bump(s, &b.form);
    }
    if event.kind != "commit" || !b.interactive(s) {
        return None;
    }
    if event.selected.is_none() && !form(s)?.time_field(&b.key).ok()?.optional {
        return None;
    }
    let action = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .time_input(&b.key, event.selected.clone())
        .ok()?;
    s.app_home.native.times.get_mut(&index)?.visible = event.selected.clone();
    crate::app_home::inputs::static_select::fixture::event(s, event, &b.key);
    bump(s, &b.form);
    action
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let o = owner(s);
    json!({"timeWidgets":o.as_ref().map(|o|s.app_home.native.times.iter().filter(|((f,_),_)|f==o).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"initial":w.initial,"visible":w.visible,"initialized":w.initialized,"localInvalid":w.local_invalid,"expanded":w.expanded,"rawDirty":w.raw_dirty,"locale":w.locale,"time24":w.time24,"current":w.binding.as_ref().is_some_and(|b|mounted(s,b,s.app_home.native.entries.get(&(o.clone(),k.clone())).map_or(0,|e|e.mount)))})).collect::<Vec<_>>()),
    "nativeMounts":o.as_ref().map(|o|s.app_home.native.entries.iter().filter(|((f,_),_)|f==o).map(|((_,k),e)|json!({"block":k.block_id,"action":k.action_id,"id":e.id,"mount":e.mount,"seq":e.seq,"value":e.value,"pending":e.pending,"invalid":e.invalid})).collect::<Vec<_>>())})
}

// Serial test setup only; production fixtures still need real mounted receipts.
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
    let (locale, time24) = preferences(s);
    if let Some(w) = s.app_home.native.times.get_mut(&(o, key.clone())) {
        w.visible = (!value.is_empty()).then(|| value.to_owned());
        w.binding = Some(b);
        w.initialized = true;
        w.locale = locale;
        w.time24 = time24;
    }
}
