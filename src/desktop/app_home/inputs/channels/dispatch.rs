//! Channel callbacks reuse Home scheduling and typed picker custody.
use super::*;
use crate::app_home::inputs::static_select::Binding;
use super_platinum_core::block_actions::form::home::{ChannelParent, StaticEvent};
#[cfg(test)]
mod tests;

pub(super) struct Custody {
    pub(super) instance: u64,
    pub(super) parent: ChannelParent,
    pub(super) binding: Binding,
}
pub(super) struct Event {
    pub(super) action: StaticEvent,
    pub(super) binding: Binding,
    pub(super) instance: u64,
}
pub(super) fn binding(s: &ShellState, key: &InputKey) -> Option<Binding> {
    let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
    let form = super::super::form(s)?;
    if !matches!(
        form.channel_field(key).ok()?.kind,
        InputKind::Channel | InputKind::MultiChannel { .. }
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
/// Picker revisions retire callback tickets; they are not debounce identity.
pub(crate) fn instance_current(s: &ShellState, b: &Binding, instance: u64) -> bool {
    if conversation::dispatch::instance_current(s, b, instance) {
        return true;
    }
    if !b.current(s) {
        return false;
    }
    let Some(field) = super::super::form(s).and_then(|f| f.channel_field(&b.key).ok()) else {
        return false;
    };
    let index = (b.form.clone(), b.key.clone());
    match field.kind {
        InputKind::Channel => {
            mode::Observation::capture(s, &b.owner).is_some()
                && s.app_home.channels.widgets.get(&index).is_some_and(|w| {
                    !w.conversation
                        && w.visit == b.visit.visit
                        && w.dispatch.as_ref().is_some_and(|c| c.instance == instance)
                })
        }
        // Multi-channel uses its existing public-only policy, not tpNE mode.
        InputKind::MultiChannel { .. } => {
            s.app_home.channels.multiples.get(&index).is_some_and(|w| {
                !w.conversation
                    && w.visit == b.visit.visit
                    && w.dispatch.as_ref().is_some_and(|c| c.instance == instance)
            })
        }
        _ => false,
    }
}
pub(crate) fn disabled(s: &ShellState, key: &InputKey) -> bool {
    conversation::dispatch::disabled(s, key)
        || (super::super::form(s).is_some_and(|f| {
            f.channel_field(key).is_ok_and(|f| {
                matches!(f.kind, InputKind::Channel | InputKind::MultiChannel { .. })
            })
        }) && super::super::static_select::loading(s, &key.block_id))
}
pub(super) fn schedule(state: Signal<ShellState>, event: Option<Event>) {
    if let Some(e) = event {
        super::super::static_select::typed_event(state, e.binding, e.instance, e.action);
    }
}
pub(crate) fn parents(s: &ShellState) -> Vec<ChannelParent> {
    let Some(o) = super::super::owner(s) else {
        return vec![];
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return vec![];
    };
    s.app_home
        .channels
        .widgets
        .iter()
        .filter_map(|((owner, _), w)| {
            let c = w.dispatch.as_ref()?;
            (owner == &o
                && !w.conversation
                && w.visit == h.id
                && c.binding.owner.current(s)
                && super::super::form(s).is_some_and(|f| f.raw_view() == &c.binding.raw))
            .then(|| c.parent.clone())
        })
        .chain(
            s.app_home
                .channels
                .multiples
                .iter()
                .filter_map(|((owner, _), w)| {
                    let c = w.dispatch.as_ref()?;
                    (owner == &o
                        && !w.conversation
                        && w.visit == h.id
                        && c.binding.owner.current(s)
                        && super::super::form(s).is_some_and(|f| f.raw_view() == &c.binding.raw))
                    .then(|| c.parent.clone())
                }),
        )
        .collect()
}
/// Explicit product Reset clears this owner's shared lookup widgets and reads.
/// It does not grant any additional conversation dispatch.
pub(crate) fn reset(s: &mut ShellState, owner: &FormOwner) {
    s.app_home.channels.widgets.retain(|(o, _), _| o != owner);
    s.app_home.channels.multiples.retain(|(o, _), _| o != owner);
    if s.app_home
        .channels
        .picker
        .as_ref()
        .is_some_and(|p| &p.ticket.form == owner)
    {
        s.app_home.channels.picker = None;
    }
}
/// Logical ownership proof only: these picker widgets have no synthetic JS mount.
pub(crate) fn proof(s: &ShellState) -> Value {
    let Some(o) = super::super::owner(s) else {
        return json!({"channelWidgets":null,"multiChannelWidgets":null,"conversationWidgets":null,"multiConversationWidgets":null,"channelPickerBlock":null,"sharedLookupWidgets":null,"sharedLookupMultiples":null});
    };
    let parent = super::super::form(s).and_then(|f| f.state().ok());
    json!({
        "conversationWidgets":conversation::dispatch::proof(s),
        "multiConversationWidgets":conversation::dispatch::multi_proof(s),
        "channelWidgets":s.app_home.channels.widgets.iter().filter(|((owner,_),w)|owner==&o && !w.conversation)
            .filter_map(|((_,k),w)|w.dispatch.as_ref().map(|c|json!({"block":k.block_id,"action":k.action_id,
                "visit":w.visit,"instance":c.instance,"revision":w.revision,"initial":w.initial,"visible":w.visible,
                "parent":parent.as_ref().and_then(|v|v.get("values")).and_then(|v|v.get(&k.block_id)).and_then(|v|v.get(&k.action_id)),
                "current":instance_current(s,&c.binding,c.instance),"loading":disabled(s,k)}))).collect::<Vec<_>>(),
        "multiChannelWidgets":s.app_home.channels.multiples.iter().filter(|((owner,_),w)|owner==&o && !w.conversation)
            .filter_map(|((_,k),w)|w.dispatch.as_ref().map(|c|json!({"block":k.block_id,"action":k.action_id,
                "visit":w.visit,"instance":c.instance,"revision":w.revision,"initial":w.initial,"visible":w.visible,
                "parent":parent.as_ref().and_then(|v|v.get("values")).and_then(|v|v.get(&k.block_id)).and_then(|v|v.get(&k.action_id)),
                "current":instance_current(s,&c.binding,c.instance),"loading":disabled(s,k),"invalid":w.invalid}))).collect::<Vec<_>>(),
        "channelPickerBlock":s.app_home.channels.picker.as_ref().filter(|p|p.ticket.form==o).map(|p|&p.ticket.key.block_id),
        "sharedLookupWidgets":s.app_home.channels.widgets.iter().filter(|((owner,_),_)|owner==&o)
            .map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"kind":if w.conversation {"conversations_select"} else {"channels_select"},
                "visit":w.visit,"revision":w.revision,"initial":w.initial,"visible":w.visible,
                "retainedChoice":w.retained_choice.as_ref().map(|c|json!({"id":c.id,"label":c.label,"canonical":c.canonical})),
                "current":display(s,k).is_some()})).collect::<Vec<_>>(),
        "sharedLookupMultiples":s.app_home.channels.multiples.iter().filter(|((owner,_),_)|owner==&o)
            .map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"kind":if w.conversation {"multi_conversations_select"} else {"multi_channels_select"},
                "visit":w.visit,"revision":w.revision,"initial":w.initial,"visible":w.visible,"invalid":w.invalid,
                "retainedChoices":w.retained_choices.iter().map(|(id,c)|(id.clone(),json!({"id":c.id,"label":c.label,"canonical":c.canonical}))).collect::<BTreeMap<_,_>>(),
                "current":display(s,k).is_some()})).collect::<Vec<_>>()
    })
}
