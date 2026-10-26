//! Home conversations keep typed callback custody separate from picker revisions.
use super::*;
use crate::app_home::inputs::{self, static_select::Binding};
use super_platinum_core::block_actions::form::home::ConversationParent;
#[cfg(test)]
mod tests;

pub(in crate::app_home::inputs::channels) struct Custody {
    pub(in crate::app_home::inputs::channels) instance: u64,
    pub(in crate::app_home::inputs::channels) parent: ConversationParent,
    pub(in crate::app_home::inputs::channels) binding: Binding,
}
pub(in crate::app_home::inputs::channels) fn binding(
    s: &ShellState,
    key: &InputKey,
) -> Option<Binding> {
    let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
    let form = inputs::form(s)?;
    if !matches!(
        form.conversation_field(key).ok()?.kind,
        InputKind::Conversation | InputKind::MultiConversation { .. }
    ) {
        return None;
    }
    Some(Binding {
        owner: h.owner.clone(),
        form: inputs::owner(s)?,
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key.clone(),
        raw: form.raw_view().clone(),
    })
}
pub(in crate::app_home::inputs::channels) fn instance_current(
    s: &ShellState,
    b: &Binding,
    instance: u64,
) -> bool {
    if !b.current(s) {
        return false;
    }
    let Some(field) = inputs::form(s).and_then(|f| f.conversation_field(&b.key).ok()) else {
        return false;
    };
    match field.kind {
        InputKind::Conversation => s
            .app_home
            .channels
            .widgets
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|w| {
                w.conversation
                    && w.visit == b.visit.visit
                    && w.conversation_dispatch
                        .as_ref()
                        .is_some_and(|c| c.instance == instance)
            }),
        InputKind::MultiConversation { .. } => s
            .app_home
            .channels
            .multiples
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|w| {
                w.conversation
                    && w.visit == b.visit.visit
                    && w.conversation_dispatch
                        .as_ref()
                        .is_some_and(|c| c.instance == instance)
            }),
        _ => false,
    }
}
pub(in crate::app_home::inputs::channels) fn disabled(s: &ShellState, key: &InputKey) -> bool {
    inputs::form(s).is_some_and(|f| {
        f.conversation_field(key).is_ok_and(|f| {
            matches!(
                f.kind,
                InputKind::Conversation | InputKind::MultiConversation { .. }
            )
        })
    }) && inputs::static_select::loading(s, &key.block_id)
}
pub(crate) fn parents(s: &ShellState) -> Vec<ConversationParent> {
    let Some(owner) = inputs::owner(s) else {
        return vec![];
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return vec![];
    };
    s.app_home
        .channels
        .widgets
        .iter()
        .filter_map(|((o, _), w)| {
            let c = w.conversation_dispatch.as_ref()?;
            (o == &owner
                && w.conversation
                && w.visit == h.id
                && c.binding.owner.current(s)
                && inputs::form(s).is_some_and(|f| f.raw_view() == &c.binding.raw))
            .then(|| c.parent.clone())
        })
        .chain(
            s.app_home
                .channels
                .multiples
                .iter()
                .filter_map(|((o, _), w)| {
                    let c = w.conversation_dispatch.as_ref()?;
                    (o == &owner
                        && w.conversation
                        && w.visit == h.id
                        && c.binding.owner.current(s)
                        && inputs::form(s).is_some_and(|f| f.raw_view() == &c.binding.raw))
                    .then(|| c.parent.clone())
                }),
        )
        .collect()
}
pub(in crate::app_home::inputs::channels) fn multi_input(
    s: &mut ShellState,
    t: &Ticket,
    values: Vec<String>,
    event: &mut Option<super::super::dispatch::Event>,
) -> bool {
    if !t.current(s)
        || !inputs::editable(s)
        || s.app_home
            .channels
            .multiples
            .get(&(t.form.clone(), t.key.clone()))
            .and_then(|w| w.conversation_dispatch.as_ref())
            .is_none()
    {
        return false;
    }
    let Some(form) = s.app_home.forms.get_mut(&t.form) else {
        return false;
    };
    let Ok(action) = form.multi_conversation_input(&t.key, values) else {
        return false;
    };
    let Ok(parent) = form.multi_conversation_parent(&t.key) else {
        return false;
    };
    inputs::bump(s, &t.form);
    let c = s
        .app_home
        .channels
        .multiples
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap()
        .conversation_dispatch
        .as_mut()
        .unwrap();
    c.parent = parent;
    *event = action.map(|action| super::super::dispatch::Event {
        action,
        binding: c.binding.clone(),
        instance: c.instance,
    });
    true
}
pub(in crate::app_home::inputs::channels) fn input(
    s: &mut ShellState,
    t: &Ticket,
    value: Option<String>,
    event: &mut Option<super::super::dispatch::Event>,
) -> bool {
    if !t.current(s)
        || !inputs::editable(s)
        || s.app_home
            .channels
            .widgets
            .get(&(t.form.clone(), t.key.clone()))
            .and_then(|w| w.conversation_dispatch.as_ref())
            .is_none()
    {
        return false;
    }
    let Some(form) = s.app_home.forms.get_mut(&t.form) else {
        return false;
    };
    let Ok(action) = form.conversation_input(&t.key, value) else {
        return false;
    };
    let Ok(parent) = form.conversation_parent(&t.key) else {
        return false;
    };
    inputs::bump(s, &t.form);
    let w = s
        .app_home
        .channels
        .widgets
        .get_mut(&(t.form.clone(), t.key.clone()))
        .unwrap();
    let c = w.conversation_dispatch.as_mut().unwrap();
    c.parent = parent;
    *event = action.map(|action| super::super::dispatch::Event {
        action,
        binding: c.binding.clone(),
        instance: c.instance,
    });
    true
}
/// Logical widget evidence only; no fabricated native conversation mount.
pub(in crate::app_home::inputs::channels) fn proof(s: &ShellState) -> Value {
    let Some(owner) = inputs::owner(s) else {
        return Value::Null;
    };
    let parent = inputs::form(s).and_then(|f| f.state().ok());
    json!(s.app_home.channels.widgets.iter()
        .filter(|((o, _), w)| o == &owner && w.conversation)
        .filter_map(|((_, key), w)| w.conversation_dispatch.as_ref().map(|c| json!({
            "block":key.block_id,"action":key.action_id,"visit":w.visit,"instance":c.instance,
            "revision":w.revision,"initial":w.initial,"visible":w.visible,
            "parent":parent.as_ref().and_then(|v|v.get("values")).and_then(|v|v.get(&key.block_id)).and_then(|v|v.get(&key.action_id)),
            "current":instance_current(s,&c.binding,c.instance),"loading":disabled(s,key)
        }))).collect::<Vec<_>>())
}
pub(in crate::app_home::inputs::channels) fn multi_proof(s: &ShellState) -> Value {
    let Some(owner) = inputs::owner(s) else {
        return Value::Null;
    };
    let parent = inputs::form(s).and_then(|f| f.state().ok());
    json!(s.app_home.channels.multiples.iter()
        .filter(|((o, _), w)| o == &owner && w.conversation)
        .filter_map(|((_, key), w)| w.conversation_dispatch.as_ref().map(|c| json!({
            "block":key.block_id,"action":key.action_id,"visit":w.visit,"instance":c.instance,
            "revision":w.revision,"initial":w.initial,"visible":w.visible,"invalid":w.invalid,
            "parent":parent.as_ref().and_then(|v|v.get("values")).and_then(|v|v.get(&key.block_id)).and_then(|v|v.get(&key.action_id)),
            "current":instance_current(s,&c.binding,c.instance),"loading":disabled(s,key)
        }))).collect::<Vec<_>>())
}
