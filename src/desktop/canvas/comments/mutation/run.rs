use super::*;

/// Root task retains the exact receipt even if the originating dialog is gone.
pub(super) async fn execute(
    mut state: Signal<ShellState>,
    model: Signal<super::super::State>,
    attempt: Attempt,
) {
    let initial = canonical(state, model, &attempt, ReadPurpose::Preflight).await;
    let (file, root) = match initial {
        Ok(value) => value,
        Err(error) => {
            remember_error(state, model, &attempt, &error);
            retire(&mut state.write(), &attempt);
            feedback(
                state,
                model,
                &attempt,
                Some("The thread could not be checked. Nothing was sent."),
            );
            return;
        }
    };
    if !current(&state.peek(), &model.peek(), &attempt)
        || attempt.intent.revalidate(&file, &root).is_err()
    {
        retire(&mut state.write(), &attempt);
        feedback(
            state,
            model,
            &attempt,
            Some("This thread or your access changed. Refresh comments. Nothing was sent."),
        );
        return;
    }
    let record = match attempt
        .intent
        .record(&attempt.disk.scope.workspace, &attempt.disk.scope.user)
    {
        Ok(record) => record,
        Err(_) => {
            retire(&mut state.write(), &attempt);
            return;
        }
    };
    let reservation = attempt
        .disk
        .work(move |journal| journal.begin_comment(record))
        .await;
    let receipt = match reservation {
        Ok(CommentBegin::Started(receipt)) => receipt,
        Ok(CommentBegin::Blocked(_)) => {
            retire(&mut state.write(), &attempt);
            feedback(state, model, &attempt, Some(LOCKED));
            return;
        }
        Err(()) => {
            // A failed commit is not proof that no receipt was written. Reopen
            // reads the exact journal; no dispatch occurred on this path.
            retire(&mut state.write(), &attempt);
            feedback(state, model, &attempt, Some(STORAGE));
            return;
        }
    };
    {
        let mut shell = state.write();
        if let Some(operation) = shell
            .canvas_checklist
            .comment_operations
            .operations
            .iter_mut()
            .find(|operation| {
                operation.attempt.serial == attempt.serial && operation.attempt.disk == attempt.disk
            })
        {
            operation.receipt = Some(receipt.clone());
        }
    }
    if !current(&state.peek(), &model.peek(), &attempt) {
        known_unsent(state, model, &attempt, receipt).await;
        return;
    }
    let result = if crate::fixture::is_fixture() {
        if !stage(
            &mut state.write(),
            &attempt,
            Stage::Preparing,
            Stage::Sending,
        ) {
            return;
        }
        fixture::execute(&state.peek(), &attempt)
    } else {
        let Some(ctx) = attempt.owner.scope.request(&state.peek()) else {
            known_unsent(state, model, &attempt, receipt).await;
            return;
        };
        if !stage(
            &mut state.write(),
            &attempt,
            Stage::Preparing,
            Stage::Sending,
        ) {
            return;
        }
        api::execute_canvas_comment_archive(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            attempt.intent.clone(),
        )
        .await
    };
    match result {
        Err(CanvasEditFailure::NotSent(_)) => known_unsent(state, model, &attempt, receipt).await,
        Err(CanvasEditFailure::Uncertain(error)) => {
            remember_error(state, model, &attempt, &error);
            stage(
                &mut state.write(),
                &attempt,
                Stage::Sending,
                Stage::Uncertain,
            );
            feedback(state, model, &attempt, Some(LOCKED));
        }
        Ok(accepted) => {
            stage(
                &mut state.write(),
                &attempt,
                Stage::Sending,
                Stage::AwaitingRead,
            );
            let accepted = std::sync::Arc::new(accepted);
            {
                let mut shell = state.write();
                if let Some(operation) = shell
                    .canvas_checklist
                    .comment_operations
                    .operations
                    .iter_mut()
                    .find(|operation| {
                        operation.attempt.serial == attempt.serial
                            && operation.attempt.disk == attempt.disk
                    })
                {
                    operation.accepted = Some(accepted.clone());
                }
            }
            finish(state, model, &attempt, receipt, accepted).await;
        }
    }
}
async fn known_unsent(
    mut state: Signal<ShellState>,
    model: Signal<super::super::State>,
    attempt: &Attempt,
    receipt: CommentReceipt,
) {
    {
        let mut shell = state.write();
        if let Some(operation) = shell
            .canvas_checklist
            .comment_operations
            .operations
            .iter_mut()
            .find(|operation| {
                operation.attempt.serial == attempt.serial && operation.attempt.disk == attempt.disk
            })
        {
            operation.stage = Stage::NotSent;
        }
    }
    let result = attempt
        .disk
        .work(move |journal| journal.complete_comment(&receipt))
        .await;
    if result == Ok(true) {
        retire(&mut state.write(), attempt);
        feedback(
            state,
            model,
            attempt,
            Some("The thread change was not sent. Refresh comments before trying again."),
        );
    } else {
        feedback(state, model, attempt, Some(STORAGE));
    }
}
async fn finish(
    mut state: Signal<ShellState>,
    model: Signal<super::super::State>,
    attempt: &Attempt,
    receipt: CommentReceipt,
    accepted: std::sync::Arc<ArchiveAccepted>,
) {
    let confirmed = match canonical(state, model, attempt, ReadPurpose::Settlement).await {
        Ok((file, root)) => accepted.readback(&file, &root).is_ok(),
        Err(error) => {
            remember_error(state, model, attempt, &error);
            false
        }
    };
    if !confirmed {
        feedback(
            state,
            model,
            attempt,
            Some(
                "Slack acknowledged the thread change, but its current state could not be checked. Refresh comments; it will not be sent again.",
            ),
        );
        return;
    }
    let completed = attempt
        .disk
        .work(move |journal| journal.complete_comment(&receipt))
        .await;
    if completed != Ok(true) {
        feedback(state, model, attempt, Some(STORAGE));
        return;
    }
    retire(&mut state.write(), attempt);
    if attempt.owner.current(&state.peek()) && model.peek().serial == attempt.read {
        let query = model.peek().query.clone();
        if let Some(query) = query {
            super::super::start(state, model, query, false);
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum ReadPurpose {
    Preflight,
    Settlement,
}
impl ReadPurpose {
    pub(super) fn current(
        self,
        shell: &ShellState,
        model: &super::super::State,
        attempt: &Attempt,
    ) -> bool {
        match self {
            Self::Preflight => current(shell, model, attempt),
            // A dispatched write retains settlement custody after dialog retirement.
            // Its original exact account credentials must still be current.
            Self::Settlement => attempt.owner.scope.current(shell),
        }
    }
}
fn invalid() -> Error {
    Error::Api("canvas_comment_owner_changed".into())
}

/// Guards both dispatch boundaries, not merely adoption of the final response.
/// A retired preflight cannot issue its second read after files.info completes.
pub(super) async fn guarded_reads<A, B, F, G, H>(
    current: impl Fn() -> bool,
    first: F,
    second: G,
) -> Result<(A, B), Error>
where
    F: std::future::Future<Output = Result<A, Error>>,
    G: FnOnce() -> H,
    H: std::future::Future<Output = Result<B, Error>>,
{
    if !current() {
        return Err(invalid());
    }
    let a = first.await?;
    if !current() {
        return Err(invalid());
    }
    let b = second().await?;
    if !current() {
        return Err(invalid());
    }
    Ok((a, b))
}

pub(super) fn remember_rate_error(
    shell: &ShellState,
    attempt: &Attempt,
    error: &Error,
) -> Option<super::super::rate::Wait> {
    let Error::RateLimited { retry_after_secs } = error else {
        return None;
    };
    if !attempt.owner.scope.current(shell) {
        return None;
    }
    super::super::rate::remember(shell, *retry_after_secs);
    super::super::rate::current(shell)
}
fn remember_error(
    state: Signal<ShellState>,
    model: Signal<super::super::State>,
    attempt: &Attempt,
    error: &Error,
) {
    let wait = remember_rate_error(&state.peek(), attempt, error);
    if attempt.owner.current(&state.peek()) && model.peek().serial == attempt.read {
        if let Some(wait) = wait {
            super::super::set_wait(state, model, attempt.owner.clone(), wait);
        }
    }
}
async fn canonical(
    state: Signal<ShellState>,
    model: Signal<super::super::State>,
    attempt: &Attempt,
    purpose: ReadPurpose,
) -> Result<(File, Message), Error> {
    let current = || purpose.current(&state.peek(), &model.peek(), attempt);
    if !current() {
        return Err(invalid());
    }
    if crate::fixture::is_fixture() {
        return fixture::canonical(&state.peek(), attempt);
    }
    let ctx = attempt
        .owner
        .scope
        .request(&state.peek())
        .ok_or_else(invalid)?;
    let (file, raw) = guarded_reads(
        &current,
        api::fetch_canvas_file(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            &attempt.owner.file,
        ),
        || async {
            let request = api::canvas_comment_replies(
                &ctx.client,
                &ctx.session,
                &attempt.owner.file,
                attempt.intent.root_ts(),
                None,
            )?;
            ctx.transport.execute(request).await
        },
    )
    .await?;
    let file_id = attempt.owner.file.clone();
    let root_ts = attempt.intent.root_ts().to_owned();
    let page =
        tokio::task::spawn_blocking(move || ReplyPage::parse(&file_id, &root_ts, true, &raw))
            .await
            .map_err(|_| invalid())?
            .map_err(|_| invalid())?;
    if !current() {
        return Err(invalid());
    }
    let root = page
        .messages
        .into_iter()
        .find(|message| message.ts.as_deref() == Some(attempt.intent.root_ts()))
        .ok_or_else(invalid)?;
    Ok((file, root))
}
