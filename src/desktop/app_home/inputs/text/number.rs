//! Number callback state is independent of the native spelling and local display.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::home::{NumberParent, NumberTrigger};
#[cfg(test)]
mod tests;

pub(super) struct Widget {
    visit: u64,
    local: Option<String>,
    parent: NumberParent,
    binding: Option<Binding>,
    initialized: bool,
    bad_input: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NumericOccurrence {
    pub local: Option<String>,
    pub callback: bool,
    pub callback_value: Option<String>,
    pub bad_input: bool,
    pub composing: bool,
    pub valid: bool,
    pub step_mismatch: bool,
    pub range_underflow: bool,
    pub range_overflow: bool,
}
pub(crate) fn supported(s: &ShellState, key: &InputKey) -> bool {
    form(s).is_some_and(|f| f.number_field(key).is_ok())
}
pub(crate) fn parents(s: &ShellState) -> Vec<NumberParent> {
    let Some(o) = owner(s) else { return vec![] };
    s.app_home
        .text
        .numbers
        .iter()
        .filter(|((f, _), w)| {
            f == &o
                && w.initialized
                && w.visit == s.app_home.surface.as_ref().unwrap().id
                && w.binding.as_ref().is_some_and(|b| {
                    b.owner.current(s) && form(s).is_some_and(|f| f.raw_view() == &b.raw)
                })
        })
        .map(|(_, w)| w.parent.clone())
        .collect()
}
pub(crate) fn hydrate(s: &mut ShellState) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let rows = f
        .fields
        .keys()
        .filter_map(|k| {
            f.number_field(k).ok()?;
            Some((k.clone(), f.number_seed(k).ok()?, f.number_parent(k).ok()?))
        })
        .collect::<Vec<_>>();
    s.app_home.text.numbers.retain(|(owner, key), _| {
        s.app_home.forms.contains_key(owner)
            && (owner != &o || rows.iter().any(|(k, _, _)| k == key))
    });
    for (key, local, parent) in rows {
        let index = (o.clone(), key);
        if s.app_home
            .text
            .numbers
            .get(&index)
            .is_some_and(|w| w.visit == visit)
        {
            continue;
        }
        if s.app_home.text.numbers.len() >= 256 && !s.app_home.text.numbers.contains_key(&index) {
            continue;
        }
        s.app_home.text.numbers.insert(
            index,
            Widget {
                visit,
                local,
                parent,
                binding: None,
                initialized: false,
                bad_input: false,
            },
        );
    }
}
pub(crate) fn reset(s: &mut ShellState, o: &FormOwner) {
    let keys = s
        .app_home
        .text
        .numbers
        .keys()
        .filter(|(f, _)| f == o)
        .cloned()
        .collect::<Vec<_>>();
    for index in keys {
        s.app_home.text.numbers.remove(&index);
        s.app_home.text.mounts.remove(&index);
        s.app_home.text.queued.remove(&index);
    }
    s.app_home.text.activity.retain(|(f, _), _| f != o);
}
pub(super) fn configure(s: &mut ShellState, b: &Binding) -> Option<Value> {
    let f = form(s)?;
    let field = f.number_field(&b.key).ok()?;
    let InputKind::Number {
        is_decimal_allowed,
        min_value,
        max_value,
    } = &field.kind
    else {
        return None;
    };
    let mut config = json!({"decimal":is_decimal_allowed,"min":min_value,"max":max_value,
        "dispatch":f.dispatch(&b.key)!=Some(Dispatch::Quiet),
        "enter":matches!(f.dispatch(&b.key),Some(Dispatch::Enter|Dispatch::EnterOrCharacter)),
        "character":matches!(f.dispatch(&b.key),Some(Dispatch::Character|Dispatch::EnterOrCharacter)),
        "placeholder":field.placeholder.as_deref().filter(|v|!v.is_empty()).unwrap_or("Enter a number")});
    let w = s
        .app_home
        .text
        .numbers
        .get_mut(&(b.form.clone(), b.key.clone()))?;
    if w.visit != b.visit.visit {
        return None;
    }
    w.binding = Some(b.clone());
    config["local"] = json!(w.local);
    Some(config)
}
pub(super) fn settled(s: &ShellState, key: &InputKey, m: &Mount) -> bool {
    s.app_home
        .text
        .numbers
        .get(&(m.binding.form.clone(), key.clone()))
        .is_some_and(|w| {
            w.initialized
                && !w.bad_input
                && w.visit == m.binding.visit.visit
                && w.binding.as_ref().is_some_and(|b| b.current(s))
        })
}
pub(crate) fn error(s: &ShellState, key: &InputKey) -> Option<String> {
    let w = s.app_home.text.numbers.get(&(owner(s)?, key.clone()))?;
    (w.bad_input && w.binding.as_ref().is_some_and(|b| b.current(s))).then(||
        "This native number is not confirmed. Complete the number or reset the inputs before sending an app action.".into())
}
pub(super) fn receive(s: &mut ShellState, b: &Binding, e: &Occurrence) -> Option<u64> {
    let n = e.number.as_ref()?;
    if !matches!(
        e.kind.as_str(),
        "mounted" | "input" | "enter" | "blur" | "closed" | "composition"
    ) || n.local.as_ref().is_some_and(|v| v.len() > 4096)
        || n.callback_value.as_ref().is_some_and(|v| v.len() > 4096)
    {
        return None;
    }
    let w = s
        .app_home
        .text
        .numbers
        .get_mut(&(b.form.clone(), b.key.clone()))?;
    if w.visit != b.visit.visit {
        return None;
    }
    w.local = n.local.clone();
    w.initialized = e.kind != "closed";
    w.bad_input = n.bad_input || n.composing;
    if !n.callback || n.bad_input || n.composing || !b.interactive(s) {
        return None;
    }
    let trigger = match e.kind.as_str() {
        "input" => NumberTrigger::Change,
        "enter" if e.key.as_deref() == Some("Enter") && e.prevented => NumberTrigger::Enter,
        "blur" => NumberTrigger::Blur,
        _ => return None,
    };
    let event = s
        .app_home
        .forms
        .get_mut(&b.form)?
        .number_input(&b.key, n.callback_value.clone(), trigger)
        .ok()?;
    bump(s, &b.form);
    super::super::number_fixture::event(s, &json!(e), &b.key);
    queue(s, b, e.mount, event?)
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let Some(o) = owner(s) else {
        return json!({"numberWidgets":null,"numberMounts":null});
    };
    let f = form(s);
    let state = f.and_then(|f| f.state().ok());
    json!({"numberWidgets":s.app_home.text.numbers.iter().filter(|((owner,_),_)|owner==&o).map(|((_,k),w)|
        json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"local":w.local,
        "parent":state.as_ref().and_then(|v|v["values"][&k.block_id].get(&k.action_id)),
        "initialized":w.initialized,"badInput":w.bad_input,"current":w.binding.as_ref().is_some_and(|b|b.current(s))})).collect::<Vec<_>>(),
        "numberMounts":s.app_home.text.mounts.iter().filter(|((owner,k),_)|owner==&o&&supported(s,k)).map(|((_,k),m)|
        json!({"block":k.block_id,"action":k.action_id,"mount":m.id,"seq":m.seq,"value":m.value,"current":m.binding.current(s)})).collect::<Vec<_>>()})
}
