//! Stable multi-option lifetime and mounted prop-callback handoff.
use super::*;

pub(in crate::app_home::inputs::external) fn instance_current(
    s: &ShellState,
    b: &Binding,
    instance: u64,
) -> bool {
    b.current(s)
        && inputs::form(s).is_some_and(|f| {
            f.external_field(&b.key)
                .is_ok_and(|f| matches!(f.kind, InputKind::MultiExternalSelect { .. }))
        })
        && s.app_home
            .external
            .catalogs
            .get(&(b.form.clone(), b.key.clone()))
            .is_some_and(|c| {
                crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
                    && c.multi
                        .as_ref()
                        .is_some_and(|w| w.mount == b.visit.visit && w.instance == instance)
            })
}
pub(in crate::app_home::inputs::external) fn parents(s: &ShellState) -> Vec<ExternalParent> {
    let Some(owner) = inputs::owner(s) else {
        return vec![];
    };
    let Some(h) = s.app_home.surface.as_ref() else {
        return vec![];
    };
    s.app_home
        .external
        .catalogs
        .iter()
        .filter_map(|((o, key), c)| {
            let w = c.multi.as_ref()?;
            let survives = h.view.as_ref().is_some_and(|v| {
                v.raw()["blocks"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|b| {
                        b["type"] == "input"
                            && b["block_id"] == key.block_id
                            && b["element"]["action_id"] == key.action_id
                            && b["element"]["type"] == "multi_external_select"
                    })
                })
            });
            (survives
                && o == &owner
                && w.mount == h.id
                && w.binding.owner.current(s)
                && crate::message_buttons::inputs::user::directory::context_current(s, &c.context)
                && inputs::form(s).is_some_and(|f| f.raw_view() == &w.binding.raw))
            .then(|| w.parent.clone())
        })
        .collect()
}
pub(in crate::app_home::inputs::external) fn pending_revision(
    s: &ShellState,
    key: &InputKey,
) -> Option<u64> {
    let w = widget(s, key)?;
    w.pending.as_ref().map(|_| w.revision)
}
fn take_pending(
    s: &mut ShellState,
    key: &InputKey,
    revision: Option<u64>,
) -> Option<dispatch::Event> {
    let w = widget(s, key)?;
    if revision != Some(w.revision) {
        return None;
    }
    let owner = w.binding.form.clone();
    let event = s
        .app_home
        .external
        .catalogs
        .get_mut(&(owner, key.clone()))?
        .multi
        .as_mut()?
        .pending
        .take()?;
    instance_current(s, &event.binding, event.instance).then_some(event)
}
pub(in crate::app_home::inputs::external) fn flush_pending(
    mut state: Signal<ShellState>,
    key: &InputKey,
    revision: Option<u64>,
) {
    // Peek avoids subscribing this effect to its own consumption write.
    if pending_revision(&state.peek(), key) != revision || revision.is_none() {
        return;
    }
    let event = take_pending(&mut state.write(), key, revision);
    dispatch::schedule(state, event);
}
#[cfg(test)]
mod tests;
