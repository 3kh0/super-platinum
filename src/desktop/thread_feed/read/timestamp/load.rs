//! Original navigation-data reads, then one ingress-linearized typed invocation.
use super::*;
use super_platinum_core::{slack::api::thread_timestamp::Request, thread_timestamp::Target};
fn unavailable() -> Error {
    Error::Api("thread_timestamp_unavailable".into())
}
fn not_requested() -> Error {
    Error::Offline("Timestamp read was not requested".into())
}
async fn read(state: Signal<ShellState>, p: &Intent, request: Request) -> Result<Value, Error> {
    if !state.try_peek().is_ok_and(|s| p.current(&s)) {
        return Err(not_requested());
    }
    #[cfg(test)]
    let result = p.io.request(request).await;
    #[cfg(not(test))]
    let result = p
        .action
        .scope
        .source
        .transport
        .execute_thread_timestamp_current(&request, || {
            state.try_peek().is_ok_and(|s| p.current(&s))
        })
        .await;
    // Keep the original outcome, especially a late429, after UI retirement.
    p.bank.observe(&p.actor, &result);
    result
}
async fn data(state: Signal<ShellState>, p: &Intent) -> Result<Target, Error> {
    let source = &p.action.scope.source;
    let (c, ts) = &p.action.key;
    let history = read(
        state,
        p,
        Request::history(&source.client, &source.session, c, ts)?,
    )
    .await?;
    if !state.try_peek().is_ok_and(|s| p.current(&s)) {
        return Err(not_requested());
    }
    let channel = read(
        state,
        p,
        Request::channel(&source.client, &source.session, c, ts)?,
    )
    .await?;
    if !state.try_peek().is_ok_and(|s| p.current(&s)) {
        return Err(not_requested());
    }
    Target::from_fresh(&source.team, c, ts, &history, &channel).map_err(|_| unavailable())
}
pub(super) async fn run(mut state: Signal<ShellState>, p: Arc<Intent>) {
    let result = data(state, &p).await;
    let work = {
        let Ok(mut s) = state.try_write() else { return };
        if !p.owned(&s) || !p.watch.current() {
            p.closed.store(true, Ordering::Release);
            if s.thread_feed_reads
                .timestamp
                .intent
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &p))
            {
                s.thread_feed_reads.timestamp.intent = None;
            }
            return;
        }
        match result {
            Err(error) => {
                s.report_failure(&error, "This timestamp needs a connection", || {
                    "Could not open this timestamp. Your Threads results are retained.".into()
                });
                p.fail("Could not open this timestamp. Refresh Threads or try again.");
                None
            }
            Ok(target) => {
                if !p.current(&s)
                    || target.team() != p.action.scope.source.team
                    || target.channel() != p.action.key.0
                    || target.timestamp() != p.action.key.1
                {
                    return;
                }
                // All native owner/data/canonical checks above share this serial
                // Shell borrow. The registry lock only seals a unique invocation;
                // it never calls current()/Dioxus/helper graphs recursively.
                let sealed = p
                    .watch
                    .with_current(|| {
                        p.phase
                            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                    })
                    .unwrap_or(false);
                if !sealed {
                    return;
                }
                let Some(commit) = p.target.clone().and_then(|t| t.seal(&s)) else {
                    p.fail("This channel changed. Refresh Threads.");
                    return;
                };
                let work = s.open_thread_timestamp(commit);
                p.closed.store(true, Ordering::Release);
                s.thread_feed_reads.timestamp.intent = None;
                work
            }
        }
    };
    #[cfg(test)]
    let io = p.io.clone();
    // The outgoing watch/attempt cannot survive into the reader's lifetime.
    drop(p);
    if let Some(work) = work {
        #[cfg(test)]
        io.run_work(state, work).await;
        #[cfg(not(test))]
        work.run_with(state, crate::bootstrap::refresh_selected_channel)
            .await;
    }
    drop(state.try_write());
}
