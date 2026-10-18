//! ACK-triggered preference work has its own token and current-state receipt.
use super::*;
use super_platinum_core::slack_lists::{
    ListDefaultViewPreference, ListDefaultViewPreferenceAttempt, ListDefaultViewPreferencePermit,
    ViewRefinementAcknowledgement,
};

pub(super) async fn before(state: Signal<ShellState>, guard: &Guard) -> Result<Value, Failure> {
    if super::super::super::super::fixture::enabled(&state.peek()) {
        let request =
            api::list_default_view_preference_read(&state.peek().core.client, &fixture::session());
        return fixture::refinement::read_preference(state, request, true, || {
            guard.current(&state.peek(), Stage::Preparing) && !registry::blocked(&guard.key.owner)
        })
        .await;
    }
    let c = guard
        .scope
        .request(&state.peek())
        .ok_or(Failure::NotRequested(Error::TransportNotConfigured))?;
    c.transport
        .execute_list_default_view_preference_read(
            api::list_default_view_preference_read(&c.client, &c.session),
            || {
                guard.current(&state.peek(), Stage::Preparing)
                    && !registry::blocked(&guard.key.owner)
            },
        )
        .await
}
pub(super) fn admit(
    prefs: &Value,
    ack: ViewRefinementAcknowledgement,
    document: &Document,
) -> Option<(
    ListDefaultViewPreferenceAttempt,
    ListDefaultViewPreferencePermit,
)> {
    let intent = ListDefaultViewPreference::capture_acknowledged(prefs, ack).ok()?;
    let mut attempt = ListDefaultViewPreferenceAttempt::new(intent);
    let permit = attempt.admit(prefs, document).ok()?;
    Some((attempt, permit))
}
fn current(shell: &ShellState, guard: &Guard, token: u64) -> bool {
    if !guard.account_current(shell) || registry::blocked(&guard.key.owner) {
        return false;
    }
    // Parent Check can change its job; the original independent child token and
    // exact current custody remain the authority, never a newly captured Save.
    let receipt = with_registry(|r| {
        r.entries
            .get(&guard.key)
            .filter(|e| e.serial == guard.serial)
            .and_then(|e| e.refinement_receipt.clone())
    });
    receipt
        .is_some_and(|r| r.token == token && r.custody.as_ref().is_some_and(|c| c.current(shell)))
}
fn phase(guard: &Guard, token: u64, value: Preference) {
    with_registry(|r| {
        if let Some(receipt) = r
            .entries
            .get_mut(&guard.key)
            .filter(|e| e.serial == guard.serial)
            .and_then(|e| e.refinement_receipt.as_mut())
            .filter(|r| r.token == token)
        {
            receipt.preference = value;
        }
    });
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    guard: Guard,
    (mut attempt, permit): (
        ListDefaultViewPreferenceAttempt,
        ListDefaultViewPreferencePermit,
    ),
) {
    let token = with_registry(|r| {
        let receipt = r
            .entries
            .get_mut(&guard.key)
            .filter(|e| e.serial == guard.serial)?
            .refinement_receipt
            .as_mut()?;
        receipt.intent = Some(attempt.intent().clone());
        receipt.preference = Preference::InFlight;
        Some(receipt.token)
    });
    let Some(token) = token else {
        return;
    };
    let result = if current(&state.peek(), &guard, token) {
        if super::super::super::super::fixture::enabled(&state.peek()) {
            let request = api::list_default_view_preference(
                &state.peek().core.client,
                &fixture::session(),
                permit,
            );
            fixture::refinement::write_preference(state, &guard, request, || {
                current(&state.peek(), &guard, token)
            })
            .await
        } else if let Some(c) = guard.scope.request(&state.peek()) {
            c.transport
                .execute_list_default_view_preference(
                    api::list_default_view_preference(&c.client, &c.session, permit),
                    || current(&state.peek(), &guard, token),
                )
                .await
        } else {
            Err(Failure::NotRequested(Error::TransportNotConfigured))
        }
    } else {
        Err(Failure::NotRequested(Error::Api(
            "refinement_preference_retired".into(),
        )))
    };
    if let Err(Failure::NotRequested(e) | Failure::Unconfirmed(e)) = &result {
        registry::record(&guard.key.owner, e);
    }
    let unsent = matches!(result, Err(Failure::NotRequested(_)));
    attempt.finish(unsent);
    phase(
        &guard,
        token,
        if unsent {
            Preference::NotRequested
        } else {
            Preference::Unconfirmed
        },
    );
    state.write();
    if unsent || !current(&state.peek(), &guard, token) {
        return;
    }
    let read = if super::super::super::super::fixture::enabled(&state.peek()) {
        let request =
            api::list_default_view_preference_read(&state.peek().core.client, &fixture::session());
        fixture::refinement::read_preference(state, request, false, || {
            current(&state.peek(), &guard, token)
        })
        .await
    } else {
        let Some(c) = guard.scope.request(&state.peek()) else {
            return;
        };
        c.transport
            .execute_list_default_view_preference_read(
                api::list_default_view_preference_read(&c.client, &c.session),
                || current(&state.peek(), &guard, token),
            )
            .await
    };
    match read {
        Ok(raw) if current(&state.peek(), &guard, token) && attempt.observe(&raw) => {
            phase(&guard, token, Preference::Observed);
        }
        Err(Failure::NotRequested(e) | Failure::Unconfirmed(e)) => {
            registry::record(&guard.key.owner, &e)
        }
        _ => {}
    }
    state.write();
}

#[cfg(test)]
mod borrow_tests;
