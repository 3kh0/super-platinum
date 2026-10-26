//! User callbacks retain typed picker custody and share Home scheduling.
use super::*;
use crate::app_home::inputs::static_select::Binding;
use super_platinum_core::block_actions::form::home::{StaticEvent, UserParent};
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

pub(super) struct Event {
    pub(super) action: StaticEvent,
    pub(super) binding: Binding,
    pub(super) instance: u64,
}
pub(super) fn binding(s: &ShellState, key: &InputKey) -> Option<Binding> {
    let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
    let form = super::super::form(s)?;
    if !matches!(
        form.user_field(key).ok()?.kind,
        InputKind::User | InputKind::MultiUser { .. }
    ) {
        return None;
    }
    Some(Binding {
        owner: h.owner.clone(),
        form: super::super::owner(s)?,
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key.clone(),
        raw: form.raw_view().clone(),
    })
}
/// Interaction revision changes on each choice; this identity does not.
pub(crate) fn instance_current(s: &ShellState, b: &Binding, instance: u64) -> bool {
    if !b.current(s) {
        return false;
    }
    let Some(field) = super::super::form(s).and_then(|f| f.user_field(&b.key).ok()) else {
        return false;
    };
    let index = (b.form.clone(), b.key.clone());
    match field.kind {
        InputKind::User => s
            .app_home
            .users
            .widgets
            .get(&index)
            .is_some_and(|w| w.instance == instance && w.visit == b.visit.visit),
        InputKind::MultiUser { .. } => s
            .app_home
            .users
            .multiples
            .get(&index)
            .is_some_and(|w| w.instance == instance && w.visit == b.visit.visit),
        _ => false,
    }
}
pub(crate) fn disabled(s: &ShellState, key: &InputKey) -> bool {
    super::super::form(s).is_some_and(|f| {
        f.user_field(key)
            .is_ok_and(|f| matches!(f.kind, InputKind::User | InputKind::MultiUser { .. }))
    }) && super::super::static_select::loading(s, &key.block_id)
}
pub(super) fn schedule(state: Signal<ShellState>, event: Option<Event>) {
    if let Some(e) = event {
        super::super::static_select::typed_event(state, e.binding, e.instance, e.action);
    }
}
pub(crate) fn parents(s: &ShellState) -> Vec<UserParent> {
    let Some(o) = super::super::owner(s) else {
        return vec![];
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return vec![];
    };
    s.app_home
        .users
        .widgets
        .iter()
        .filter(|((owner, _), w)| {
            owner == &o
                && w.visit == h.id
                && w.binding.owner.current(s)
                && super::super::form(s).is_some_and(|f| f.raw_view() == &w.binding.raw)
        })
        .map(|(_, w)| w.parent.clone())
        .chain(
            s.app_home
                .users
                .multiples
                .iter()
                .filter(|((owner, _), w)| {
                    owner == &o
                        && w.visit == h.id
                        && w.binding.owner.current(s)
                        && super::super::form(s).is_some_and(|f| f.raw_view() == &w.binding.raw)
                })
                .map(|(_, w)| w.parent.clone()),
        )
        .collect()
}
/// Reset is the product's owner-wide form reset, not a reference UI callback.
pub(in crate::app_home::inputs) fn reset(s: &mut ShellState, owner: &FormOwner) {
    s.app_home.users.widgets.retain(|(o, _), _| o != owner);
    s.app_home.users.multiples.retain(|(o, _), _| o != owner);
    if s.app_home
        .users
        .picker
        .as_ref()
        .is_some_and(|p| &p.ticket.form == owner)
    {
        s.app_home.users.picker = None;
    }
}
/// Logical widget and parent proof; no browser mount or native sequence is claimed.
pub(crate) fn proof(s: &ShellState) -> Value {
    let Some(o) = super::super::owner(s) else {
        return json!({"userWidgets":null,"multiUserWidgets":null});
    };
    let parent = super::super::form(s).and_then(|f| f.state().ok());
    json!({"userWidgets":s.app_home.users.widgets.iter().filter(|((owner,_),_)|owner==&o)
        .map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,
            "instance":w.instance,"revision":w.revision,"initial":w.initial,"visible":w.visible,
            "parent":parent.as_ref().and_then(|v|v.get("values")).and_then(|v|v.get(&k.block_id)).and_then(|v|v.get(&k.action_id)),
            "current":instance_current(s,&w.binding,w.instance),"loading":disabled(s,k)}))
        .collect::<Vec<_>>(),
        "multiUserWidgets":s.app_home.users.multiples.iter().filter(|((owner,_),_)|owner==&o)
        .map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,
            "instance":w.instance,"revision":w.revision,"initial":w.initial,"visible":w.visible,
            "parent":parent.as_ref().and_then(|v|v.get("values")).and_then(|v|v.get(&k.block_id)).and_then(|v|v.get(&k.action_id)),
            "current":instance_current(s,&w.binding,w.instance),"loading":disabled(s,k),"invalid":w.invalid}))
        .collect::<Vec<_>>()})
}
