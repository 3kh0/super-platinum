//! Email local text and mounted-parent effects are distinct from callback wire.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::home::{EmailMount, EmailParent, EmailTrigger};
mod cadence;
#[cfg(test)]
mod tests;

pub(super) struct Widget {
    visit: u64,
    local: String,
    parent: EmailParent,
    new_parent: bool,
    mounted: bool,
    initialized: bool,
    composing: bool,
    binding: Option<Binding>,
    window: Option<cadence::Window>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EmailOccurrence {
    pub local: String,
    pub callback: bool,
    pub callback_value: String,
    pub enter: bool,
    pub character: bool,
    pub type_mismatch: bool,
    pub composing: bool,
}
pub(crate) fn supported(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| f.email_field(key).is_ok())
}
pub(crate) fn keys(s: &ShellState) -> Vec<InputKey> {
    form(s)
        .map(|f| {
            f.fields
                .keys()
                .filter(|k| f.email_field(k).is_ok())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}
pub(crate) fn parents(s: &ShellState) -> Vec<EmailParent> {
    let Some(o) = owner(s) else { return vec![] };
    s.app_home
        .text
        .emails
        .iter()
        .filter(|((f, _), w)| {
            f == &o
                && w.mounted
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
    let rows = f
        .fields
        .keys()
        .filter_map(|k| Some((k.clone(), f.email_seed(k).ok()?, f.email_parent(k).ok()?)))
        .collect::<Vec<_>>();
    s.app_home.text.emails.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || rows.iter().any(|(k, _, _)| k == key))
    });
    for (key, local, parent) in rows {
        let index = (o.clone(), key.clone());
        if s.app_home
            .text
            .emails
            .get(&index)
            .is_some_and(|w| w.visit == visit)
        {
            continue;
        }
        if s.app_home.text.emails.len() >= 256 && !s.app_home.text.emails.contains_key(&index) {
            continue;
        }
        s.app_home.text.emails.insert(
            index,
            Widget {
                visit,
                local,
                parent,
                new_parent: new_keys.contains(&key),
                mounted: false,
                initialized: false,
                composing: false,
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
        .emails
        .keys()
        .filter(|(f, _)| f == o)
        .cloned()
        .collect::<Vec<_>>();
    for k in keys {
        s.app_home.text.emails.remove(&k);
        s.app_home.text.mounts.remove(&k);
        s.app_home.text.queued.remove(&k);
    }
    s.app_home.text.activity.retain(|(f, _), _| f != o);
}
pub(super) fn configure(s: &mut ShellState, b: &Binding) -> Option<Value> {
    let f = form(s)?;
    let field = f.email_field(&b.key).ok()?;
    let mode = f.dispatch(&b.key)?;
    // Trigger flags are source config, independent of quiet dispatch_action.
    let triggers = field.raw["dispatch_action_config"]["trigger_actions_on"].as_array();
    let enabled = |name: &str| triggers.is_some_and(|a| a.iter().any(|v| v == name));
    let mut config = json!({"enter":enabled("on_enter_pressed"),"character":enabled("on_character_entered"),"dispatch":mode!=Dispatch::Quiet});
    let w = s
        .app_home
        .text
        .emails
        .get_mut(&(b.form.clone(), b.key.clone()))?;
    if w.visit != b.visit.visit {
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
        .emails
        .get(&(m.binding.form.clone(), key.clone()))
        .is_some_and(|w| {
            w.initialized
                && !w.composing
                && w.visit == m.binding.visit.visit
                && w.binding.as_ref().is_some_and(|b| b.current(s))
        })
}
pub(super) fn receive(s: &mut ShellState, b: &Binding, e: &Occurrence) -> Option<u64> {
    let n = e.email.as_ref()?;
    if n.local.len() > 4 * 1024 * 1024
        || n.callback_value.len() > 4 * 1024 * 1024
        || !matches!(
            e.kind.as_str(),
            "mounted" | "closed" | "input" | "enter" | "composition"
        )
    {
        return None;
    }
    let index = (b.form.clone(), b.key.clone());
    let w = s.app_home.text.emails.get_mut(&index)?;
    if w.visit != b.visit.visit {
        return None;
    }
    w.local = n.local.clone();
    w.initialized = e.kind != "closed" && (e.kind != "mounted" || w.mounted);
    w.composing = n.composing;
    if e.kind == "mounted" && !w.mounted {
        let mode = if w.new_parent {
            EmailMount::NewParent
        } else {
            EmailMount::RetainedParent
        };
        s.app_home
            .forms
            .get_mut(&b.form)?
            .email_mount(&b.key, mode)
            .ok()?;
        s.app_home.text.emails.get_mut(&index)?.mounted = true;
        s.app_home.text.emails.get_mut(&index)?.initialized = true;
        bump(s, &b.form);
        super::super::email_fixture::event(s, &json!(e), &b.key);
        return None;
    }
    if !n.callback || n.composing || !b.interactive(s) {
        return None;
    }
    let trigger = match e.kind.as_str() {
        "input" => EmailTrigger::Change,
        "enter" if e.key.as_deref() == Some("Enter") && e.prevented => EmailTrigger::Enter,
        _ => return None,
    };
    let event = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .email_input(&b.key, n.callback_value.clone(), trigger)
        .ok()?;
    bump(s, &b.form);
    super::super::email_fixture::event(s, &json!(e), &b.key);
    cadence::enqueue(s, b, e.mount, event?)
}
pub(super) fn poll(s: &mut ShellState) -> bool {
    let stale = s
        .app_home
        .text
        .emails
        .iter()
        .filter(|(_, w)| w.window.as_ref().is_some_and(|v| !v.current(s)))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for k in &stale {
        s.app_home.text.emails.get_mut(k).unwrap().window = None;
    }
    !stale.is_empty()
}
pub(super) use cadence::launch;
pub(crate) fn proof(s: &ShellState) -> Value {
    let Some(o) = owner(s) else {
        return json!({"emailWidgets":null,"emailMounts":null,"emailWindows":null});
    };
    let parent = form(s).and_then(|f| f.state().ok());
    json!({"emailWidgets":s.app_home.text.emails.iter().filter(|((f,_),_)|f==&o).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"local":w.local,"parent":parent.as_ref().and_then(|p|p["values"][&k.block_id].get(&k.action_id)),"initialized":w.initialized,"mounted":w.mounted,"newParent":w.new_parent,"composing":w.composing,"current":w.binding.as_ref().is_some_and(|b|b.current(s)),"loading":loading(s,k)})).collect::<Vec<_>>(),
        "emailMounts":s.app_home.text.mounts.iter().filter(|((f,k),_)|f==&o&&supported(s,k)).map(|((_,k),m)|json!({"block":k.block_id,"action":k.action_id,"mount":m.id,"seq":m.seq,"value":m.value,"current":m.binding.current(s)})).collect::<Vec<_>>(),
        "emailWindows":s.app_home.text.emails.iter().filter(|((f,_),_)|f==&o).filter_map(|((_,k),w)|w.window.as_ref().map(|v|json!({"block":k.block_id,"action":k.action_id,"id":v.id,"current":v.current(s),"leadingId":v.leading.as_ref().map(|q|q.id),"trailingId":v.trailing.as_ref().map(|q|q.id)}))).collect::<Vec<_>>()})
}
