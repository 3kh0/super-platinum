//! First-owned URL semantic local and parent wire remain separate from the native node.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::home::{UrlMount, UrlParent, UrlTrigger};
mod cadence;
pub(crate) mod fixture;
mod native;
pub(crate) use native::Control;
#[cfg(test)]
mod tests;

pub(super) struct Widget {
    visit: u64,
    local: String,
    parent: UrlParent,
    new_parent: bool,
    mounted: bool,
    initialized: bool,
    composing: bool,
    bad_input: bool,
    binding: Option<Binding>,
    window: Option<cadence::Window>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UrlOccurrence {
    pub mount: u64,
    pub seq: u64,
    pub kind: String,
    pub value: String,
    pub key: Option<String>,
    pub prevented: bool,
    pub local: String,
    pub callback: bool,
    pub callback_value: String,
    pub valid: bool,
    pub bad_input: bool,
    pub composing: bool,
}
pub(crate) fn supported(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| f.url_field(key).is_ok())
}
pub(crate) fn keys(s: &ShellState) -> Vec<InputKey> {
    form(s)
        .map(|f| {
            f.fields
                .keys()
                .filter(|k| f.url_field(k).is_ok())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}
pub(crate) fn parents(s: &ShellState) -> Vec<UrlParent> {
    let Some(o) = owner(s) else { return vec![] };
    s.app_home
        .text
        .urls
        .iter()
        .filter(|((f, _), w)| {
            f == &o
                && w.mounted
                && w.initialized
                && !w.bad_input
                && !w.composing
                && w.visit == s.app_home.surface.as_ref().unwrap().id
                && w.binding.as_ref().is_some_and(|b| {
                    b.owner.current(s) && form(s).is_some_and(|f| f.raw_view() == &b.raw)
                })
        })
        .map(|(_, w)| w.parent.clone())
        .collect()
}
/// The caller supplies actual new-parent/new-key/Reset identity, not a value heuristic.
pub(crate) fn hydrate(s: &mut ShellState, new_keys: &[InputKey]) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let active_keys = f
        .fields
        .keys()
        .filter(|k| f.url_field(k).is_ok())
        .cloned()
        .collect::<Vec<_>>();
    let rows = f
        .fields
        .keys()
        .filter_map(|k| Some((k.clone(), f.url_seed(k).ok()?, f.url_parent(k).ok()?)))
        .collect::<Vec<_>>();
    s.app_home.text.urls.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner) && (owner != &o || active_keys.contains(key))
    });
    for (key, local, parent) in rows {
        let index = (o.clone(), key.clone());
        if s.app_home
            .text
            .urls
            .get(&index)
            .is_some_and(|w| w.visit == visit)
        {
            continue;
        }
        if s.app_home.text.urls.len() >= 256 && !s.app_home.text.urls.contains_key(&index) {
            continue;
        }
        s.app_home.text.urls.insert(
            index,
            Widget {
                visit,
                local,
                parent,
                new_parent: new_keys.contains(&key),
                mounted: false,
                initialized: false,
                composing: false,
                bad_input: false,
                binding: None,
                window: None,
            },
        );
    }
}
pub(crate) fn reset(s: &mut ShellState, o: &FormOwner) {
    let keys = s
        .app_home
        .text
        .urls
        .keys()
        .filter(|(f, _)| f == o)
        .cloned()
        .collect::<Vec<_>>();
    for k in keys {
        s.app_home.text.urls.remove(&k);
        s.app_home.text.mounts.remove(&k);
        s.app_home.text.queued.remove(&k);
    }
    s.app_home.text.activity.retain(|(f, _), _| f != o);
}
pub(super) fn configure(s: &mut ShellState, b: &Binding) -> Option<Value> {
    let f = form(s)?;
    let field = f.url_field(&b.key).ok()?;
    let mode = f.dispatch(&b.key)?;
    // Trigger flags are source config, independent of quiet dispatch_action.
    let triggers = field.raw["dispatch_action_config"]["trigger_actions_on"].as_array();
    let enabled = |name: &str| triggers.is_some_and(|a| a.iter().any(|v| v == name));
    let mut config = json!({"enter":enabled("on_enter_pressed"),"character":enabled("on_character_entered"),"dispatch":mode!=Dispatch::Quiet});
    let w = s
        .app_home
        .text
        .urls
        .get_mut(&(b.form.clone(), b.key.clone()))?;
    if w.visit != b.visit.visit {
        return None;
    }
    if (w.bad_input || w.composing) && !w.initialized {
        return None;
    }
    w.binding = Some(b.clone());
    config["local"] = json!(w.local);
    Some(config)
}
pub(crate) fn loading(s: &ShellState, key: &InputKey) -> bool {
    owner(s)
        .and_then(|o| s.app_home.text.activity.get(&(o, key.block_id.clone())))
        .is_some_and(|a| a.loading && a.binding.current(s) && a.route.current(s))
}
pub(super) fn settled(s: &ShellState, key: &InputKey, m: &Mount) -> bool {
    s.app_home
        .text
        .urls
        .get(&(m.binding.form.clone(), key.clone()))
        .is_some_and(|w| {
            w.initialized
                && form(s).is_some_and(|f| f.native_error_text(key).is_none())
                && !w.composing
                && !w.bad_input
                && w.visit == m.binding.visit.visit
                && w.binding.as_ref().is_some_and(|b| b.current(s))
        })
}
pub(super) fn receive(s: &mut ShellState, b: &Binding, e: &UrlOccurrence) -> Option<u64> {
    if !b.current(s)
        || e.value.len() > 4 * 1024 * 1024
        || e.local.len() > 4 * 1024 * 1024
        || e.callback_value.len() > 4 * 1024 * 1024
        || !matches!(
            e.kind.as_str(),
            "mounted" | "closed" | "input" | "enter" | "composition"
        )
    {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let m = s.app_home.text.mounts.get_mut(&index)?;
    if m.id != e.mount || m.seq.checked_add(1) != Some(e.seq) || e.seq > 9_007_199_254_740_991 {
        return None;
    }
    m.seq = e.seq;
    m.value = e.value.clone();
    let w = s.app_home.text.urls.get_mut(&index)?;
    if w.visit != b.visit.visit {
        return None;
    }
    if e.kind == "closed" {
        w.initialized = false;
        w.window = None;
        s.app_home.text.mounts.remove(&index);
        return None;
    }
    if e.kind == "mounted" {
        if w.bad_input
            || w.composing
            || e.bad_input
            || e.composing
            || e.callback
            || e.local != w.local
        {
            return None;
        }
        if !w.mounted {
            let mode = if w.new_parent {
                UrlMount::NewParent
            } else {
                UrlMount::RetainedParent
            };
            s.app_home
                .forms
                .get_mut(&b.form)?
                .url_mount(&b.key, mode, e.valid)
                .ok()?;
        }
        let w = s.app_home.text.urls.get_mut(&index)?;
        w.mounted = true;
        w.initialized = true;
        bump(s, &b.form);
        return None;
    }
    w.composing = e.composing;
    w.bad_input = e.bad_input;
    if e.bad_input || e.composing {
        s.app_home
            .forms
            .get_mut(&b.form)?
            .url_mark_native_buffer(&b.key)
            .ok()?;
        bump(s, &b.form);
        return None;
    }
    let initialized = w.initialized;
    if !initialized
        || !e.callback
        || e.callback_value != e.local
        || e.local != e.value
        || !b.interactive(s)
    {
        return None;
    }
    s.app_home.text.urls.get_mut(&index)?.local = e.local.clone();
    let trigger = match e.kind.as_str() {
        "input" => UrlTrigger::Change,
        "enter" if e.key.as_deref() == Some("Enter") && e.prevented => UrlTrigger::Enter,
        _ => return None,
    };
    let f = s.app_home.forms.get_mut(&b.form)?;
    f.url_acknowledge_native_buffer(&b.key).ok()?;
    let event = f
        .url_input(&b.key, e.callback_value.clone(), e.valid, trigger)
        .ok()?;
    bump(s, &b.form);
    cadence::enqueue(s, b, e.mount, event?)
}
pub(super) fn poll(s: &mut ShellState) -> bool {
    let stale = s
        .app_home
        .text
        .urls
        .iter()
        .filter(|(_, w)| w.window.as_ref().is_some_and(|v| !v.current(s)))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in &stale {
        s.app_home.text.urls.get_mut(k).unwrap().window = None;
    }
    !stale.is_empty()
}
pub(super) use cadence::launch;
pub(crate) fn proof(s: &ShellState) -> Value {
    let Some(o) = owner(s) else {
        return json!({"urlWidgets":null,"urlMounts":null,"urlWindows":null});
    };
    let parent = form(s).and_then(|f| f.state().ok());
    json!({"urlWidgets":s.app_home.text.urls.iter().filter(|((f,_),_)|f==&o).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"local":w.local,"parent":parent.as_ref().and_then(|p|p["values"][&k.block_id].get(&k.action_id)),"initialized":w.initialized,"mounted":w.mounted,"newParent":w.new_parent,"composing":w.composing,"current":w.binding.as_ref().is_some_and(|b|b.current(s)),"loading":loading(s,k)})).collect::<Vec<_>>(),
        "urlMounts":s.app_home.text.mounts.iter().filter(|((f,k),_)|f==&o&&supported(s,k)).map(|((_,k),m)|json!({"block":k.block_id,"action":k.action_id,"mount":m.id,"seq":m.seq,"value":m.value,"current":m.binding.current(s)})).collect::<Vec<_>>(),
        "urlWindows":s.app_home.text.urls.iter().filter(|((f,_),_)|f==&o).filter_map(|((_,k),w)|w.window.as_ref().map(|v|json!({"block":k.block_id,"action":k.action_id,"id":v.id,"current":v.current(s),"leadingId":v.leading.as_ref().map(|q|q.id),"trailingId":v.trailing.as_ref().map(|q|q.id)}))).collect::<Vec<_>>()})
}
