//! Exact two-service knowledge-tab removal, retained across dialog dismissal.
use super::*;
use super_platinum_core::channel_tabs::{KnowledgeRemoval, KnowledgeStage};

/// New supplies the reviewed inventory; Finish and Review consume only retained
/// custody. Review never issues either mutation.
pub(crate) fn start(
    mut state: Signal<ShellState>,
    visit: &Visit,
    new: Option<KnowledgeRemoval>,
    finish: bool,
) {
    if !visit.current(&state.peek()) {
        return;
    }
    let owner = (*visit.0).clone();
    let serial = {
        let mut shell = state.write();
        let Some(entry) = shell.channel_tabs.entries.get(&owner.key) else {
            return;
        };
        if entry.busy
            || entry.cooling
            || entry.pending.is_some()
            || (new.is_some() && entry.knowledge.is_some())
        {
            return;
        }
        let Some(op) = new.as_ref().or(entry.knowledge.as_ref()) else {
            return;
        };
        if (new.is_some() || finish)
            && !selection::can_mutate(&shell, &Action::Remove { id: op.id().into() })
        {
            return;
        }
        if finish && op.stage() != KnowledgeStage::Disabled {
            return;
        }
        let Some(serial) = shell.channel_tabs.next.checked_add(1) else {
            return;
        };
        shell.channel_tabs.next = serial;
        let entry = shell.channel_tabs.entries.get_mut(&owner.key).unwrap();
        if let Some(op) = new {
            entry.knowledge = Some(op);
        }
        entry.busy = true;
        entry.serial = serial;
        entry.notice.clear();
        serial
    };
    dioxus::core::spawn_forever(execute(state, owner, serial, finish));
}
fn retained(state: Signal<ShellState>, owner: &Owner, serial: u64) -> Option<KnowledgeRemoval> {
    state
        .peek()
        .channel_tabs
        .entries
        .get(&owner.key)
        .filter(|e| e.serial == serial)
        .and_then(|e| e.knowledge.clone())
}
fn refuse(mut state: Signal<ShellState>, owner: &Owner, serial: u64, text: &str) {
    if let Some(entry) = current_entry(&mut state.write(), owner, serial) {
        if entry
            .knowledge
            .as_ref()
            .is_some_and(|op| op.stage() == KnowledgeStage::Ready)
        {
            entry.knowledge = None;
        }
    }
    run::finish(state, owner, serial, text);
}
fn failed(
    mut state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    error: &super_platinum_core::slack::Error,
) {
    let sent = retained(state, owner, serial).is_some_and(|op| op.stage() != KnowledgeStage::Ready);
    if !sent {
        if let Some(entry) = current_entry(&mut state.write(), owner, serial) {
            entry.knowledge = None;
        }
    }
    run::failure(state, owner, serial, error, sent);
}
fn adopt(
    mut state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    current: &Inventory,
) -> Option<KnowledgeStage> {
    if !owner.account_current(&state.peek()) {
        run::finish(
            state,
            owner,
            serial,
            "Return to the original account to review the retained change.",
        );
        return None;
    }
    let mut shell = state.write();
    let entry = current_entry(&mut shell, owner, serial)?;
    let stage = entry.knowledge.as_mut()?.observe(current);
    entry.inventory = Some(current.clone());
    entry.permission = None;
    entry.busy = false;
    entry.notice=match stage {
        KnowledgeStage::Disabled=>"Automatic refresh is confirmed disabled. Finish removing the tab when ready.",
        KnowledgeStage::Complete=>"Automatic refresh is disabled and the tab removal is confirmed.",
        _=>"The knowledge-tab change is unconfirmed. Review current tabs; neither change will be sent again.",
    }.into();
    if stage == KnowledgeStage::Complete {
        entry.knowledge = None;
    }
    Some(stage)
}
fn publish(
    mut state: Signal<ShellState>,
    owner: &Owner,
    serial: u64,
    op: KnowledgeRemoval,
) -> bool {
    let mut shell = state.write();
    if !owner.current(&shell)
        || !selection::can_mutate(&shell, &Action::Remove { id: op.id().into() })
    {
        return false;
    }
    let Some(entry) = current_entry(&mut shell, owner, serial) else {
        return false;
    };
    entry.knowledge = Some(op);
    entry.busy = true;
    true
}
pub(super) async fn execute(state: Signal<ShellState>, owner: Owner, serial: u64, finish: bool) {
    if !owner.current(&state.peek()) {
        refuse(
            state,
            &owner,
            serial,
            "The conversation changed. No new change was sent.",
        );
        return;
    }
    let Some(mut op) = retained(state, &owner, serial) else {
        return;
    };
    let initial = op.stage() == KnowledgeStage::Ready;
    let (raw, current) = match run::read(&owner).await {
        Ok(v) => v,
        Err(e) => {
            failed(state, &owner, serial, &e);
            return;
        }
    };
    if !owner.current(&state.peek()) {
        refuse(
            state,
            &owner,
            serial,
            "The conversation changed. No new change was sent.",
        );
        return;
    }
    if !initial && !finish {
        adopt(state, &owner, serial, &current);
        return;
    }
    let permission = match owner.read_permission(state, &raw).await {
        Ok(p) => p,
        Err(e) => {
            failed(state, &owner, serial, &e);
            return;
        }
    };
    if permission != Permission::Allowed {
        refuse(
            state,
            &owner,
            serial,
            "Current permission is required. No new change was sent.",
        );
        return;
    }
    let request = if initial {
        op.begin_disable(&current, &owner.client, &owner.session)
    } else {
        op.begin_remove(&current, &owner.client, &owner.session)
    };
    let Ok(request) = request else {
        refuse(
            state,
            &owner,
            serial,
            "The knowledge Canvas or tab inventory changed. Review current tabs.",
        );
        return;
    };
    if !publish(state, &owner, serial, op) {
        refuse(
            state,
            &owner,
            serial,
            "The conversation or Canvas edit changed. No new change was sent.",
        );
        return;
    }
    match owner.execute(request).await {
        Ok(raw) if run::envelope(&raw).is_ok() => {}
        Ok(_) => {
            run::finish(
                state,
                &owner,
                serial,
                "The knowledge-tab change is unconfirmed. Review current tabs; it will not be sent again.",
            );
            return;
        }
        Err(e) => {
            failed(state, &owner, serial, &e);
            return;
        }
    }
    if !owner.account_current(&state.peek()) {
        run::finish(
            state,
            &owner,
            serial,
            "Return to the original account to review the retained change.",
        );
        return;
    }
    let (raw, current) = match run::read(&owner).await {
        Ok(v) => v,
        Err(e) => {
            failed(state, &owner, serial, &e);
            return;
        }
    };
    let stage = adopt(state, &owner, serial, &current);
    if !initial || stage != Some(KnowledgeStage::Disabled) || !owner.current(&state.peek()) {
        return;
    }
    // Only the uninterrupted, acknowledged initial action may continue to the
    // distinct second service. Explicit Review never resumes it automatically.
    let permission = match owner.read_permission(state, &raw).await {
        Ok(p) => p,
        Err(e) => {
            failed(state, &owner, serial, &e);
            return;
        }
    };
    if permission != Permission::Allowed {
        refuse(
            state,
            &owner,
            serial,
            "Refresh is disabled. Permission is required to finish removing the tab.",
        );
        return;
    }
    let Some(mut op) = retained(state, &owner, serial) else {
        return;
    };
    let Ok(request) = op.begin_remove(&current, &owner.client, &owner.session) else {
        return;
    };
    if !publish(state, &owner, serial, op) {
        refuse(
            state,
            &owner,
            serial,
            "Refresh is disabled. Return to this conversation to finish removing the tab.",
        );
        return;
    }
    match owner.execute(request).await {
        Ok(raw) if run::envelope(&raw).is_ok() => {}
        Ok(_) => {
            run::finish(
                state,
                &owner,
                serial,
                "Tab removal is unconfirmed. Review current tabs; it will not be sent again.",
            );
            return;
        }
        Err(e) => {
            failed(state, &owner, serial, &e);
            return;
        }
    }
    if !owner.account_current(&state.peek()) {
        run::finish(
            state,
            &owner,
            serial,
            "Return to the original account to review tab removal.",
        );
        return;
    }
    match run::read(&owner).await {
        Ok((_, current)) => {
            adopt(state, &owner, serial, &current);
        }
        Err(e) => failed(state, &owner, serial, &e),
    }
}
