use super::*;
use super_platinum_core::slack::api;

pub(super) fn dispatch(mut state: Signal<ShellState>, attempt: Attempt) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::dispatch(&mut state.write(), attempt);
        return;
    }
    let Some(context) = attempt.scope.request(&state.peek()) else {
        rejected(
            &mut state.write(),
            &attempt,
            "Sign in again before deleting this item.",
            None,
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        let loaded = match super::super::network::load(&context, attempt.intent.list_id()).await {
            Ok(loaded) => loaded,
            Err(error) => {
                rejected(
                    &mut state.write(),
                    &attempt,
                    "Could not verify the item and your access. Refresh before deleting it.",
                    Some(&error),
                );
                return;
            }
        };
        if attempt.restoring {
            // Full metadata loading may outlive cancellation, a visit, or the
            // account. Retire its old preflight before starting another read.
            if !undo::preflight_current(&state.peek(), &attempt) {
                rejected(
                    &mut state.write(),
                    &attempt,
                    "This Undo is no longer current.",
                    None,
                );
                return;
            }
            let readback = match api::fetch_list_record_info(
                &context.transport,
                &context.client,
                &context.session,
                attempt.intent.list_id(),
                attempt.intent.record_id(),
            )
            .await
            {
                Ok(readback) => readback,
                Err(error) => {
                    rejected(
                        &mut state.write(),
                        &attempt,
                        "Could not verify this item before Undo. Refresh and try again.",
                        Some(&error),
                    );
                    return;
                }
            };
            let validation =
                undo::validate_restore(&mut state.write(), &attempt, &loaded, &readback);
            let intent = match validation {
                Ok(intent) => intent,
                Err(error) => {
                    rejected(&mut state.write(), &attempt, error, None);
                    return;
                }
            };
            let result = context
                .transport
                .execute_list_record_update_deletion(api::list_record_update_deletion(
                    &context.client,
                    &context.session,
                    &intent,
                ))
                .await
                .and_then(|response| {
                    intent.validate_response(&response).map_err(|error| {
                        ListDeleteFailure::Uncertain(Error::Api(error.to_string()))
                    })
                });
            let check = lifecycle::finish_restore(&mut state.write(), &attempt, result);
            if check {
                check_result(state, attempt.serial);
            }
            return;
        }
        let validation = validate(&mut state.write(), &attempt, &loaded);
        if let Err(error) = validation {
            rejected(&mut state.write(), &attempt, error, None);
            return;
        }
        // No await separates the serial full-scope check from this no-hold,
        // no-retry transport. The immutable intent remains tied to its target.
        let result = context
            .transport
            .execute_list_record_delete(api::list_record_delete(
                &context.client,
                &context.session,
                &attempt.intent,
            ))
            .await
            .and_then(|response| {
                attempt
                    .intent
                    .validate_response(&response)
                    .map_err(|error| ListDeleteFailure::Uncertain(Error::Api(error.to_string())))
            });
        let check = finish(&mut state.write(), &attempt, result);
        if check {
            check_result(state, attempt.serial);
        }
    });
}
pub(super) fn check(mut state: Signal<ShellState>, attempt: ReadbackAttempt) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::check(&mut state.write(), attempt);
        return;
    }
    let Some(context) = attempt.scope.request(&state.peek()) else {
        finish_check(
            &mut state.write(),
            &attempt,
            Err(Error::TransportNotConfigured),
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !attempt.current(&state.peek()) {
            finish_check(
                &mut state.write(),
                &attempt,
                Err(Error::TransportNotConfigured),
            );
            return;
        }
        let result = api::fetch_list_record_info(
            &context.transport,
            &context.client,
            &context.session,
            &attempt.list_id,
            &attempt.record_id,
        )
        .await;
        let refresh = finish_check(&mut state.write(), &attempt, result);
        if refresh {
            super::super::refresh(state);
        }
    });
}
