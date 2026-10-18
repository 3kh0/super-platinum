use super::*;
pub(super) fn current(shell: &ShellState, model: &State, binding: &Binding, serial: u64) -> bool {
    binding.current(shell, model)
        && entry(shell, &binding.id)
            .is_some_and(|e| e.serial == serial && !e.composing && e.sending)
}
fn error(mut state: Signal<ShellState>, id: &str, text: &'static str) {
    if let Some(e) = entry_mut(&mut state.write(), id) {
        e.sending = false;
        e.error = Some(text);
    }
}
async fn canonical(
    state: Signal<ShellState>,
    model: Signal<State>,
    binding: &Binding,
    serial: u64,
) -> Result<(File, Message), Error> {
    let invalid = || Error::Api("canvas_reply_owner_changed".into());
    if !current(&state.peek(), &model.peek(), binding, serial) {
        return Err(invalid());
    }
    if crate::fixture::is_fixture() {
        return fixture::canonical(&state.peek(), binding);
    }
    let ctx = binding
        .owner
        .scope
        .request(&state.peek())
        .ok_or_else(invalid)?;
    let file = api::fetch_canvas_file(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        &binding.owner.file,
    )
    .await?;
    if !current(&state.peek(), &model.peek(), binding, serial) {
        return Err(invalid());
    }
    let request = api::canvas_comment_replies(
        &ctx.client,
        &ctx.session,
        &binding.owner.file,
        &binding.root,
        None,
    )?;
    let raw = ctx.transport.execute(request).await?;
    if !current(&state.peek(), &model.peek(), binding, serial) {
        return Err(invalid());
    }
    let file_id = binding.owner.file.clone();
    let root_ts = binding.root.clone();
    let page =
        tokio::task::spawn_blocking(move || ReplyPage::parse(&file_id, &root_ts, true, &raw))
            .await
            .map_err(|_| invalid())?
            .map_err(|_| invalid())?;
    let root = page
        .messages
        .into_iter()
        .find(|m| m.ts.as_deref() == Some(&binding.root))
        .ok_or_else(invalid)?;
    Ok((file, root))
}
fn limited(state: Signal<ShellState>, mut model: Signal<State>, binding: &Binding, error: &Error) {
    if let Error::RateLimited { retry_after_secs } = error {
        if binding.owner.scope.current(&state.peek()) {
            rate::remember(&state.peek(), *retry_after_secs);
            if binding.current(&state.peek(), &model.peek()) {
                if let Some(wait) = rate::current(&state.peek()) {
                    set_wait(state, model, binding.owner.clone(), wait)
                }
            }
        }
    }
}
pub(super) fn send(mut state: Signal<ShellState>, model: Signal<State>, binding: Binding) {
    let request = {
        let mut shell = state.write();
        let value = model.peek();
        if !binding.current(&shell, &value) || value.loading || value.cooling() {
            return;
        }
        if super::super::mutation::pending_for_current_file(&shell)
            || super::super::super::checklist::has_pending_write(&shell)
        {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.native_busy
            || e.sending
            || e.saving
            || e.loading
            || e.composing
            || e.error.is_some()
            || e.retired_text.as_ref() == Some(&e.text)
        {
            return;
        }
        let Some(inventory) = e.inventory.clone() else {
            return;
        };
        let [draft] = inventory.drafts() else { return };
        if e.locked.is_some()
            || inventory.attempt().is_some()
            || draft.text() != e.text
            || draft.capture() != e.capture
            || e.text.trim().is_empty()
        {
            return;
        }
        let Some(file) = value.file.as_ref() else {
            return;
        };
        let Some(root) = value.root.as_ref() else {
            return;
        };
        let Ok(intent) = ReplyIntent::capture(
            &e.disk.scope.workspace,
            &e.disk.scope.user,
            file,
            root,
            &e.text,
            &uuid::Uuid::new_v4().to_string(),
        ) else {
            return;
        };
        e.sending = true;
        e.reviewed = false;
        (e.disk.clone(), inventory, intent, e.serial)
    };
    dioxus::core::spawn_forever(async move {
        let fresh = canonical(state, model, &binding, request.3).await;
        let (file, root) = match fresh {
            Ok(v) => v,
            Err(err) => {
                limited(state, model, &binding, &err);
                error(
                    state,
                    &binding.id,
                    "The thread could not be checked. Nothing was sent; your reply is retained.",
                );
                return;
            }
        };
        if !current(&state.peek(), &model.peek(), &binding, request.3)
            || request.2.revalidate(&file, &root).is_err()
        {
            error(
                state,
                &binding.id,
                "The thread or reply changed. Nothing was sent; review it before sending.",
            );
            return;
        }
        let record = request.2.record().clone();
        let expected = request.1;
        let receipt = match request
            .0
            .work(move |j| j.begin_reply(&expected, record))
            .await
        {
            Ok(Some(v)) => v,
            _ => {
                error(
                    state,
                    &binding.id,
                    "Reply admission could not be confirmed. Reload local state before another send.",
                );
                reload(state, &binding);
                return;
            }
        };
        // Make the durable lock visible before any transport handoff.
        if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
            e.locked = Some(receipt.clone());
        }
        if !current(&state.peek(), &model.peek(), &binding, request.3) {
            settle(
                state,
                model,
                binding,
                request.0,
                Outcome::NotSent(receipt),
                request.3,
            )
            .await;
            return;
        }
        let result = if crate::fixture::is_fixture() {
            fixture::execute(state, &binding, request.2).await
        } else {
            let context = {
                let shell = state.peek();
                binding.owner.scope.request(&shell)
            };
            let Some(ctx) = context else {
                settle(
                    state,
                    model,
                    binding,
                    request.0,
                    Outcome::NotSent(receipt),
                    request.3,
                )
                .await;
                return;
            };
            api::execute_canvas_comment_reply(&ctx.transport, &ctx.client, &ctx.session, request.2)
                .await
        };
        match result {
            Ok(accepted) => {
                settle(
                    state,
                    model,
                    binding,
                    request.0,
                    Outcome::Accepted(receipt, accepted),
                    request.3,
                )
                .await
            }
            Err(CanvasEditFailure::NotSent(_)) => {
                settle(
                    state,
                    model,
                    binding,
                    request.0,
                    Outcome::NotSent(receipt),
                    request.3,
                )
                .await
            }
            Err(CanvasEditFailure::Uncertain(err)) => {
                limited(state, model, &binding, &err);
                error(
                    state,
                    &binding.id,
                    "This reply may have been sent. It will not be sent again. Review the thread before archiving its local attempt.",
                );
                reload(state, &binding);
            }
        }
    });
}
fn reload(mut state: Signal<ShellState>, binding: &Binding) {
    let request = {
        let shell = state.peek();
        let Some(e) = entry(&shell, &binding.id) else {
            return;
        };
        (e.disk.clone(), e.root.clone())
    };
    let id = binding.id.clone();
    dioxus::core::spawn_forever(async move {
        let scope = request.0.scope.clone();
        let root = request.1;
        let result = request
            .0
            .work(move |j| j.reply_inventory(&scope, &root))
            .await;
        if let Some(e) = entry_mut(&mut state.write(), &id) {
            match result {
                Ok(i) => adopt(e, i),
                Err(()) => {
                    e.error = Some(
                        "The local reply receipt could not be read. Sending remains unavailable.",
                    )
                }
            }
        }
    });
}
async fn settle(
    mut state: Signal<ShellState>,
    model: Signal<State>,
    binding: Binding,
    disk: Disk,
    outcome: Outcome,
    serial: u64,
) {
    let outcome = std::sync::Arc::new(outcome);
    let work = outcome.clone();
    let scope = disk.scope.clone();
    let root = binding.root.clone();
    let result = disk
        .work(move |j| {
            let done = match &*work {
                Outcome::NotSent(r) => j.reply_not_sent(r)?,
                Outcome::Accepted(r, a) => j.reply_acknowledged(r, a)?,
            };
            Ok((done, j.reply_inventory(&scope, &root)?))
        })
        .await;
    let accepted = matches!(&*outcome, Outcome::Accepted(..));
    match result {
        Ok((true, inventory)) => {
            if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
                adopt(e, inventory);
                e.sending = false;
                e.error = if accepted {
                    None
                } else {
                    Some(
                        "Nothing was sent. Your reply is retained. Review it before a new deliberate send.",
                    )
                };
            }
            if accepted {
                if let Outcome::Accepted(receipt, _) = &*outcome {
                    mark_retired(state, &binding.id, serial, receipt.record().text());
                }
                clear_delivered(state, model, &binding, serial, &outcome).await;
                if binding.current(&state.peek(), &model.peek()) {
                    start(state, model, Query::Replies(binding.root.clone()), false);
                }
            }
        }
        _ => {
            let outcome = std::sync::Arc::try_unwrap(outcome).ok();
            if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
                e.outcome = outcome;
                e.sending = false;
                e.error = Some(
                    "The delivery result is retained, but its local receipt could not be saved. Retry local result save; it will not send again.",
                );
            }
        }
    }
}
fn mark_retired(mut state: Signal<ShellState>, id: &str, serial: u64, text: &str) {
    if let Some(e) = entry_mut(&mut state.write(), id) {
        retire_exact(e, serial, text);
    }
}
pub(super) fn retire_exact(e: &mut Entry, serial: u64, text: &str) {
    if e.serial == serial && e.text == text {
        e.retired_text = Some(text.into());
    }
}
async fn clear_delivered(
    state: Signal<ShellState>,
    model: Signal<State>,
    binding: &Binding,
    serial: u64,
    outcome: &Outcome,
) {
    if let Outcome::Accepted(receipt, _) = outcome {
        settle_field(state, model, binding, serial, receipt.record().text()).await;
    }
}
async fn settle_field(
    mut state: Signal<ShellState>,
    model: Signal<State>,
    binding: &Binding,
    serial: u64,
    expected: &str,
) {
    {
        let mut shell = state.write();
        if !binding.current(&shell, &model.peek()) {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.serial != serial || e.composing || e.native_busy {
            return;
        }
        e.native_busy = true;
    }
    let scope = binding.field_scope();
    let receipt =
        crate::view::composer::settle_scheduled_field("canvas-comment-reply", &scope, expected)
            .await;
    // A successful old-field Changed capture belongs to its original root owner
    // even if navigation retired its visible Binding while the evaluation ran.
    let changed = {
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        e.native_busy = false;
        adopt_dom(e, serial, &scope, receipt)
    };
    if changed {
        save(state, binding.id.clone());
    }
}
pub(super) fn adopt_dom(
    e: &mut Entry,
    serial: u64,
    scope: &str,
    receipt: crate::view::composer::ScheduledClearReceipt,
) -> bool {
    if e.serial != serial || e.composing {
        return false;
    }
    use crate::view::composer::ScheduledClearReceipt;
    let text = match receipt {
        ScheduledClearReceipt::Cleared { scope: s } if s == scope => String::new(),
        ScheduledClearReceipt::Changed { scope: s, text } if s == scope => text,
        _ => return false,
    };
    if e.text == text {
        return false;
    }
    e.text = text;
    e.serial = e.serial.wrapping_add(1);
    e.capture = uuid::Uuid::new_v4().to_string();
    e.retired_text = None;
    e.error = None;
    e.reviewed = false;
    true
}
pub(super) fn retry_result(mut state: Signal<ShellState>, model: Signal<State>, binding: Binding) {
    let request = {
        let mut shell = state.write();
        if !binding.current(&shell, &model.peek()) {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.sending {
            return;
        }
        let Some(outcome) = e.outcome.take() else {
            return;
        };
        e.sending = true;
        (e.disk.clone(), outcome, e.serial)
    };
    dioxus::core::spawn_forever(settle(
        state, model, binding, request.0, request.1, request.2,
    ));
}

pub(super) fn review(mut state: Signal<ShellState>, model: Signal<State>, binding: Binding) {
    let request = {
        let mut shell = state.write();
        let value = model.peek();
        if !binding.current(&shell, &value)
            || value.loading
            || value.error.is_some()
            || value.cooling()
        {
            return;
        }
        let Some(root) = value.root.clone() else {
            return;
        };
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if !e.reviewed || e.sending || e.saving || e.composing || e.native_busy {
            return;
        }
        let Some(inventory) = e.inventory.clone() else {
            return;
        };
        let Some(receipt) = inventory.attempt() else {
            return;
        };
        if receipt.record().root_ts() != binding.root {
            return;
        }
        e.sending = true;
        e.reviewed = false;
        (e.disk.clone(), inventory, root, e.serial)
    };
    dioxus::core::spawn_forever(async move {
        let fresh = canonical(state, model, &binding, request.3).await;
        let (_, root) = match fresh {
            Ok(v) => v,
            Err(err) => {
                limited(state, model, &binding, &err);
                error(
                    state,
                    &binding.id,
                    "The current thread could not be reviewed. Its reply lock remains.",
                );
                return;
            }
        };
        if !current(&state.peek(), &model.peek(), &binding, request.3) {
            error(
                state,
                &binding.id,
                "The reply changed during review. Its lock remains.",
            );
            return;
        }
        let receipt = request.1.attempt().unwrap().clone();
        let scope = request.0.scope.clone();
        let root_ts = binding.root.clone();
        let result = request
            .0
            .work(move |j| {
                let done = j.archive_unconfirmed_reply(&request.1, &root)?;
                Ok((done, j.reply_inventory(&scope, &root_ts)?))
            })
            .await;
        match result {
            Ok((true, inventory)) => {
                if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
                    adopt(e, inventory);
                    e.sending = false;
                    e.error = None;
                }
                mark_retired(state, &binding.id, request.3, receipt.record().text());
                settle_field(state, model, &binding, request.3, receipt.record().text()).await;
            }
            _ => error(
                state,
                &binding.id,
                "Saved reply versions changed or storage is unavailable. The attempt remains locked.",
            ),
        }
    });
}
