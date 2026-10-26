//! Focus history follows a logical Home field, not its revision-keyed DOM node.
//! Product Reset preserves this history; a new visit, removal or kind change
//! starts a new instance. This is an explicit adapter, not React reconciliation.
use super::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use super_platinum_core::block_actions::form::InputKind;

#[derive(Default)]
pub(super) struct State {
    widgets: BTreeMap<InputKey, Widget>,
    owner: Option<FormOwner>,
    visit: u64,
}
struct Widget {
    instance: u64,
    kind: &'static str,
    flag: bool,
}
fn kind(field: &super_platinum_core::block_actions::form::FormField) -> Option<&'static str> {
    match field.kind {
        InputKind::Text {
            multiline: true, ..
        } => Some("multiline"),
        InputKind::Text { .. } => Some("singleline"),
        InputKind::Number { .. } => Some("number"),
        InputKind::Email => Some("email"),
        InputKind::Date => Some("date"),
        InputKind::Time { .. } => Some("time"),
        _ => None,
    }
}
pub(super) fn hydrate(s: &mut ShellState) {
    let Some(o) = owner(s) else { return };
    let visit = s.app_home.surface.as_ref().unwrap().id;
    let Some(f) = form(s) else { return };
    let rows = f
        .fields
        .iter()
        .filter_map(|(key, field)| {
            Some((
                key.clone(),
                kind(field)?,
                field.raw["focus_on_load"] == true,
            ))
        })
        .collect::<Vec<_>>();
    let state = &mut s.app_home.text.focus;
    if state.owner.as_ref() != Some(&o) || state.visit != visit {
        state.widgets.clear();
    }
    state.owner = Some(o);
    state.visit = visit;
    state
        .widgets
        .retain(|key, _| rows.iter().any(|(k, _, _)| k == key));
    for (key, kind, flag) in rows {
        if let Some(w) = state.widgets.get_mut(&key).filter(|w| w.kind == kind) {
            w.flag = flag;
        } else if state.widgets.len() < 256 || state.widgets.contains_key(&key) {
            state.widgets.insert(
                key,
                Widget {
                    instance: next(),
                    kind,
                    flag,
                },
            );
        }
    }
}
fn current(s: &ShellState) -> bool {
    let state = &s.app_home.text.focus;
    state.owner.as_ref() == owner(s).as_ref()
        && s.app_home.surface.as_ref().is_some_and(|h| {
            h.owner.current(s)
                && h.id == state.visit
                && h.view
                    .as_ref()
                    .is_some_and(|v| form(s).is_some_and(|f| f.raw_view() == v.raw()))
        })
}
pub(crate) fn config(s: &ShellState, key: &InputKey) -> Option<Value> {
    if !current(s) {
        return None;
    }
    let state = &s.app_home.text.focus;
    let w = state.widgets.get(key)?;
    let h = s.app_home.surface.as_ref()?;
    Some(json!({"instance":w.instance,"kind":w.kind,"flag":w.flag,
        "visit":h.id,"revision":h.revision,"id":crate::view::app_modal::field_id(key),
        "live":state.widgets.values().map(|w|w.instance).collect::<Vec<_>>()}))
}
pub(crate) fn proof(s: &ShellState) -> Value {
    let state = &s.app_home.text.focus;
    json!({"focusWidgets":state.widgets.iter().map(|(key,w)|json!({
        "block":key.block_id,"action":key.action_id,"visit":state.visit,
        "instance":w.instance,"kind":w.kind,"flag":w.flag,"current":current(s)
    })).collect::<Vec<_>>()})
}
pub(crate) const SCRIPT: &str = include_str!("focus.js");

/// Quiet inputs retain their typed RSX callbacks. This attaches focus only.
pub(crate) fn mount_quiet(s: Signal<ShellState>, key: &InputKey) {
    let Some(c) = config(&s.peek(), key) else {
        return;
    };
    dioxus::document::eval(&format!(
        "{}\nwindow.spHomeFocus.attachQuiet({});",
        SCRIPT, c
    ));
}

#[cfg(test)]
mod tests;
