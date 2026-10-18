//! Closed fixture/configured Slash context enters the existing renderer Root.
//! Native custody and the actual owned worker are shared by both origins.
use super::*;
use crate::drafts::protected::{
    NativeClear,
    operation::{Operation, Phase, Request, Seed, Status},
};
use super_platinum_core::drafts::store::protected::{self as durable};
use super_platinum_core::slash_commands::catalog::Choice;

pub(crate) struct StartSlash {
    pub state: Signal<ShellState>,
    pub target: ComposerTarget,
    pub registry: Arc<Registry>,
    pub editor: Arc<Editor>,
    pub visit: Visit,
    pub context: Arc<crate::drafts::protected::owned_slash::Context>,
    pub stream: durable::Stream,
    pub choice: Choice,
    /// Existing app-modal origin token; this adapter never mints/replaces one.
    pub action: crate::app_modal::ActionToken,
}
pub(crate) struct RefusedSlash {
    pub error: Error,
    pub request: StartSlash,
    captured: Option<Captured>,
}
pub(crate) fn start_slash(
    root: &super::super::protected_operations::Root,
    input: StartSlash,
) -> Result<Operation, RefusedSlash> {
    let input = root
        .admit_request(input)
        .map_err(|(error, request)| RefusedSlash {
            error,
            request,
            captured: None,
        })?;
    let captured = match capture(
        input.state,
        input.target,
        input.registry.clone(),
        input.editor.clone(),
        input.visit.clone(),
        false,
    ) {
        Ok(value) => value,
        Err(error) => {
            return Err(RefusedSlash {
                error,
                request: input,
                captured: None,
            });
        }
    };
    if input.context.scope() != input.editor.scope()
        || !input.stream.is_live()
        || input.action.token.is_empty()
        || input.action.token.len() > 256
        || !input
            .state
            .try_peek()
            .is_ok_and(|s| crate::app_modal::action_scope_current(&s, &input.action))
        || !captured.native.mount.watch_slash(&input.context)
        || !context_current(&captured.native, &input.context, &input.stream)
    {
        input.context.revoke();
        return Err(RefusedSlash {
            error: Error::Stale,
            request: input,
            captured: Some(captured),
        });
    }
    let operation = match Operation::reserve(
        &input.editor,
        Seed {
            request: Request::Slash {
                context: input.context.clone(),
                choice: input.choice.clone(),
                client_token: input.action.token.clone(),
            },
            local: captured.native.local.clone(),
            book: captured.native.book.clone(),
            serial: captured.native.serial,
            epoch: captured.native.epoch,
        },
    ) {
        Ok(value) => value,
        Err(refused) => {
            input.context.revoke();
            return Err(RefusedSlash {
                error: refused.error,
                request: input,
                captured: Some(captured),
            });
        }
    };
    let owned = operation.clone();
    let _ = root.install(
        operation.clone(),
        Box::pin(async move {
            let result = run(
                captured,
                owned,
                input.context.clone(),
                input.stream,
                &input.action,
            )
            .await;
            if input.context.definitely_unattempted() {
                let mut state = input.state;
                if let Ok(mut shell) = state.try_write() {
                    crate::app_modal::reject_action(&mut shell, &input.action);
                }
            }
            input.context.revoke();
            result
        }),
    );
    // Installation refusal is recorded Unknown by the existing armed Root guard.
    Ok(operation)
}
fn context_current(
    native: &NativeVisit,
    context: &crate::drafts::protected::owned_slash::Context,
    stream: &durable::Stream,
) -> bool {
    context.is_live()
        && stream.is_live()
        && stream.account() == native.scope.account_id
        && stream.team() == native.scope.key.team
        && native
            .state
            .try_peek()
            .is_ok_and(|s| native.current(&s) && context.current(&s, stream))
}
async fn run(
    captured: Captured,
    parent: Operation,
    context: Arc<crate::drafts::protected::owned_slash::Context>,
    stream: durable::Stream,
    action: &crate::app_modal::ActionToken,
) -> Result<Status, Error> {
    let Prepared {
        lease,
        native,
        evidence,
        ..
    } = prepare(captured, parent.clone()).await?;
    let original = evidence.snapshot().clone();
    parent.phase(Phase::NativeAdmission)?;
    let admission = Rc::new(lock_evidence(evidence, &native, &parent, &context).await?);
    let check = admission.clone();
    let current = native.clone();
    let f = context.clone();
    let s = stream.clone();
    let operation = parent.clone();
    let child = native
        .editor
        .prepare_slash(lease, &parent, move || {
            if !operation.current() || !check.current() || !context_current(&current, &f, &s) {
                f.revoke();
                return Err(Error::Stale);
            }
            Ok(s.clone())
        })
        .await?;
    drop(admission);
    parent.phase(Phase::Prepared)?;
    child.prepare_dispatch().await?;
    if !native
        .state
        .try_peek()
        .is_ok_and(|s| crate::app_modal::action_scope_current(&s, action))
    {
        context.revoke();
        return Err(Error::Stale);
    }
    // Storage awaits are over. A NEW exact native proof precedes actual arm.
    let dispatch = fresh_lock(&native, &parent, &context, &stream, &original).await?;
    if !native.editor.slash_current(&child) {
        context.revoke();
        return Err(Error::Stale);
    }
    parent.phase(Phase::Http)?;
    let execute = child.execute();
    tokio::pin!(execute);
    let mut short_lease = Some(dispatch);
    // execute's first poll performs actual worker arm and final admission without
    // an intervening await. Release the short readOnly lease after this poll,
    // including when HeldClear reaches Pending. It is never an HTTP-duration lock.
    std::future::poll_fn(|cx| {
        if let Some(lease) = &short_lease {
            if !parent.current()
                || !lease.current()
                || !native.editor.slash_current(&child)
                || !context_current(&native, &context, &stream)
                || !native
                    .state
                    .try_peek()
                    .is_ok_and(|s| crate::app_modal::action_scope_current(&s, action))
            {
                context.revoke();
                short_lease.take();
                return std::task::Poll::Ready(Err(Error::Stale));
            }
        }
        let polled = std::future::Future::poll(execute.as_mut(), cx);
        short_lease.take();
        polled
    })
    .await?;
    // Real completion wait/accounting succeeded. Canonical and native projection
    // remain independent evidence, including after an operation was retired.
    let after = native.editor.slash_canonical(&child).await?;
    parent.retain(|s| s.recorded = Some(Arc::new(after.clone())))?;
    if !child.has_fresh_clear()? {
        if child.acknowledged_without_clear() {
            let keep = fresh_lock(&native, &parent, &context, &stream, &original).await?;
            native.editor.reconcile_slash_keep(&child, &after, || {
                parent.current() && keep.current() && context_current(&native, &context, &stream)
            })?;
            child.native_result(NativeClear::NotAttempted, true)?;
            return Ok(Status::Finished);
        }
        return Ok(Status::AwaitingRecovery);
    }
    if !after.local().empty() || !native.editor.slash_current(&child) {
        child.native_result(NativeClear::Deferred, false)?;
        return Ok(Status::AwaitingRecovery);
    }
    let clear = match fresh_lock(&native, &parent, &context, &stream, &original).await {
        Ok(value) => value,
        Err(_) => {
            child.native_result(NativeClear::Deferred, false)?;
            return Ok(Status::AwaitingRecovery);
        }
    };
    if !native.editor.slash_current(&child) || !parent.current() || !clear.current() {
        child.native_result(NativeClear::Deferred, false)?;
        return Ok(Status::AwaitingRecovery);
    }
    child.consume_fresh_clear()?;
    parent.phase(Phase::NativeClear)?;
    let cleared = match clear.clear().await {
        super::super::protected_admission::Clear::Applied => {
            parent.current()
                && native
                    .editor
                    .reconcile_slash(&child, &after, || {
                        let mut state = native.state;
                        let Ok(mut shell) = state.try_write() else {
                            return false;
                        };
                        if !native.current(&shell) || !stream.is_live() {
                            return false;
                        }
                        if shell
                            .core
                            .drafts
                            .project_clean(&native.scope.key, native.book.as_deref(), after.local())
                            .is_err()
                        {
                            return false;
                        }
                        match native.target {
                            ComposerTarget::Channel => {
                                shell.core.composer = after.local().composer.clone()
                            }
                            ComposerTarget::Thread => {
                                shell.core.thread_composer = after.local().composer.clone()
                            }
                        }
                        true
                    })
                    .is_ok()
        }
        super::super::protected_admission::Clear::Changed(current) => {
            parent.native(&current, true)?;
            parent.preservation(native.preserve_changed(current, child.input.serial()).await)?;
            false
        }
        _ => false,
    };
    child.native_result(
        if cleared {
            NativeClear::Cleared
        } else {
            NativeClear::Deferred
        },
        cleared,
    )?;
    Ok(if cleared {
        Status::Finished
    } else {
        Status::AwaitingRecovery
    })
}
async fn fresh_lock(
    native: &Rc<NativeVisit>,
    parent: &Operation,
    context: &crate::drafts::protected::owned_slash::Context,
    stream: &durable::Stream,
    original: &super::super::protected_projection::NativeSnapshot,
) -> Result<super::super::protected_admission::Lease, Error> {
    let evidence = match super::super::protected_admission::Evidence::capture(
        &original.id,
        &native.mount,
        &original.wire,
    )
    .await
    {
        Ok(value) => value,
        Err(super::super::protected_admission::Failure::Changed(current)) => {
            parent.native(&current, true)?;
            parent.preservation(native.preserve_changed(current, native.serial).await)?;
            context.revoke();
            return Err(Error::ReviewRequired);
        }
        Err(_) => {
            context.revoke();
            return Err(Error::Stale);
        }
    };
    if evidence.snapshot() != original
        || !parent.current()
        || !context_current(native, context, stream)
    {
        context.revoke();
        return Err(Error::Stale);
    }
    let lease = lock_evidence(evidence, native, parent, context).await?;
    if !parent.current() || !context_current(native, context, stream) || !lease.current() {
        context.revoke();
        return Err(Error::Stale);
    }
    Ok(lease)
}

// Capture and lock are separate observations. A change between them still has
// input custody and must reach the existing guarded preservation path.
async fn lock_evidence(
    evidence: super::super::protected_admission::Evidence,
    native: &Rc<NativeVisit>,
    parent: &Operation,
    context: &crate::drafts::protected::owned_slash::Context,
) -> Result<super::super::protected_admission::Lease, Error> {
    match evidence.lock(native.mount.clone()).await {
        Ok(lease) => Ok(lease),
        Err(super::super::protected_admission::Failure::Changed(current)) => {
            context.revoke();
            parent.native(&current, true)?;
            parent.preservation(native.preserve_changed(current, native.serial).await)?;
            Err(Error::ReviewRequired)
        }
        Err(_) => {
            context.revoke();
            Err(Error::Stale)
        }
    }
}
