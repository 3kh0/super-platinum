//! Event-triggered slice of Ltli / 72tf. No age or missing-label heuristic.
use super::*;

pub(in crate::canvas::comments) fn known(
    shell: &ShellState,
    model: &Comments,
    message: &Message,
) -> Option<Cold> {
    if !visible_root(message) || model.error.is_some() {
        return None;
    }
    let state = &model.file_source;
    let origin = state.origins.get(message.ts.as_deref()?)?;
    if !same_message(message, &origin.message) {
        return None;
    }
    let predecessor = state.store.get(&origin.first)?;
    // Salesforce has its own selector path. Tombstones do not enter 72tf.
    // Inspect retained raw metadata too: Salesforce is outside display facts.
    if !predecessor.refreshable() {
        return None;
    }
    let stamp = state.stamps.get(&origin.first)?;
    // A new socket/account is not an event-refresh permission. Require the
    // original stamped owner and a concrete later event for this exact file.
    if !stamp.current(shell)
        || !crate::file_events::changed_since(
            shell,
            &stamp.owner.scope.team,
            &origin.first,
            stamp.events,
        )
    {
        return None;
    }
    let owner = model.owner.clone()?;
    let cold = Cold {
        read: Read::capture(shell, &owner)?,
        owner,
        query: model.query.clone()?,
        serial: model.serial,
        identity: state.identity.clone(),
        original: Arc::new(message.clone()),
        first: origin.first.clone(),
        predecessor: Some(predecessor),
    };
    cold.demanded(shell, model).then_some(cold)
}
