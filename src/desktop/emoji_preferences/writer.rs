//! Same async algorithm for Native and isolated tests. Unsent visits rebase;
//! dispatched visits never replay after an uncertain response.
use super::*;
use boundary::Work;
use std::{
    future::{Future, poll_fn},
    task::Poll,
};
use super_platinum_core::{composer_emoji::history::History, slack::Error};

// Reserve ACK order synchronously, including before the futures' first polls.
// Weak slots let cancellation retire only the completed prefix: cancelling B
// while A runs cannot release C ahead of A.
#[derive(Clone, Default)]
pub(super) struct Order(
    std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<std::sync::Weak<OrderNode>>>>,
);
#[derive(Default)]
struct OrderNode(std::sync::Mutex<Option<std::task::Waker>>);
struct Turn {
    order: Order,
    node: Option<std::sync::Arc<OrderNode>>,
}
impl Order {
    fn reserve(&self) -> Turn {
        let node = std::sync::Arc::new(OrderNode::default());
        self.0
            .lock()
            .unwrap()
            .push_back(std::sync::Arc::downgrade(&node));
        Turn {
            order: self.clone(),
            node: Some(node),
        }
    }
}
impl Turn {
    async fn wait(&self) {
        poll_fn(|cx| {
            let node = self.node.as_ref().unwrap();
            let ready = self
                .order
                .0
                .lock()
                .unwrap()
                .front()
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|first| std::sync::Arc::ptr_eq(&first, node));
            if ready {
                Poll::Ready(())
            } else {
                *node.0.lock().unwrap() = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}
impl Drop for Turn {
    fn drop(&mut self) {
        drop(self.node.take());
        let wake = {
            let mut queue = self.order.0.lock().unwrap();
            while queue.front().is_some_and(|n| n.strong_count() == 0) {
                queue.pop_front();
            }
            queue
                .front()
                .and_then(std::sync::Weak::upgrade)
                .and_then(|node| node.0.lock().unwrap().take())
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

fn updated(value: Option<Value>, visits: Vec<Visit>) -> Result<String, String> {
    let mut history = match value {
        Some(value) => History::from_pref(&value)?,
        None => History::default(),
    };
    for visit in visits {
        history = history.recorded(&visit.usage.id, &visit.usage.query, visit.now_ms)?;
    }
    history.to_wire()
}

pub(super) fn record_with<B: Boundary>(
    boundary: B,
    mut state: Signal<ShellState>,
    usage: Usage,
) -> impl Future<Output = ()> {
    // Freeze owner and event time now, before the caller schedules first poll.
    let owner = Owner::capture(&state.peek()).map(Arc::new);
    let reserved = owner.as_ref().map(|owner| {
        let mut shell = state.write();
        let team = owner.context.session.team_id.clone();
        let writers = &mut shell.emoji_preferences.writers;
        if writers.get(&team).is_none_or(|w| !w.owner.same(owner)) {
            writers.insert(team.clone(), Writer::new(owner.clone()));
        }
        let writer = &writers[&team];
        (writer.owner.clone(), writer.order.reserve())
    });
    let visit = Visit {
        usage,
        now_ms: boundary.now_ms(),
    };
    async move {
        let Some((owner, turn)) = reserved else {
            return;
        };
        turn.wait().await;
        let team = owner.context.session.team_id.clone();
        loop {
            let (revision, seq, value, visits) = {
                let shell = state.peek();
                if !current(&shell, &owner) {
                    return;
                }
                let prefs = &shell.core.workspaces[&team].emoji_preferences;
                if !prefs.loaded() {
                    return;
                }
                let writer = shell
                    .emoji_preferences
                    .writers
                    .get(&team)
                    .filter(|w| w.owner.same(&owner));
                let mut visits = writer
                    .filter(|w| w.latest.as_deref() != prefs.frecency().and_then(Value::as_str))
                    .map(|w| w.pending.clone())
                    .unwrap_or_default();
                visits.push(visit.clone());
                (
                    prefs.revision(),
                    writer.map(|w| w.seq),
                    prefs.frecency().cloned(),
                    visits,
                )
            };
            let Ok(Ok(wire)) = boundary
                .work(Work::History, move || updated(value, visits))
                .await
            else {
                return;
            };
            let start = {
                let mut shell = state.write();
                if !current(&shell, &owner) {
                    return;
                }
                let prefs = &shell.core.workspaces[&team].emoji_preferences;
                let current = shell
                    .emoji_preferences
                    .writers
                    .get(&team)
                    .filter(|w| w.owner.same(&owner));
                if prefs.revision() != revision || current.map(|w| w.seq) != seq {
                    continue;
                }
                let prefs = &mut shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .emoji_preferences;
                prefs.apply_change("frecency", Value::String(wire.clone()));
                let applied = prefs.revision();
                let writers = &mut shell.emoji_preferences.writers;
                if writers.get(&team).is_none_or(|w| !w.owner.same(&owner)) {
                    writers.insert(team.clone(), Writer::new(owner.clone()));
                }
                let writer = writers.get_mut(&team).unwrap();
                writer.pending.push(visit.clone());
                writer.latest = Some(wire);
                writer.applied_revision = applied;
                writer.seq = writer.seq.wrapping_add(1);
                let start = !writer.running;
                writer.running = true;
                start
            };
            boundary.persist(state, &team);
            if start {
                boundary
                    .clone()
                    .spawn(Box::pin(flush(boundary.clone(), state, owner.clone())));
            }
            return;
        }
    }
}

async fn flush<B: Boundary>(boundary: B, mut state: Signal<ShellState>, owner: Arc<Owner>) {
    let team = owner.context.session.team_id.clone();
    loop {
        let captured = {
            let mut shell = state.write();
            if !current(&shell, &owner) {
                shell.emoji_preferences.retire_writer(&owner);
                return;
            }
            let prefs = &shell.core.workspaces[&team].emoji_preferences;
            let revision = prefs.revision();
            let value = prefs.frecency().cloned();
            let loaded = prefs.loaded();
            let Some(writer) = shell
                .emoji_preferences
                .writers
                .get_mut(&team)
                .filter(|w| w.owner.same(&owner))
            else {
                return;
            };
            if writer.pending.is_empty() || writer.blocked || !loaded {
                writer.running = false;
                return;
            }
            (
                writer.wait(boundary.now()),
                revision,
                writer.seq,
                value,
                writer.applied_revision,
                writer.pending.clone(),
                writer.latest.clone(),
            )
        };
        let (wait, revision, seq, value, applied, visits, wire) = captured;
        if !wait.is_zero() {
            boundary.sleep(wait).await;
            continue;
        }
        if applied != revision {
            // Tone/mode/emoji_use share the Preferences revision. An unchanged
            // frecency wire already includes our unsent visits; rebasing it
            // would count those visits twice.
            if wire.is_some() && wire.as_deref() == value.as_ref().and_then(Value::as_str) {
                let mut shell = state.write();
                if !current(&shell, &owner) {
                    shell.emoji_preferences.retire_writer(&owner);
                    return;
                }
                if shell.core.workspaces[&team].emoji_preferences.revision() != revision {
                    continue;
                }
                if let Some(writer) = shell
                    .emoji_preferences
                    .writers
                    .get_mut(&team)
                    .filter(|w| w.owner.same(&owner) && w.seq == seq)
                {
                    writer.applied_revision = revision;
                }
                continue;
            }
            let Ok(Ok(wire)) = boundary
                .work(Work::History, move || updated(value, visits))
                .await
            else {
                stop(state, &owner);
                return;
            };
            let mut shell = state.write();
            if !current(&shell, &owner) {
                shell.emoji_preferences.retire_writer(&owner);
                return;
            }
            if shell.core.workspaces[&team].emoji_preferences.revision() != revision
                || shell
                    .emoji_preferences
                    .writers
                    .get(&team)
                    .is_none_or(|w| !w.owner.same(&owner) || w.seq != seq)
            {
                continue;
            }
            let prefs = &mut shell
                .core
                .workspaces
                .get_mut(&team)
                .unwrap()
                .emoji_preferences;
            prefs.apply_change("frecency", Value::String(wire.clone()));
            let applied = prefs.revision();
            let writer = shell.emoji_preferences.writers.get_mut(&team).unwrap();
            writer.latest = Some(wire);
            writer.applied_revision = applied;
            drop(shell);
            boundary.persist(state, &team);
            continue;
        }
        let Some(wire) = wire else {
            stop(state, &owner);
            return;
        };
        let request_owner = owner.clone();
        let request = boundary
            .work(Work::Request, move || {
                api::frecency_request(
                    &request_owner.context.client,
                    &request_owner.context.session,
                    wire,
                )
            })
            .await;
        let Ok(Ok(request)) = request else {
            stop(state, &owner);
            return;
        };
        {
            let mut shell = state.write();
            if !current(&shell, &owner) {
                shell.emoji_preferences.retire_writer(&owner);
                return;
            }
            if shell.core.workspaces[&team].emoji_preferences.revision() != revision {
                continue;
            }
            let Some(writer) = shell
                .emoji_preferences
                .writers
                .get_mut(&team)
                .filter(|w| w.owner.same(&owner))
            else {
                return;
            };
            if writer.seq != seq || writer.applied_revision != revision {
                continue;
            }
            if writer.blocked {
                writer.running = false;
                return;
            }
            if !writer.wait(boundary.now()).is_zero() {
                continue;
            }
            writer.pending.clear();
            writer.latest = None;
            writer.seq = writer.seq.wrapping_add(1);
            writer.last_dispatch = Some(boundary.now());
            writer.running = false;
        }
        // Final validation and first executor poll are one serial task turn.
        // Detach only the already-started response, never an unpolled mutation.
        let run_boundary = boundary.clone();
        let run_owner = owner.clone();
        let mut response = Box::pin(async move {
            run_boundary
                .execute(run_owner, request)
                .await
                .and_then(|v| api::success(&v))
        });
        match poll_fn(|cx| Poll::Ready(response.as_mut().poll(cx))).await {
            Poll::Ready(result) => finish(&boundary, state, &owner, result),
            Poll::Pending => boundary.clone().spawn(Box::pin(async move {
                let result = response.await;
                finish(&boundary, state, &owner, result);
            })),
        }
        return;
    }
}

fn current(shell: &ShellState, owner: &Arc<Owner>) -> bool {
    owner.current(shell)
        && shell
            .emoji_preferences
            .writers
            .get(&owner.context.session.team_id)
            .is_some_and(|w| Arc::ptr_eq(&w.owner, owner))
}
fn stop(mut state: Signal<ShellState>, owner: &Arc<Owner>) {
    let mut shell = state.write();
    if !current(&shell, &owner) {
        shell.emoji_preferences.retire_writer(owner);
        return;
    }
    if let Some(writer) = shell
        .emoji_preferences
        .writers
        .get_mut(&owner.context.session.team_id)
        .filter(|w| w.owner.same(owner))
    {
        writer.running = false;
    }
}
fn finish<B: Boundary>(
    boundary: &B,
    mut state: Signal<ShellState>,
    owner: &Arc<Owner>,
    result: Result<(), Error>,
) {
    if !current(&state.peek(), &owner) {
        return;
    }
    if let Err(error) = result {
        if let Some(delay) = retry_delay(&error) {
            let mut shell = state.write();
            if let Some(writer) = shell
                .emoji_preferences
                .writers
                .get_mut(&owner.context.session.team_id)
                .filter(|w| w.owner.same(owner))
            {
                let deadline = boundary.now().checked_add(delay);
                match deadline {
                    Some(at) => {
                        writer.not_before = Some(writer.not_before.map_or(at, |old| old.max(at)))
                    }
                    None => writer.blocked = true,
                }
            }
        }
        // Never assign a response snapshot, replay a visit or roll back newer
        // optimistic/remote prefs, including after unknown transport outcomes.
        boundary.failure(state, error, "Could not save emoji history");
    }
}
