use super::*;
use std::{future::Future, task::Poll, time::Duration};
use super_platinum_core::slack::{PreparedRequest, client::SlackClient};

pub(in crate::canvas) fn request(
    client: &SlackClient,
    session: &super_platinum_core::config::WorkspaceSession,
    id: &str,
) -> PreparedRequest {
    client.rest_form(
        session,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
            ("skip_shares", "true".into()),
            ("_x_reason", "files-store-unknown-fetch".into()),
        ],
    )
}
/// Retained 72tf -> 20Mt.B2 -> PF known-file refresh; shares are not skipped.
pub(in crate::canvas) fn known_request(
    client: &SlackClient,
    session: &super_platinum_core::config::WorkspaceSession,
    id: &str,
) -> PreparedRequest {
    client.rest_form(
        session,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
            (
                "_x_reason",
                "files-store.ConditionalFetchManager.fetch".into(),
            ),
        ],
    )
}
/// The original 20Mt singleton success mapping, not a typed File projection.
fn mapped(raw: &Value, id: &str) -> Result<Value, Error> {
    if raw["ok"] != true
        || raw.get("error").is_some_and(|v| !v.is_null())
        || raw["file"]["id"].as_str() != Some(id)
    {
        return Err(Error::Api("canvas_file_details_invalid".into()));
    }
    let mut file = raw["file"]
        .as_object()
        .cloned()
        .ok_or_else(|| Error::Api("canvas_file_details_invalid".into()))?;
    file.insert("is_deleted".into(), json!(false));
    if file.get("todos").is_none_or(Value::is_null) {
        file.remove("todos");
    }
    for key in [
        "comments",
        "content",
        "content_html",
        "content_html_url",
        "content_highlight_html",
        "content_highlight_html_truncated",
        "public_shared",
    ] {
        match raw.get(key) {
            Some(value) => {
                file.insert(key.into(), value.clone());
            }
            None => {
                file.remove(key);
            }
        }
    }
    Ok(Value::Object(file))
}
async fn guarded<T>(
    read: impl Future<Output = Result<T, Error>>,
    current: impl Fn() -> bool,
) -> Result<T, Error> {
    let mut read = std::pin::pin!(read);
    let checked = std::future::poll_fn(|cx| {
        if !current() {
            return Poll::Ready(Err(unavailable()));
        }
        match read.as_mut().poll(cx) {
            Poll::Ready(_) if !current() => Poll::Ready(Err(unavailable())),
            result => result,
        }
    });
    tokio::select! {biased;
        _=async {loop {
            if !current() {break;}
            tokio::time::sleep(Duration::from_millis(100)).await;
        }} => Err(unavailable()),
        _=tokio::time::sleep(Duration::from_secs(45)) => Err(Error::Api("canvas_file_details_timeout".into())),
        result=checked => result,
    }
}
fn active(
    state: Signal<ShellState>,
    model: Signal<super::super::State>,
    attempt: &Attempt,
    worker: &Arc<()>,
) -> bool {
    state
        .try_read()
        .ok()
        .zip(model.try_read().ok())
        .is_some_and(|(shell, value)| {
            worker_current(&value, worker)
                && slot_current(&shell, &value, attempt)
                && attempt.cold.demanded(&shell, &value)
                && !cooling(&shell, &value)
        })
}
async fn acquire(
    state: Signal<ShellState>,
    model: Signal<super::super::State>,
    attempt: &Attempt,
    worker: &Arc<()>,
) -> Result<Value, Error> {
    let current = || active(state, model, attempt, worker);
    let read = async {
        if !current() {
            return Err(unavailable());
        }
        if crate::fixture::is_fixture() {
            if attempt.cold.known() {
                let req =
                    fixture::canonical_files::known::request(&state.peek(), attempt.cold.id())?;
                fixture::canonical_files::known::read(state, &req).await
            } else {
                let req =
                    fixture::canonical_files::cold::request(&state.peek(), attempt.cold.id())?;
                fixture::canonical_files::cold::read(state, &req).await
            }
        } else {
            let context = attempt
                .cold
                .context(&state.peek())
                .ok_or_else(unavailable)?;
            context
                .transport
                .execute_file_metadata_current(
                    if attempt.cold.known() {
                        known_request(&context.client, &context.session, attempt.cold.id())
                    } else {
                        request(&context.client, &context.session, attempt.cold.id())
                    },
                    || current(),
                )
                .await
        }
    };
    guarded(read, current).await
}
fn failure(error: &Error) -> (bool, Option<Instant>, &'static str) {
    match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => (
            // Same bounded native fallback used by the existing List title reader.
            true,
            Instant::now().checked_add(Duration::from_secs(retry_after_secs.unwrap_or(30))),
            LIMITED,
        ),
        Error::Offline(_) => (false, None, OFFLINE),
        _ => (false, None, FAILED),
    }
}
pub(super) fn arm(
    state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
    scope: Scope,
    deadline: Instant,
) {
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
        let notify = state
            .try_read()
            .ok()
            .zip(model.try_read().ok())
            .is_some_and(|(shell, value)| {
                scope.stable_current(&shell)
                    && value
                        .file_acquisition
                        .limit
                        .as_ref()
                        .is_some_and(|l| l.until == Some(deadline))
            });
        if notify {
            let wake = {
                let Ok(shell) = state.try_read() else { return };
                let Ok(mut value) = model.try_write() else {
                    return;
                };
                value.file_acquisition.limit = None;
                sync_limit(&shell, &mut value);
                value
                    .file_acquisition
                    .limit
                    .as_ref()
                    .and_then(|l| l.until.map(|at| (l.scope.clone(), at)))
            };
            if let Some((scope, at)) = wake {
                arm(state, model, scope, at);
            }
        }
        // No automatic request is dispatched when the deadline expires.
    });
}
pub(super) fn finish(
    shell: &ShellState,
    value: &mut super::super::State,
    attempt: &Attempt,
    response: Result<Value, Error>,
) -> Option<Instant> {
    let current = attempt.cold.demanded(shell, value);
    let receipt = response.as_ref().ok().cloned();
    let result = if current {
        response
            .and_then(|raw| mapped(&raw, attempt.cold.id()))
            .and_then(|file| attempt.cold.publish(shell, value, &file))
    } else {
        Err(unavailable())
    };
    let (phase, notice, limited, deadline) = if !current {
        (Phase::Retired, Some(RETIRED), false, None)
    } else {
        match result {
            Ok(()) => (Phase::Ready, None, false, None),
            Err(error) => {
                let (limited, deadline, notice) = failure(&error);
                (Phase::Unavailable, Some(notice), limited, deadline)
            }
        }
    };
    let slot = value
        .file_acquisition
        .attempts
        .iter_mut()
        .find(|a| Arc::ptr_eq(&a.token, &attempt.token))
        .unwrap();
    slot.phase = phase;
    slot.notice = notice;
    slot.receipt = receipt;
    if limited {
        rate::remember(shell, deadline);
        sync_limit(shell, value);
        let deadline = value
            .file_acquisition
            .limit
            .as_ref()
            .filter(|l| l.scope.stable_current(shell))
            .map_or(deadline, |l| match (l.until, deadline) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => None,
            });
        value.file_acquisition.limit = Some(Limit {
            scope: attempt.cold.scope().clone(),
            until: deadline,
        });
        for queued in value
            .file_acquisition
            .attempts
            .iter_mut()
            .filter(|a| a.phase == Phase::Queued)
        {
            queued.phase = Phase::Unavailable;
            queued.notice = Some(LIMITED);
        }
        deadline
    } else {
        sync_limit(shell, value);
        None
    }
}
pub(super) async fn worker(
    state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
    worker: Arc<()>,
) {
    loop {
        let attempt = {
            let Ok(shell) = state.try_read() else { return };
            let Ok(mut value) = model.try_write() else {
                return;
            };
            if !worker_current(&value, &worker) {
                return;
            }
            sync_limit(&shell, &mut value);
            if cooling(&shell, &value) {
                for queued in value
                    .file_acquisition
                    .attempts
                    .iter_mut()
                    .filter(|a| a.phase == Phase::Queued)
                {
                    queued.phase = Phase::Unavailable;
                    queued.notice = Some(LIMITED);
                }
                let wake = value
                    .file_acquisition
                    .limit
                    .as_ref()
                    .and_then(|l| l.until.map(|at| (l.scope.clone(), at)));
                drop(value);
                drop(shell);
                if let Some((scope, deadline)) = wake {
                    arm(state, model, scope, deadline);
                }
                break;
            }
            let Some(slot) = value
                .file_acquisition
                .attempts
                .iter_mut()
                .find(|a| a.phase == Phase::Queued)
            else {
                break;
            };
            slot.phase = Phase::Loading;
            slot.clone()
        };
        let response = acquire(state, model, &attempt, &worker).await;
        let deadline = {
            let Ok(shell) = state.try_read() else { return };
            let Ok(mut value) = model.try_write() else {
                return;
            };
            if !worker_current(&value, &worker) {
                return;
            }
            if !slot_current(&shell, &value, &attempt) {
                continue;
            }
            finish(&shell, &mut value, &attempt, response)
        };
        if let Some(deadline) = deadline {
            arm(state, model, attempt.cold.scope().clone(), deadline);
        }
    }
    if let Ok(mut value) = model.try_write() {
        if worker_current(&value, &worker) {
            value.file_acquisition.worker = None;
        }
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
