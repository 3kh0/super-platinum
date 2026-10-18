use super::*;
use super_platinum_core::slack::api;

async fn observe(
    context: &crate::scheduled::Context,
    list: &str,
    row: &str,
    mut current: impl FnMut() -> bool,
) -> Result<(Loaded, Option<SavedItem>), Error> {
    if !current() {
        return Err(Error::TransportNotConfigured);
    }
    let loaded = super::super::network::load(context, list).await?;
    if !current() {
        return Err(Error::TransportNotConfigured);
    }
    let record = loaded
        .records
        .record(row)
        .ok_or_else(|| Error::Api("List item is unavailable".into()))?;
    let saved = if record.raw.get("saved").is_some() {
        api::fetch_saved_list_record(
            &context.transport,
            &context.client,
            &context.session,
            list,
            row,
        )
        .await?
    } else {
        None
    };
    if !current() {
        return Err(Error::TransportNotConfigured);
    }
    Ok((loaded, saved))
}
pub(super) fn load_draft(mut state: Signal<ShellState>, origin: Origin) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::load(&mut state.write(), origin);
        return;
    }
    let Some(context) = origin.scope.request(&state.peek()) else {
        loaded(
            &mut state.write(),
            &origin,
            Err(Error::TransportNotConfigured),
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = observe(&context, &origin.list, &origin.row, || {
            origin.read_current(&state.peek())
        })
        .await;
        loaded(&mut state.write(), &origin, result);
    });
}
pub(super) fn dispatch(mut state: Signal<ShellState>, attempt: Attempt) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::dispatch(&mut state.write(), attempt);
        return;
    }
    let Some(context) = attempt.origin.scope.request(&state.peek()) else {
        rejected(
            &mut state.write(),
            &attempt,
            "Sign in again before setting a reminder.",
            None,
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !attempt.preparing(&state.peek()) {
            rejected(
                &mut state.write(),
                &attempt,
                "This reminder was cancelled.",
                None,
            );
            return;
        }
        let result = observe(&context, &attempt.origin.list, &attempt.origin.row, || {
            attempt.preparing(&state.peek())
        })
        .await;
        let (loaded, item) = match result {
            Ok(pair) => pair,
            Err(error) => {
                rejected(
                    &mut state.write(),
                    &attempt,
                    "Could not check the current reminder. No change was sent.",
                    Some(&error),
                );
                return;
            }
        };
        if !validate(&mut state.write(), &attempt, &loaded, item.as_ref()) {
            rejected(
                &mut state.write(),
                &attempt,
                "This reminder changed. Close and reopen it.",
                None,
            );
            return;
        }
        let result = context
            .transport
            .execute_list_reminder(
                api::list_reminder_change(&context.client, &context.session, &attempt.intent),
                || attempt.sending(&state.peek()),
            )
            .await;
        if finish(&mut state.write(), &attempt, result) {
            super::check(state, attempt.origin.serial);
        }
    });
}
pub(super) fn check(mut state: Signal<ShellState>, read: Read) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::check(&mut state.write(), read);
        return;
    }
    let Some(context) = read.scope.request(&state.peek()) else {
        finish_check(
            &mut state.write(),
            &read,
            Err(Error::TransportNotConfigured),
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !read.current(&state.peek()) {
            finish_check(
                &mut state.write(),
                &read,
                Err(Error::TransportNotConfigured),
            );
            return;
        }
        let result = super::super::network::load(&context, &read.key.1).await;
        if finish_check(&mut state.write(), &read, result) {
            super::super::network::refresh(state);
        }
    });
}
