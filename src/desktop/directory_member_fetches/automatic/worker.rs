use super::*;
use std::time::Duration;
use super_platinum_core::slack::realtime::{MemberSubscription, MemberSubscriptionError};

struct Guard {
    state: Signal<ShellState>,
    wake: Wake,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut state = self.state;
        let wake = self.wake.clone();
        let release = move |shell: &mut ShellState| {
            if !wake.current(shell) {
                return;
            }
            let r = shell
                .directory_member_fetches
                .automatic
                .slots
                .get_mut(&wake.context.key())
                .unwrap();
            for cycle in r.manager.cycles() {
                let _ = r.manager.return_unsent(&r.scope, &cycle);
            }
            r.worker = None;
            r.finished = r.started;
            r.notify();
        };
        if let Ok(mut shell) = state.try_write() {
            release(&mut shell);
        } else {
            dioxus::core::spawn_forever(async move {
                release(&mut state.write());
            });
        }
    }
}
pub(super) async fn run(mut state: Signal<ShellState>, wake: Wake) {
    let _guard = Guard {
        state,
        wake: wake.clone(),
    };
    let Ok(subscription) = cfm_lifecycle::subscribe(&state.peek(), &wake.context.session.team_id)
    else {
        return;
    };
    let mut lifecycle = subscription.changes();
    loop {
        let step = advance(&mut state.write(), &wake);
        match step {
            Ok(Step::Done) | Err(_) => return,
            Ok(Step::Wait(delay, mut changes)) => {
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {},
                    _ = changes.changed() => {},
                    _ = lifecycle.changed() => {},
                }
            }
            Ok(Step::Operation(cycle, op, context, connection)) => {
                let original = context.clone();
                if execute(
                    state,
                    &wake,
                    &cycle,
                    &op,
                    context,
                    connection,
                    &mut lifecycle,
                )
                .await
                .is_err()
                {
                    let shell = state.peek();
                    // The accepted full reconnect already retired the original
                    // tokens and returned IDs. Continue that same account owner.
                    let successor = wake.current(&shell)
                        && !original.current(&shell)
                        && socket(&shell, &original.session.team_id).is_ok();
                    drop(shell);
                    if successor {
                        count(state, &wake, |r| &mut r.finished);
                    } else {
                        return;
                    }
                }
            }
        }
    }
}
enum Step {
    Done,
    Wait(Duration, tokio::sync::watch::Receiver<u64>),
    Operation(model::Cycle, model::Operation, Arc<Context>, Connection),
}
fn advance(shell: &mut ShellState, wake: &Wake) -> Result<Step, Error> {
    if !wake.current(shell) {
        return Err(unavailable());
    }
    let team = &wake.context.session.team_id;
    let observation = facts(shell, team)?;
    let at = now(shell)?;
    let context = Arc::new(Context::capture(shell, team)?);
    let connection = socket(shell, team)?;
    let r = shell
        .directory_member_fetches
        .automatic
        .slots
        .get_mut(&wake.context.key())
        .unwrap();
    r.manager.begin_due(&r.scope, at, observation)?;
    for cycle in r.manager.cycles() {
        if r.manager.parent_active(&r.scope, &cycle) && at >= cycle.parent_deadline_ms() {
            r.manager
                .finish_parent(&r.scope, &cycle, at, model::ParentOutcome::Timeout)?;
            r.notify();
            if !r.manager.cycle_current(&r.scope, &cycle) {
                continue;
            }
        }
        if r.manager
            .chunk_deadline_ms(&r.scope, &cycle)?
            .is_some_and(|due| at >= due)
        {
            if r.manager.timeout_chunk(&r.scope, &cycle, at).is_err() {
                r.manager.return_unsent(&r.scope, &cycle)?;
                r.notify();
                continue;
            }
            r.notify();
        }
        // Reserve actual cadence once, then retain the same operation token.
        let entropy = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .subsec_nanos() as f64)
            / 1_000_000_000.0;
        if let Some(op) = r.manager.next_operation(&r.scope, &cycle, at, entropy)? {
            return Ok(Step::Operation(cycle, op, context, connection));
        }
        if r.manager.cycle_done(&r.scope, &cycle)? {
            r.manager
                .finish_parent(&r.scope, &cycle, at, model::ParentOutcome::Completed)?;
            r.notify();
        }
    }
    match r.manager.next_wake_ms() {
        Some(due) => Ok(Step::Wait(
            Duration::from_millis(due.saturating_sub(at)),
            r.changed.subscribe(),
        )),
        None => Ok(Step::Done),
    }
}
fn current(shell: &ShellState, wake: &Wake, context: &Context, op: &model::Operation) -> bool {
    wake.current(shell)
        && context.current(shell)
        && socket(shell, &context.session.team_id).is_ok()
        && mode::observed(shell, &context.session.team_id).is_ok()
        && shell
            .directory_member_fetches
            .automatic
            .slots
            .get(&context.key())
            .is_some_and(|r| r.manager.operation_current(&r.scope, op))
}
fn count(mut state: Signal<ShellState>, wake: &Wake, field: fn(&mut Runtime) -> &mut u64) {
    let mut shell = state.write();
    if wake.current(&shell) {
        let r = shell
            .directory_member_fetches
            .automatic
            .slots
            .get_mut(&wake.context.key())
            .unwrap();
        *field(r) += 1;
        r.notify();
    }
}
async fn receipt(
    state: Signal<ShellState>,
    wake: &Wake,
    context: &Context,
    op: &model::Operation,
    connection: Connection,
    initial: MemberSubscription,
) -> Result<(), Failure> {
    let mut pending = Some(initial);
    for retry in 0..=3 {
        if !current(&state.peek(), wake, context, op) {
            return Err(Failure::Refused);
        }
        let request = match pending.take() {
            Some(initial) => initial,
            None => connection
                .request_member_subscription(op.ids(), op.kind() == model::Kind::Subscribe)
                .map_err(|_| Failure::Refused)?,
        };
        let result = request.wait().await;
        match result {
            Ok(_) => {
                count(state, wake, |r| &mut r.acknowledgments);
                return Ok(());
            }
            Err(MemberSubscriptionError::SocketClosed) if retry < 3 => {}
            Err(MemberSubscriptionError::Rejected(reply))
                if retry < 3 && reply.reason() == "socket_closed" => {}
            Err(_) => return Err(Failure::Failed),
        }
    }
    Err(Failure::Failed)
}
async fn execute(
    mut state: Signal<ShellState>,
    wake: &Wake,
    cycle: &model::Cycle,
    op: &model::Operation,
    context: Arc<Context>,
    connection: Connection,
    lifecycle: &mut tokio::sync::watch::Receiver<u64>,
) -> Result<(), Error> {
    let (mut changes, initial) = {
        let mut shell = state.write();
        if !current(&shell, wake, &context, op) {
            return Err(unavailable());
        }
        if op.kind() == model::Kind::Subscribe {
            super::super::known::automatic_admissible(
                &mut shell,
                &context.session.team_id,
                op.ids(),
            )?;
        }
        let at = now(&shell)?;
        let initial = connection
            .request_member_subscription(op.ids(), op.kind() == model::Kind::Subscribe)
            .map_err(|_| unavailable())?;
        let r = shell
            .directory_member_fetches
            .automatic
            .slots
            .get_mut(&wake.context.key())
            .unwrap();
        r.manager.started(&r.scope, op, at)?;
        r.started += 1;
        r.notify();
        (r.changed.subscribe(), initial)
    };
    let subscription = receipt(state, wake, &context, op, connection, initial);
    let operation_state = state;
    let operation = async {
        if op.kind() == model::Kind::Subscribe {
            super::super::known::refresh(
                operation_state,
                &context.session.team_id,
                op.ids(),
                |s| current(s, wake, &context, op),
                subscription,
                || count(operation_state, wake, |r| &mut r.http_finished),
            )
            .await
        } else {
            subscription.await
        }
    };
    tokio::pin!(operation);
    let mut parent_finished = {
        let shell = state.peek();
        let r = &shell.directory_member_fetches.automatic.slots[&wake.context.key()];
        !r.manager.parent_active(&r.scope, cycle)
    };
    let result = loop {
        let at = now(&state.peek())?;
        let deadline = op
            .deadline_ms()
            .into_iter()
            .chain((!parent_finished).then_some(cycle.parent_deadline_ms()))
            .min();
        let timeout = async {
            match deadline {
                Some(due) => {
                    tokio::time::sleep(Duration::from_millis(due.saturating_sub(at))).await
                }
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            result = &mut operation => break Some(result),
            _ = changes.changed() => {},
            _ = lifecycle.changed() => {
                let mut shell = state.write();
                if !wake.current(&shell) { return Err(unavailable()); }
                let observation = facts(&mut shell, &context.session.team_id)?;
                let at = now(&shell)?;
                let r = shell.directory_member_fetches.automatic.slots.get_mut(&wake.context.key()).unwrap();
                r.manager.observe(&r.scope, at, observation)?;
                r.notify();
            },
            _ = timeout => {
                let mut shell = state.write();
                if !current(&shell, wake, &context, op) { return Err(unavailable()); }
                let at = now(&shell)?;
                let r = shell.directory_member_fetches.automatic.slots.get_mut(&wake.context.key()).unwrap();
                if !parent_finished && at >= cycle.parent_deadline_ms() {
                    r.manager.finish_parent(&r.scope, cycle, at, model::ParentOutcome::Timeout)?;
                    parent_finished = true;
                    r.notify();
                }
                if op.deadline_ms().is_some_and(|due| at >= due) {
                    r.manager.timeout_chunk(&r.scope, cycle, at)?;
                    break None;
                }
            }
        }
        if !current(&state.peek(), wake, &context, op) {
            return Err(unavailable());
        }
    };
    let mut shell = state.write();
    if !wake.current(&shell) || !context.current(&shell) {
        return Err(unavailable());
    }
    let at = now(&shell)?;
    let r = shell
        .directory_member_fetches
        .automatic
        .slots
        .get_mut(&wake.context.key())
        .unwrap();
    if matches!(result, Some(Err(Failure::Refused))) {
        return Err(unavailable());
    }
    let success = result.as_ref().is_some_and(|r| r.is_ok());
    if result.is_some() {
        r.manager.settle(
            &r.scope,
            op,
            at,
            if success {
                model::Outcome::Success
            } else {
                model::Outcome::Failure
            },
        )?;
    }
    r.finished += 1;
    if !success {
        r.failures += 1;
    }
    r.notify();
    Ok(())
}
