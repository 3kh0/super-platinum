//! Retained async adapters. No adapter is mounted or called until the shared
//! app-root reservation hooks and disconnected fixture checks are integrated.
use super::{Attempt, Lease, Operations};
use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::slack::{Error, api};

fn capture(state: Signal<ShellState>, lease: &Lease) -> Option<Context> {
    let context = Context::get(&state)?;
    lease.current(&state.peek(), &context).then_some(context)
}

fn report(mut operations: Signal<Operations>, lease: &Lease, message: &str) {
    let _ = operations.write().set_error(lease, message.into());
}

pub(crate) fn save(state: Signal<ShellState>, mut operations: Signal<Operations>, lease: Lease) {
    let Some(context) = capture(state, &lease) else {
        report(
            operations,
            &lease,
            "Reconnect to this draft's account before saving.",
        );
        return;
    };
    let Ok(attempt) = operations.write().freeze(&lease) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !lease.current(&state.peek(), &context) {
            let _ = operations.write().preflight_failed(
                &attempt,
                "The account changed before saving. No attachment update was sent.".into(),
            );
            return;
        }
        let readback = read(&context, attempt.draft_id()).await;
        if !lease.current(&state.peek(), &context) {
            let _ = operations.write().preflight_failed(
                &attempt,
                "The account changed before saving. No attachment update was sent.".into(),
            );
            return;
        }
        let current = match readback {
            Ok(current) => current,
            Err(error) => {
                let _ = operations.write().preflight_failed(
                    &attempt,
                    "The current draft could not be checked. No attachment update was sent.".into(),
                );
                if let Error::RateLimited { retry_after_secs } = error {
                    cooldown(operations, lease.clone(), retry_after_secs.unwrap_or(30));
                }
                return;
            }
        };
        // Exact frozen clone, built before admission; no user editable state is
        // reread and no await separates final owner/lease admission from dispatch.
        let request = api::drafts_schedule(
            &context.client,
            &context.session,
            &attempt.request(),
            Some(attempt.draft_id()),
        );
        if !lease.current(&state.peek(), &context)
            || operations.write().admit(&attempt, current.raw()).is_err()
        {
            return;
        }
        let result = context.transport.execute_draft_mutation(request).await;
        match result {
            Ok(response) => {
                let confirmed = operations
                    .write()
                    .acknowledge(&attempt, &response)
                    .unwrap_or(false);
                if confirmed {
                    project_confirmed(state, &context, &attempt, &response);
                }
            }
            Err(error) => {
                let _ = operations.write().uncertain(&attempt);
                if let Error::RateLimited { retry_after_secs } = error {
                    cooldown(operations, lease, retry_after_secs.unwrap_or(30));
                }
            }
        }
    });
}

pub(crate) fn review(state: Signal<ShellState>, mut operations: Signal<Operations>, lease: Lease) {
    let Some(context) = capture(state, &lease) else {
        report(
            operations,
            &lease,
            "Reconnect to this draft's account to review the result.",
        );
        return;
    };
    let Ok(attempt) = operations.write().review_attempt(&lease) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !lease.current(&state.peek(), &context) {
            let _ = operations.write().review(&attempt, None);
            return;
        }
        let result = read(&context, attempt.draft_id()).await;
        if !lease.current(&state.peek(), &context) {
            let _ = operations.write().review(&attempt, None);
            return;
        }
        match result {
            Ok(current) => {
                let confirmed = operations
                    .write()
                    .review(&attempt, Some(current.raw()))
                    .unwrap_or(false);
                if confirmed {
                    project_confirmed(state, &context, &attempt, current.raw());
                } else {
                    report(
                        operations,
                        &lease,
                        "The current draft does not confirm these attachment changes. Nothing was sent again.",
                    );
                }
            }
            Err(error) => {
                let _ = operations.write().review(&attempt, None);
                if let Error::RateLimited { retry_after_secs } = error {
                    let seconds = retry_after_secs.unwrap_or(30);
                    cooldown(operations, lease, seconds);
                } else {
                    report(
                        operations,
                        &lease,
                        "The draft could not be verified. A missing draft does not confirm what happened. Nothing was sent again.",
                    );
                }
            }
        }
    });
}

fn cooldown(mut operations: Signal<Operations>, lease: Lease, seconds: u64) {
    let _ = operations.write().review_cooldown(&lease, seconds);
    report(
        operations,
        &lease,
        "Slack asked us to wait before checking again. Nothing was sent again.",
    );
    // One bounded wake to refresh the button; never a request retry. The registry
    // retains the full duration even if it exceeds this wake interval.
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(seconds.min(86_400))).await;
        let _ = operations.write().refresh_cooldown(&lease);
    });
}

async fn read(context: &Context, id: &str) -> Result<api::DraftInfo, Error> {
    let request = api::drafts_info(&context.client, &context.session, id)?;
    let response = context.transport.execute(request).await?;
    api::decode_drafts_info(id, response)
}

pub(crate) fn review_other(
    state: Signal<ShellState>,
    mut operations: Signal<Operations>,
    lease: Lease,
) {
    let Some(context) = capture(state, &lease) else {
        return;
    };
    if operations.write().begin_other_review(&lease).is_err() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        if !lease.current(&state.peek(), &context) {
            let _ = operations.write().finish_other_review(&lease, None);
            return;
        }
        let result = read(&context, lease.draft_id()).await;
        if !lease.current(&state.peek(), &context) {
            let _ = operations.write().finish_other_review(&lease, None);
            return;
        }
        match result {
            Ok(current) => {
                let confirmed = operations
                    .write()
                    .finish_other_review(&lease, Some(current.raw()))
                    .unwrap_or(false);
                if confirmed {
                    project_readback(state, &context, &lease, current.raw());
                }
            }
            Err(error) => {
                let _ = operations.write().finish_other_review(&lease, None);
                if let Error::RateLimited { retry_after_secs } = error {
                    let seconds = retry_after_secs.unwrap_or(30);
                    let _ = operations.write().other_review_cooldown(&lease, seconds);
                    tokio::time::sleep(std::time::Duration::from_secs(seconds.min(86_400))).await;
                    let _ = operations.write().refresh_other_cooldown(&lease);
                }
            }
        }
    });
}

fn project_confirmed(
    state: Signal<ShellState>,
    context: &Context,
    attempt: &Attempt,
    response: &serde_json::Value,
) {
    project_readback(state, context, attempt.lease(), response);
}

fn project_readback(
    mut state: Signal<ShellState>,
    context: &Context,
    lease: &Lease,
    response: &serde_json::Value,
) {
    if !lease.current(&state.peek(), context) {
        return;
    }
    // Deserialization is projection only, after the complete raw proof. It can
    // never turn a defaulted/partial object into acknowledgement authority.
    let Ok(draft) = serde_json::from_value(response["draft"].clone()) else {
        return;
    };
    let mut shell = state.write();
    shell
        .core
        .scheduled
        .ensure_scope(context.epoch, &context.session.team_id);
    shell.core.scheduled.accept_remote(draft);
    shell.show_toast("Draft changes confirmed.");
}

/// Fetch titles lazily in order. At most sixteen automatic lookups per dialog;
/// every remaining original ID stays visible/removable and Preview loads its metadata.
pub(crate) fn load_metadata(
    state: Signal<ShellState>,
    mut operations: Signal<Operations>,
    lease: Lease,
) {
    let Some(context) = capture(state, &lease) else {
        return;
    };
    let Ok(snapshot) = operations.peek().snapshot(&lease) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        for row in snapshot.rows.into_iter().take(16) {
            if !lease.current(&state.peek(), &context)
                || operations.peek().snapshot(&lease).is_err()
            {
                return;
            }
            if operations
                .peek()
                .preview(&lease, &row.id)
                .ok()
                .flatten()
                .is_some()
            {
                continue;
            }
            let result = api::fetch_file(
                &context.transport,
                &context.client,
                &context.session,
                &row.id,
            )
            .await;
            if !lease.current(&state.peek(), &context) {
                return;
            }
            match result {
                Ok(file) => {
                    if operations.write().metadata(&lease, &row.id, file).is_err() {
                        return;
                    }
                }
                Err(Error::RateLimited { .. }) => return,
                Err(_) => {}
            }
        }
    });
}

pub(crate) fn preview(
    state: Signal<ShellState>,
    mut operations: Signal<Operations>,
    lease: Lease,
    id: String,
) {
    let Some(context) = capture(state, &lease) else {
        return;
    };
    if !operations
        .peek()
        .snapshot(&lease)
        .is_ok_and(|snapshot| snapshot.rows.iter().any(|row| row.id == id))
    {
        return;
    }
    let cached = operations.peek().preview(&lease, &id).ok().flatten();
    dioxus::core::spawn_forever(async move {
        if !lease.current(&state.peek(), &context) || operations.peek().snapshot(&lease).is_err() {
            return;
        }
        let file = match cached {
            Some(file) => file,
            None => {
                match api::fetch_file(&context.transport, &context.client, &context.session, &id)
                    .await
                {
                    Ok(file) => file,
                    Err(_) => {
                        report(
                            operations,
                            &lease,
                            "This file could not be loaded for preview.",
                        );
                        return;
                    }
                }
            }
        };
        if !lease.current(&state.peek(), &context)
            || operations
                .write()
                .metadata(&lease, &id, file.clone())
                .is_err()
        {
            return;
        }
        crate::files::open(state, file);
    });
}
