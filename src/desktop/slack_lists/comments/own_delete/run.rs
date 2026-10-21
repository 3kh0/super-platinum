use super::*;
use durable::{Outcome, Recovery};
use super_platinum_core::slack::{api, transport::ListCommentDeleteFailure};
mod dispatch;
#[cfg(test)]
#[path = "tests/publication.rs"]
mod tests;
pub(crate) use dispatch::confirm;

pub(crate) fn open(mut state: Signal<ShellState>, generation: u64, ts: String) {
    let captured = {
        let shell = state.peek();
        if !available(&shell, generation, &ts) {
            return;
        }
        let source = Source::capture(&shell, generation, &ts).unwrap();
        let preview = shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .find(|r| r.ts == ts)
            .unwrap()
            .clone();
        (Arc::new(source), preview)
    };
    let id = next_id();
    state.write().slack_lists.comments.own_delete.active = Some(Session {
        id,
        source: captured.0.clone(),
        preview: captured.1,
        busy: true,
        notice: Some("Checking this message and deletion permissions…".into()),
        record: None,
        phase: None,
        retry_at: 0,
        local_pending: false,
        recovery: None,
        suspension: Arc::new(Mutex::new(None)),
    });
    dioxus::core::spawn_forever(async move {
        open_owned(state, id, captured.0).await;
    });
}
async fn open_owned(mut state: Signal<ShellState>, id: u64, source: Arc<Source>) {
    let result = async {
        let suspension = draft::suspend_for_edit(state, source.owner.serial, |s| {
            session(s, id).is_some()
                && source.current(s)
                && !own_edit::blocks_departure(s, source.owner.serial)
        })
        .await?;
        if let Some(s) = session_mut(&mut state.write(), id) {
            *s.suspension.lock().unwrap() = Some(suspension);
        } else {
            return Err(unavailable());
        }
        let disk = storage::Disk::capture(&state.peek(), &source.owner, &source.target)?;
        let previous = {
            let shell = state.peek();
            shell
                .slack_lists
                .comments
                .own_delete
                .retained
                .iter()
                .find(|r| r.disk == disk && r.ts == source.ts)
                .cloned()
        };
        let recovery = match previous {
            Some(r) => r,
            None => {
                if state.peek().slack_lists.comments.own_delete.retained.len() >= 32 {
                    return Err(unavailable());
                }
                let r = Arc::new(Recovery::load(disk, source.ts.clone()).await?);
                state
                    .write()
                    .slack_lists
                    .comments
                    .own_delete
                    .retained
                    .push(r.clone());
                r
            }
        };
        if let Some(s) = session_mut(&mut state.write(), id) {
            s.recovery = Some(recovery.clone());
        }
        let outcome = recovery.flush().await?;
        let inventory = recovery.inventory().await;
        adopt(state, id, &recovery, outcome).await;
        // Reopening during a persisted cooldown is a read of the saved outcome.
        // Keep its wait guidance; a rejected preflight must not replace it with
        // a preparation error or create another request before expiry.
        if inventory.retry_at() > now().max(0) as u64
            || inventory.attempt().is_some_and(|a| {
                a.phase().locked()
                    || matches!(a.phase(), DeletePhase::Confirmed | DeletePhase::Observed)
            })
        {
            return Ok(());
        }
        let (_, serial, _, _) =
            controller::begin_preflight(&mut state.write()).ok_or_else(unavailable)?;
        let prepared = prepare(state, id, &source, serial).await;
        finish_read(
            &mut state.write(),
            &source.owner,
            serial,
            prepared.as_ref().err(),
        );
        let (intent, _) = prepared?;
        if !source.current(&state.peek()) || !suspended(&state.peek(), id) {
            return Err(unavailable());
        }
        if let Some(s) = session_mut(&mut state.write(), id) {
            s.record = Some(intent.record().clone());
            s.notice = None;
            s.busy = false;
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        let recovery = session(&state.peek(), id).and_then(|s| s.recovery.clone());
        if let Some(r) = recovery {
            adopt(state, id, &r, None).await;
            if r.pending().await {
                notice(
                    state,
                    id,
                    "The original result needs local recovery. Retry recovery; this deletion will not be sent again.",
                );
                return;
            }
        }
        notice(
            state,
            id,
            "This message could not be prepared for deletion. Cancel returns to your saved reply. Reopen Delete when storage and permissions are available.",
        );
    }
}
async fn prepare(
    state: Signal<ShellState>,
    id: u64,
    source: &Source,
    serial: u64,
) -> Result<(OwnDeleteIntent, own_delete_policy::Observation), Error> {
    let work = async {
        let loaded = network::acquire(state, &source.owner, serial, &source.target, None).await?;
        let known = loaded
            .page
            .messages()
            .iter()
            .find(|m| m.ts.as_deref() == Some(source.ts.as_str()))
            .ok_or_else(unavailable)?;
        if !source.same(known)
            || loaded.page.target() != &source.target
            || loaded
                .projection
                .visible(
                    std::slice::from_ref(known),
                    source.target.root_ts().unwrap_or(""),
                )
                .is_empty()
        {
            return Err(unavailable());
        }
        let policy = if crate::fixture::is_fixture() {
            fixture::own_delete::policy(state, &source.owner, serial).await?
        } else {
            own_delete_policy::acquire(state, &source.owner, serial).await?
        };
        if !source.current(&state.peek())
            || !source.owner.reading(&state.peek(), serial)
            || !policy.current(&state.peek())
            || !suspended(&state.peek(), id)
        {
            return Err(unavailable());
        }
        let (_, team, user) = source.owner.scope.stable_owner();
        let intent = OwnDeleteIntent::capture(&team, &user, &loaded.page, known, policy.policy())
            .map_err(|e| Error::Api(e.to_string()))?;
        Ok((intent, policy))
    };
    tokio::select! {biased;
        _=async {loop {tokio::time::sleep(std::time::Duration::from_millis(100)).await;if !source.current(&state.peek())||!source.owner.reading(&state.peek(),serial)||session(&state.peek(),id).is_none(){break;}}}=>Err(unavailable()),
        value=work=>value,
    }
}
fn finish_read(shell: &mut ShellState, owner: &Owner, serial: u64, error: Option<&Error>) {
    if !owner.reading(shell, serial) {
        return;
    }
    let deadline = error.map_or(0, retry_deadline).max(cooldown(shell));
    if deadline > now() {
        shell.slack_lists.comments.cooldown = Some((owner.scope.clone(), deadline));
    }
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    e.loading = false;
    e.read = next_id();
    e.retry_at = e.retry_at.max(deadline);
}
async fn adopt(
    mut state: Signal<ShellState>,
    id: u64,
    recovery: &Recovery,
    outcome: Option<Outcome>,
) {
    let inventory = recovery.inventory().await;
    let pending = recovery.pending().await;
    let mut shell = state.write();
    let Some(s) = session(&shell, id) else {
        return;
    };
    let source = s.source.clone();
    // Retain disk evidence regardless of view lifetime; projection is original
    // owner/socket/target only. Never impersonate a realtime message_deleted.
    if !pending && source.owner_current(&shell) {
        let proof = match &outcome {
            Some(Outcome::Confirmed(a)) => Some(a.record()),
            Some(Outcome::Observed(_)) => inventory.attempt().map(|a| a.record()),
            _ => inventory
                .attempt()
                .filter(|a| matches!(a.phase(), DeletePhase::Confirmed | DeletePhase::Observed))
                .map(|a| a.record()),
        };
        if let Some(record) = proof {
            let current = shell.slack_lists.comments.entry.as_ref().is_some_and(|e| {
                e.page.as_ref().is_some_and(|p| {
                    p.messages()
                        .iter()
                        .find(|m| m.ts.as_deref() == Some(record.ts()))
                        .is_some_and(|m| record.current(m))
                })
            });
            if current {
                deletions::remove_scoped(&mut shell, &source.owner, &source.target, &source.ts);
            }
        }
    }
    let deadline = inventory.retry_at();
    if source.owner.scope.stable_current(&shell) && deadline > now().max(0) as u64 {
        let deadline = (deadline.min(i64::MAX as u64) as i64).max(cooldown(&shell));
        shell.slack_lists.comments.cooldown = Some((source.owner.scope.clone(), deadline));
        if source.owner_current(&shell) {
            let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
            entry.retry_at = entry.retry_at.max(deadline);
        }
    }
    if let Some(s) = session_mut(&mut shell, id) {
        s.phase = inventory.attempt().map(|a| a.phase());
        s.retry_at = deadline;
        s.local_pending = pending;
        s.busy = false;
        if let Some(a) = inventory.attempt() {
            s.record = Some(a.record().clone());
        }
        s.notice=Some(if pending {"The original result could not be saved locally. Retry recovery; no deletion will be resent."}
            else {match s.phase {
                Some(DeletePhase::Pending|DeletePhase::Uncertain)=>"Deletion is unconfirmed. Check the result; this attempt will not be sent again.",
                Some(DeletePhase::Confirmed|DeletePhase::Observed)=>"Message deleted. Your reply and saved edit are retained.",
                Some(DeletePhase::Rejected)=>"The message could not be deleted. Current permissions will be checked before another explicit Delete.",
                Some(DeletePhase::RateLimited)=>"Slack asked you to wait. Your drafts are retained; no request is automatically retried.",
                Some(DeletePhase::NotSent)=>"The deletion was not sent. Your drafts are retained.",
                None=>"Checking this message and deletion permissions…",
            }}.into());
    }
    drop(shell);
    // A loaded durable outcome needs the same expiry repaint as a fresh one.
    // The existing owner/read/deadline guard prevents a late timer touching a
    // successor visit, and the callback never retries a request.
    controller::arm_retry(state, source.owner.serial);
}
pub(crate) fn retry_local(mut state: Signal<ShellState>, id: u64) {
    let recovery = {
        let shell = state.peek();
        if !presentation(&shell).is_some_and(|p| p.id == id && p.can_retry_local) {
            return;
        }
        session(&shell, id)
            .and_then(|s| s.recovery.clone())
            .unwrap()
    };
    session_mut(&mut state.write(), id).unwrap().busy = true;
    dioxus::core::spawn_forever(async move {
        let outcome = recovery.flush().await.ok().flatten();
        adopt(state, id, &recovery, outcome).await;
    });
}
pub(crate) fn check_result(mut state: Signal<ShellState>, id: u64) {
    let (source, recovery) = {
        let shell = state.peek();
        if !presentation(&shell).is_some_and(|p| p.id == id && p.can_check) {
            return;
        }
        let s = session(&shell, id).unwrap();
        (s.source.clone(), s.recovery.clone().unwrap())
    };
    session_mut(&mut state.write(), id).unwrap().busy = true;
    dioxus::core::spawn_forever(async move {
        let result = async {
            let inventory = recovery.inventory().await;
            let attempt = inventory.attempt().ok_or_else(unavailable)?;
            let (_, serial, _, _) =
                controller::begin_preflight(&mut state.write()).ok_or_else(unavailable)?;
            let loaded = network::acquire(state, &source.owner, serial, &source.target, None).await;
            let current = source.owner_current(&state.peek())
                && source.owner.reading(&state.peek(), serial)
                && session(&state.peek(), id).is_some();
            finish_read(
                &mut state.write(),
                &source.owner,
                serial,
                loaded.as_ref().err(),
            );
            let loaded = loaded?;
            if !current
                || !attempt
                    .record()
                    .observed(&loaded.page)
                    .map_err(|e| Error::Api(e.to_string()))?
            {
                return Err(unavailable());
            }
            let outcome = recovery
                .resolve(
                    attempt.id().into(),
                    Outcome::Observed(loaded.page),
                    inventory.retry_at(),
                )
                .await?;
            adopt(state, id, &recovery, outcome).await;
            Ok(())
        }
        .await;
        if result.is_err() {
            adopt(state, id, &recovery, None).await;
            if !recovery.pending().await {
                notice(
                    state,
                    id,
                    "Deletion is still unconfirmed. Missing or changed comments are not proof; the original attempt will not be sent again.",
                );
            }
        }
    });
}
pub(crate) fn cancel(mut state: Signal<ShellState>, id: u64) {
    let suspension = {
        let mut shell = state.write();
        let Some(s) = session_mut(&mut shell, id) else {
            return;
        };
        if s.busy {
            return;
        }
        s.busy = true;
        s.suspension.clone()
    };
    dioxus::core::spawn_forever(async move {
        let lease = suspension.lock().unwrap().take();
        let released = if let Some(lease) = lease {
            lease.release().await.is_ok()
        } else {
            true
        };
        let mut shell = state.write();
        if session(&shell, id).is_some() {
            shell.slack_lists.comments.own_delete.active = None;
        }
        if !released {
            if let Some(binding) = shell.slack_lists.comments.drafts.active.clone() {
                if let Some(e) = draft::entry_mut(&mut shell, &binding.id) {
                    e.error = Some(
                        "Your reply is retained. Retry local recovery before continuing to edit."
                            .into(),
                    );
                }
            }
        }
    });
}
