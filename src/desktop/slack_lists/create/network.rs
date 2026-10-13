//! One dispatch after fresh access/schema checks; transport neither holds nor retries.
use super::*;
use super_platinum_core::slack::api;

pub(super) fn dispatch(mut state: Signal<ShellState>, attempt: Attempt) {
    if fixture::enabled(&state.peek()) {
        fixture::dispatch_creation(&mut state.write(), attempt);
        return;
    }
    let Some(context) = attempt.scope.request(&state.peek()) else {
        rejected(
            &mut state.write(),
            &attempt,
            "Sign in again before adding an item.",
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
                    "Could not verify access before adding the item. Refresh and try again.",
                    Some(&error),
                );
                return;
            }
        };
        let validation = validate(&mut state.write(), &attempt, &loaded);
        if let Err(error) = validation {
            rejected(&mut state.write(), &attempt, error, None);
            return;
        }
        // No await separates live ownership validation from this no-hold call.
        let result = context
            .transport
            .execute_list_record_create(api::list_record_create(
                &context.client,
                &context.session,
                &attempt.intent,
                &attempt.timestamp,
            ))
            .await;
        let refresh = finish(&mut state.write(), &attempt, result);
        if refresh {
            super::super::refresh(state);
        }
    });
}
