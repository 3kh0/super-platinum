use super::*;
use super_platinum_core::slack::api;

pub(super) async fn load(context: &crate::scheduled::Context, id: &str) -> Result<Loaded, Error> {
    let (document, records, allow_lists, personal_task_lists, list_experiments) = tokio::join!(
        api::fetch_list_document(&context.transport, &context.client, &context.session, id),
        api::fetch_list_records(&context.transport, &context.client, &context.session, id),
        api::fetch_list_allow_lists(&context.transport, &context.client, &context.session),
        api::fetch_list_personal_task_lists(&context.transport, &context.client, &context.session),
        api::fetch_list_experiments(&context.transport, &context.client, &context.session),
    );
    Ok(Loaded {
        document: document?,
        records: records?,
        // A permission outage must not hide readable content. Missing permission
        // also causes mutation preflight to reject the write.
        allow_lists: allow_lists.unwrap_or(None),
        personal_task_lists: personal_task_lists.ok(),
        list_experiments: list_experiments.ok(),
    })
}
pub(crate) fn refresh(mut state: Signal<ShellState>) {
    if fixture::enabled(&state.peek()) {
        fixture::refresh(&mut state.write());
        return;
    }
    let Some(attempt) = begin_read(&mut state.write()) else {
        return;
    };
    let Some(context) = attempt.scope.request(&state.peek()) else {
        finish_read(
            &mut state.write(),
            &attempt,
            Err(Error::TransportNotConfigured),
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = load(&context, &attempt.id).await;
        let refresh_again = finish_read(&mut state.write(), &attempt, result);
        read_state::ensure(state);
        record_detail::ensure_hydration(state);
        if refresh_again {
            refresh(state);
        }
    });
}
pub(crate) fn toggle_checkbox(
    mut state: Signal<ShellState>,
    row: String,
    column: String,
    generation: u64,
    layout_generation: u64,
) {
    let Some(row) =
        layout::instance_record(&state.peek(), &row, &column, generation, layout_generation)
    else {
        return;
    };
    if !checkbox_visit_current(&state.peek(), generation) {
        return;
    }
    if fixture::enabled(&state.peek()) {
        fixture::toggle_checkbox(&mut state.write(), &row, &column);
        return;
    }
    let started = begin_edit(&mut state.write(), &row, &column);
    let attempt = match started {
        Ok(attempt) => attempt,
        Err(error) => {
            if let Some(view) = state.write().slack_lists.view.as_mut() {
                view.error = Some(error.into());
            }
            return;
        }
    };
    dispatch(state, attempt);
}
pub(super) fn dispatch(mut state: Signal<ShellState>, attempt: EditAttempt) {
    if fixture::enabled(&state.peek()) {
        fixture::dispatch(&mut state.write(), attempt);
        return;
    }
    let Some(context) = attempt.scope.request(&state.peek()) else {
        cancel_preflight(
            &mut state.write(),
            &attempt,
            "Sign in again before changing this list.",
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        // Re-read access, schema and canonical cell before sending one write.
        let loaded = match load(&context, attempt.edit.list_id()).await {
            Ok(loaded) => loaded,
            Err(error) => {
                let current = attempt.current(&state.peek());
                cancel_preflight(
                    &mut state.write(),
                    &attempt,
                    "Could not check your access or the current cell. Refresh before trying again.",
                );
                if current
                    && let Some(view) = state
                        .write()
                        .slack_lists
                        .view
                        .as_mut()
                        .filter(|v| v.id == attempt.edit.list_id())
                {
                    view.retry_at = retry_at(&error);
                }
                return;
            }
        };
        let validated = validate_edit(&mut state.write(), &attempt, loaded);
        if let Err(error) = validated {
            cancel_preflight(&mut state.write(), &attempt, error);
            return;
        }
        // validate_edit checks account, visit, selected view and canonical cell. No
        // await separates that guard from this single-attempt dispatch, and the
        // transport never holds this write for a later offline recovery.
        let result = context
            .transport
            .execute_list_cell_update(api::list_cell_update(
                &context.client,
                &context.session,
                &attempt.edit,
            ))
            .await;
        let acknowledged = result
            .as_ref()
            .is_ok_and(|value| attempt.edit.validate_response(value).is_ok());
        finish_edit(&mut state.write(), &attempt, result);
        // A successful reply acknowledges the write but does not replace full
        // record metadata. Reconcile through a guarded read, never a retry.
        if acknowledged && attempt.visible(&state.peek()) {
            refresh(state);
        }
    });
}
