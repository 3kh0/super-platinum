//! Ordered human tokens are distinct from the resolved single destination.
use super::*;

/// Retire only resolution ownership; receipts remain addressable by original
/// operation token even after a replacement or Cancel.
fn retire(v: &mut State) {
    if let Some(o) = v.opening.take() {
        v.opening_history.push(o);
    }
    if let Some(o) = v.group_opening.take() {
        v.group_history.push(o);
    }
    v.selected.clear();
    v.selected_recipient = None;
    v.recipient_revision = next();
    v.loading = !v.recipients.is_empty();
    v.notice = None;
    v.epoch = next();
    v.bound = false;
    if v.recipients.len() == 1 {
        v.selected = v.recipients[0].id().to_owned();
        v.opening = Some(person::Opening::new(&v.selected));
    } else if v.recipients.len() > 1 {
        v.group_opening = Some(group::Opening::new(&v.recipients));
    }
}
async fn resolve(state: Signal<ShellState>, visit: u64, id: u64) {
    bind(state, visit, id).await;
    let (count, revision) = attempt(&state.peek(), id)
        .and_then(|a| a.confirmation_share.as_ref())
        .map_or((0, 0), |v| (v.recipients.len(), v.recipient_revision));
    if count == 1 {
        person::resolve(state, visit, id).await;
    } else if count > 1 {
        group::resolve(state, visit, id).await;
    }
    let query = attempt(&state.peek(), id)
        .and_then(|a| a.confirmation_share.as_ref())
        .filter(|v| {
            v.active
                && v.recipient_revision == revision
                && !v.loading
                && (v.recipients.is_empty()
                    || !v.selected.is_empty()
                        && v.opening.as_ref().is_none_or(|o| o.ready())
                        && v.group_opening.as_ref().is_none_or(|o| o.ready()))
        })
        .map(|v| v.search.query.clone());
    if let Some(query) = query {
        search::input(state, visit, id, query);
    }
}
pub(super) fn select(mut state: Signal<ShellState>, visit: u64, id: u64, value: String) {
    let captured = {
        let s = state.peek();
        Query::capture(&s, visit, id).and_then(|q| {
            let v = attempt(&s, id)?.confirmation_share.as_ref()?;
            if v.operation.is_some()
                || v.loading
                || v.search.busy()
                || !v.rows().iter().any(|r| r.0 == value) && !value.is_empty()
            {
                return None;
            }
            // An ordinary DM token expands to its actual known human peer.
            let dm = v
                .listing
                .as_ref()
                .and_then(|l| l.find(&value))
                .filter(|c| c.raw()["is_im"] == true && c.current(&s));
            let person_id = dm.and_then(|c| c.raw()["user"].as_str()).unwrap_or(&value);
            let candidate = v
                .people
                .iter()
                .find(|p| p.id() == person_id)
                .cloned()
                .or_else(|| v.recipients.iter().find(|p| p.id() == person_id).cloned())
                .or_else(|| {
                    dm.and_then(|_| {
                        peer::candidates_for(&s, &q.ctx().session.team_id, &[person_id.to_owned()])
                            .into_iter()
                            .next()
                    })
                });
            if candidate.as_ref().is_some_and(|p| !p.current(&s)) {
                return None;
            }
            Some((
                q,
                candidate.clone(),
                candidate
                    .as_ref()
                    .is_some_and(|p| v.recipients.iter().any(|r| r.id() == p.id())),
            ))
        })
    };
    let Some((q, candidate, duplicate)) = captured else {
        return;
    };
    // Only a newly added person needs destination resolution and the search
    // following its source-required query clear. Channel/blank selections keep
    // their existing query observations and perform no acquisition here.
    let resolve_recipient = candidate.is_some() && !duplicate;
    dioxus::core::spawn_forever(async move {
        if !q.current(&state.peek()) || !native::current(&q,Some(&json!({"kind":"select","value":value,"clearQuery":candidate.is_some()&&!duplicate,"restoreSelection":duplicate}))).await {return;}
        {
            let mut s = state.write();
            if !q.current(&s) || candidate.as_ref().is_some_and(|p| !p.current(&s)) {
                return;
            }
            let v = owned_mut(&mut s, &q).unwrap();
            if v.operation.is_some() || v.loading || v.search.busy() {
                return;
            }
            if duplicate {
                v.epoch = next();
                v.bound = false;
            } else if let Some(candidate) = candidate {
                v.recipients.push(candidate);
                v.search.clear_for_recipient();
                retire(v);
            } else {
                v.recipients.clear();
                retire(v);
                v.selected = value;
            }
        }
        if resolve_recipient {
            resolve(state, visit, id).await;
        } else {
            bind(state, visit, id).await;
        }
    });
}
/// Only the exact original mousedown or query key event can authorize removal.
pub(super) fn event(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    kind: &'static str,
    value: String,
) {
    let Some(q) = Query::capture(&state.peek(), visit, id) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !q.current(&state.peek())
            || !native::current(&q, Some(&json!({"kind":kind,"value":value}))).await
        {
            return;
        }
        let resolve_needed = {
            let mut s = state.write();
            if !q.current(&s) {
                return;
            }
            let v = owned_mut(&mut s, &q).unwrap();
            if v.operation.is_some() {
                return;
            }
            let index = if kind == "recipientRemove" {
                v.recipients.iter().position(|p| p.id() == value)
            } else if kind == "recipientKey" && v.search.query.is_empty() {
                if value == "Backspace" && v.selected_recipient.is_none() {
                    v.selected_recipient = v.recipients.len().checked_sub(1);
                    v.epoch = next();
                    v.bound = false;
                    None
                } else if value == "Backspace" || value == "Delete" {
                    v.selected_recipient
                } else {
                    return;
                }
            } else {
                return;
            };
            if let Some(index) = index.filter(|i| *i < v.recipients.len()) {
                v.recipients.remove(index);
                retire(v);
                true
            } else {
                false
            }
        };
        if resolve_needed {
            resolve(state, visit, id).await;
        } else {
            bind(state, visit, id).await;
        }
    });
}
