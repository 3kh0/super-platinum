//! Choreography only: reads and writes use the existing List loader and
//! single-attempt cells.update executor. No endpoint, replay queue or retry loop.
use super::*;
use super_platinum_core::slack::api;

async fn load_target(
    context: &crate::scheduled::Context,
    list: &str,
    row: &str,
    mut current: impl FnMut() -> bool,
) -> Result<Loaded, Error> {
    if !current() {
        return Err(Error::TransportNotConfigured);
    }
    let mut loaded = super::super::network::load(context, list).await?;
    if !current() {
        return Err(Error::TransportNotConfigured);
    }
    // References can point outside the first page. Use the already supported,
    // exact identity record-info endpoint, never infer absence from a cursor.
    if loaded.records.record(row).is_none() {
        let record = api::fetch_list_record_info(
            &context.transport,
            &context.client,
            &context.session,
            list,
            row,
        )
        .await?;
        if !current() {
            return Err(Error::TransportNotConfigured);
        }
        extend_target(&mut loaded, &record)?;
    }
    Ok(loaded)
}
pub(super) fn extend_target(
    loaded: &mut Loaded,
    record: &super_platinum_core::slack_lists::ListRecordReadback,
) -> Result<(), Error> {
    if record.list_id() != loaded.document.id || loaded.records.record(record.record_id()).is_some()
    {
        return Err(Error::Api("completion_record_identity_mismatch".into()));
    }
    let raw = record
        .record()
        .ok_or_else(|| Error::Api("record_deleted".into()))?
        .raw
        .clone();
    let mut page = loaded.records.raw.clone();
    page.get_mut("records")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Error::Api("completion_records_unavailable".into()))?
        .push(raw);
    loaded.records =
        RecordsPage::parse(page).map_err(|_| Error::Api("completion_record_shape".into()))?;
    Ok(())
}
fn context(state: &Signal<ShellState>, task: &Task) -> Option<crate::scheduled::Context> {
    task.current(&state.peek())
        .then(|| task.scope.request(&state.peek()))
        .flatten()
}
fn entry(state: &Signal<ShellState>, task: &Task) -> Option<Entry> {
    task.current(&state.peek())
        .then(|| {
            state
                .peek()
                .slack_lists
                .completion
                .entries
                .get(&task.serial)
                .cloned()
        })
        .flatten()
}
fn refusal(mut state: Signal<ShellState>, task: &Task, error: Option<&Error>) {
    failed(
        &mut state.write(),
        task,
        "The current To-do, access or visit could not be verified. No new completion write was sent. Check any earlier result before continuing.",
        error,
    );
}
pub(super) fn start(mut state: Signal<ShellState>, task: Task) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::start(&mut state.write(), task);
        return;
    }
    let Some(context) = context(&state, &task) else {
        refusal(state, &task, None);
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some(initial) = entry(&state, &task) else {
            refusal(state, &task, None);
            return;
        };
        let primary = match load_target(
            &context,
            list(&initial),
            initial.intent.primary().edit().record_id(),
            || task.current(&state.peek()),
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                refusal(state, &task, Some(&error));
                return;
            }
        };
        if !task.current(&state.peek()) {
            refusal(state, &task, None);
            return;
        }
        let reference = if let Some(target) = initial.intent.reference() {
            match load_target(&context, &target.list_id, &target.record_id, || {
                task.current(&state.peek())
            })
            .await
            {
                Ok(value) => Some(value),
                Err(error) => {
                    refusal(state, &task, Some(&error));
                    return;
                }
            }
        } else {
            None
        };
        let prepared = prepare_primary(&mut state.write(), &task, &primary, reference.as_ref());
        if let Err(message) = prepared {
            failed(&mut state.write(), &task, message, None);
            return;
        }
        // No await separates the exact current guard and invoking the existing
        // one-attempt executor. It refuses known offline instead of holding.
        let result = context
            .transport
            .execute_list_cell_update(api::list_cell_update(
                &context.client,
                &context.session,
                initial.intent.primary().edit(),
            ))
            .await;
        let visible = task.current(&state.peek());
        let acknowledged = settled(&mut state.write(), &task, false, result);
        if acknowledged && visible {
            auto_check(state, &task, true);
        }
    });
}
fn auto_check(mut state: Signal<ShellState>, previous: &Task, continue_reference: bool) {
    let next = continuation_task(&mut state.write(), previous);
    if let Some(next) = next {
        check(state, next, continue_reference);
    }
}
pub(super) fn check(mut state: Signal<ShellState>, task: Task, continue_reference: bool) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::check(&mut state.write(), task, continue_reference);
        return;
    }
    let Some(context) = context(&state, &task) else {
        refusal(state, &task, None);
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some(initial) = entry(&state, &task) else {
            refusal(state, &task, None);
            return;
        };
        let primary = match load_target(
            &context,
            list(&initial),
            initial.intent.primary().edit().record_id(),
            || task.current(&state.peek()),
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                refusal(state, &task, Some(&error));
                return;
            }
        };
        if !task.current(&state.peek()) {
            refusal(state, &task, None);
            return;
        }
        let reference = if let Some(target) = initial.intent.reference() {
            match load_target(&context, &target.list_id, &target.record_id, || {
                task.current(&state.peek())
            })
            .await
            {
                Ok(value) => Some(value),
                Err(error) => {
                    refusal(state, &task, Some(&error));
                    return;
                }
            }
        } else {
            None
        };
        let outcome = observed(&mut state.write(), &task, &primary, reference.as_ref());
        match outcome {
            Observed::Complete => super::super::network::refresh(state),
            Observed::ReferenceReady if continue_reference => {
                let next = continuation_task(&mut state.write(), &task);
                if let Some(next) = next {
                    self::reference(state, next);
                }
            }
            _ => {}
        }
    });
}
pub(super) fn reference(mut state: Signal<ShellState>, task: Task) {
    if super::super::fixture::enabled(&state.peek()) {
        fixture::reference(&mut state.write(), task);
        return;
    }
    let Some(context) = context(&state, &task) else {
        refusal(state, &task, None);
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some(initial) = entry(&state, &task) else {
            refusal(state, &task, None);
            return;
        };
        let Some(target) = initial.intent.reference() else {
            refusal(state, &task, None);
            return;
        };
        let primary = match load_target(
            &context,
            list(&initial),
            initial.intent.primary().edit().record_id(),
            || task.current(&state.peek()),
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                refusal(state, &task, Some(&error));
                return;
            }
        };
        if !task.current(&state.peek()) {
            refusal(state, &task, None);
            return;
        }
        let reference = match load_target(&context, &target.list_id, &target.record_id, || {
            task.current(&state.peek())
        })
        .await
        {
            Ok(value) => value,
            Err(error) => {
                refusal(state, &task, Some(&error));
                return;
            }
        };
        let prepared = prepare_reference(&mut state.write(), &task, &primary, &reference);
        let mirror = match prepared {
            Ok(value) => value,
            Err(message) => {
                failed(&mut state.write(), &task, message, None);
                return;
            }
        };
        let result = context
            .transport
            .execute_list_cell_update(api::list_cell_update(
                &context.client,
                &context.session,
                mirror.edit(),
            ))
            .await;
        let visible = task.current(&state.peek());
        let acknowledged = settled(&mut state.write(), &task, true, result);
        if acknowledged && visible {
            auto_check(state, &task, false);
        }
    });
}
