use super::*;
use std::time::Duration;

pub(super) async fn worker(mut state: Signal<ShellState>, worker: Arc<()>) {
    loop {
        let attempt = {
            let shell = state.peek();
            if !worker_current(&shell, &worker) {
                return;
            }
            shell
                .slack_lists
                .comments
                .file_acquisition
                .attempts
                .iter()
                .find(|a| a.phase == Phase::Queued)
                .cloned()
        };
        let Some(attempt) = attempt else {
            break;
        };
        if !attempt_current(&state.peek(), &attempt, &worker) {
            // This private old queue entry may be discarded, never published.
            let mut shell = state.write();
            if worker_current(&shell, &worker) {
                shell
                    .slack_lists
                    .comments
                    .file_acquisition
                    .attempts
                    .retain(|a| a.serial != attempt.serial);
            }
            continue;
        }
        let prepared = {
            let mut shell = state.write();
            let context = attempt.demand.source.context(&shell);
            let read = FileInfoRead::capture(&mut shell, &attempt.demand.team, &attempt.demand.id);
            let slot = shell
                .slack_lists
                .comments
                .file_acquisition
                .attempts
                .iter_mut()
                .find(|a| a.serial == attempt.serial)
                .unwrap();
            slot.phase = Phase::Loading;
            context
                .ok_or_else(unavailable)
                .and_then(|context| read.map(|read| (context, read)))
        };
        let result = match prepared {
            Err(error) => Err(error),
            Ok((context, read)) => {
                let request = request(&context, &attempt.demand.id);
                let valid = |shell: &ShellState| {
                    attempt_current(shell, &attempt, &worker) && read.current(shell)
                };
                let admitted = valid(&state.peek());
                let result = if admitted {
                    acquire(state, request, context, valid).await
                } else {
                    Err(unavailable())
                };
                let mut shell = state.write();
                if !attempt_current(&shell, &attempt, &worker) {
                    continue;
                }
                result
                    .and_then(|raw| read.publish(&mut shell, &raw))
                    .and_then(|()| {
                        attempt
                            .demand
                            .ready(&shell)
                            .then_some(())
                            .ok_or_else(unavailable)
                    })
            }
        };
        let deadline = {
            let mut shell = state.write();
            if !attempt_current(&shell, &attempt, &worker) {
                continue;
            }
            finish(&mut shell, &attempt, result)
        };
        if deadline > now() {
            arm_retry(state, attempt.clone(), deadline);
        }
    }
    let mut shell = state.write();
    if worker_current(&shell, &worker) {
        shell.slack_lists.comments.file_acquisition.worker = None;
    }
}

async fn acquire(
    state: Signal<ShellState>,
    request: PreparedRequest,
    context: crate::scheduled::Context,
    current: impl Fn(&ShellState) -> bool,
) -> Result<OrderedFileInfo, Error> {
    let read = async {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        if crate::fixture::is_fixture() {
            if !fixture::cold_files::enabled(&state.peek()) {
                return Err(unavailable());
            }
            fixture::cold_files::read(state, &request).await
        } else {
            context
                .transport
                .execute_file_info_current(request, || current(&state.peek()))
                .await
        }
    };
    guarded(read, || current(&state.peek())).await
}

async fn guarded<T>(
    read: impl std::future::Future<Output = Result<T, Error>>,
    current: impl Fn() -> bool,
) -> Result<T, Error> {
    if !current() {
        return Err(unavailable());
    }
    let cancelled = async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !current() {
                break;
            }
        }
    };
    // The losing HTTP/fixture future is dropped, including a held response.
    let result = tokio::select! {biased;
        _=cancelled=>Err(unavailable()),
        _=tokio::time::sleep(Duration::from_secs(45))=>Err(Error::Api("list_file_read_timeout".into())),
        result=read=>result,
    };
    if !current() {
        return Err(unavailable());
    }
    result
}

pub(super) fn finish(shell: &mut ShellState, attempt: &Attempt, result: Result<(), Error>) -> i64 {
    let (phase, deadline, notice) = match result {
        Ok(()) => (Phase::Ready, 0, None),
        Err(error) => {
            let (deadline, notice) = failure(&error);
            (Phase::Unavailable, deadline, Some(notice))
        }
    };
    let slot = shell
        .slack_lists
        .comments
        .file_acquisition
        .attempts
        .iter_mut()
        .find(|a| a.serial == attempt.serial)
        .unwrap();
    slot.phase = phase;
    slot.retry_at = deadline;
    slot.notice = notice;
    if deadline > now() {
        let state = &mut shell.slack_lists.comments.file_acquisition;
        state.cooldown = Some((attempt.demand.source.receipt_scope(), deadline));
        // Retry-After applies to this endpoint, not just the first visible ID.
        // Stop the rest of this queue; a later explicit retry is required.
        for queued in state
            .attempts
            .iter_mut()
            .filter(|a| a.phase == Phase::Queued)
        {
            queued.phase = Phase::Unavailable;
            queued.retry_at = deadline;
            queued.notice = notice;
        }
    }
    deadline
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    struct Dropped(Rc<Cell<bool>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    #[test]
    fn retired_original_drops_actual_pending_read_and_refuses_late_result() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let current = Cell::new(true);
                let dropped = Rc::new(Cell::new(false));
                let guard = Dropped(dropped.clone());
                let read = async move {
                    let _guard = guard;
                    std::future::pending::<Result<(), Error>>().await
                };
                let retire = async {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    current.set(false);
                };
                let (result, ()) = tokio::join!(guarded(read, || current.get()), retire);
                assert!(result.is_err());
                assert!(dropped.get());
                current.set(true);
                let result = guarded(
                    async {
                        current.set(false);
                        Ok(42)
                    },
                    || current.get(),
                )
                .await;
                assert!(
                    result.is_err(),
                    "a completed callback cannot adopt retired authority"
                );
            });
    }
    #[test]
    fn already_retired_does_not_poll_request() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let polled = Cell::new(false);
                let result = guarded(
                    async {
                        polled.set(true);
                        Ok(())
                    },
                    || false,
                )
                .await;
                assert!(result.is_err());
                assert!(!polled.get());
            });
    }
}

pub(super) fn arm_retry(mut state: Signal<ShellState>, attempt: Attempt, deadline: i64) {
    // One deadline notification only. There is no automatic retry or idle poll.
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(Duration::from_secs(
            deadline.saturating_sub(now()).max(0) as u64
        ))
        .await;
        let notify = {
            let shell = state.peek();
            attempt.demand.current(&shell)
                && shell
                    .slack_lists
                    .comments
                    .file_acquisition
                    .attempts
                    .iter()
                    .any(|a| a.serial == attempt.serial && a.retry_at == deadline)
        };
        if notify {
            let _notify = state.write();
        }
    });
}
