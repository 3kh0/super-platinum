use super::*;
#[cfg(test)]
mod navigation_tests;

/// Prepared departure retains the original native lock. No native finish or
/// active-binding mutation occurs until the caller confirms a synchronous route.
pub(in crate::slack_lists::comments) struct Departure {
    generation: u64,
    saved: Option<Saved>,
    lease: Option<bridge::Lease>,
}
struct Saved {
    binding: Binding,
    blocks: Vec<Value>,
    capture: String,
    sequence: u64,
    mount: Option<String>,
    unread_empty: bool,
}
fn same_binding(a: &Binding, b: &Binding) -> bool {
    a.id == b.id && a.generation == b.generation && a.owner.serial == b.owner.serial
}
fn usable(e: &Draft) -> bool {
    !e.busy && !e.loading && !e.composing && !e.capture_failed
}
fn report_refusal(shell: &mut ShellState, binding: &Binding) {
    if !binding.current(shell) {
        return;
    }
    if let Some(e) = entry_mut(shell, &binding.id) {
        let message = if e.busy {
            "Wait for the current comment send to finish before opening this file. Your draft is retained."
        } else if e.composing {
            "Finish composing or close the link editor before opening this file. Your draft is retained."
        } else {
            "The comment draft is not ready to leave. Retry local recovery before opening this file."
        };
        e.error.get_or_insert_with(|| message.into());
    }
}
impl Departure {
    pub(in crate::slack_lists::comments) fn current(&self, shell: &ShellState) -> bool {
        if !controller::callback_current(shell, self.generation) {
            return false;
        }
        let active = shell.slack_lists.comments.drafts.active.as_ref();
        match &self.saved {
            None => active.is_none(),
            Some(saved) => {
                active.is_some_and(|a| same_binding(a, &saved.binding))
                    && saved.binding.current(shell)
                    && entry(shell, &saved.binding.id).is_some_and(|e| {
                        usable(e)
                            && e.generation == saved.binding.generation
                            && e.blocks == saved.blocks
                            && e.capture == saved.capture
                            && e.sequence == saved.sequence
                            && e.native.as_ref().map(|n| n.mount()) == saved.mount.as_deref()
                            && (saved.unread_empty || matches!(flushed(e, &saved.blocks), Ok(true)))
                    })
            }
        }
    }
    /// Call only after the opener has actually replaced the original view, with
    /// no await between current() and that commit. Never clear a successor.
    pub(in crate::slack_lists::comments) fn committed(mut self, shell: &mut ShellState) {
        if let Some(saved) = &self.saved {
            if shell
                .slack_lists
                .comments
                .drafts
                .active
                .as_ref()
                .is_some_and(|b| same_binding(b, &saved.binding))
            {
                shell.slack_lists.comments.drafts.active = None;
            }
        }
        if let Some(lease) = self.lease.take() {
            lease.departed();
        }
    }
}

pub(in crate::slack_lists::comments) async fn guarded_departure(
    state: Signal<ShellState>,
    generation: u64,
    valid: impl Fn(&ShellState) -> bool,
) -> Result<Departure, Error> {
    if super::super::own_edit::blocks_departure(&state.peek(), generation)
        || super::super::own_delete::blocks_departure(&state.peek(), generation)
    {
        return Err(unavailable());
    }
    guarded_capture(state, generation, valid, false).await
}

/// A separate edit may hide the original reply, but never consume its binding
/// or journal. This lease stays sealed until that edit restores the same mount.
pub(in crate::slack_lists::comments) struct ReplySuspension(Departure);
impl ReplySuspension {
    pub(in crate::slack_lists::comments) fn current(&self, shell: &ShellState) -> bool {
        self.0.current(shell)
    }
    pub(in crate::slack_lists::comments) async fn release(mut self) -> Result<(), Error> {
        if let Some(lease) = self.0.lease.take() {
            lease.release().await?;
        }
        Ok(())
    }
}
pub(in crate::slack_lists::comments) async fn suspend_for_edit(
    state: Signal<ShellState>,
    generation: u64,
    valid: impl Fn(&ShellState) -> bool,
) -> Result<ReplySuspension, Error> {
    guarded_capture(state, generation, valid, true)
        .await
        .map(ReplySuspension)
}
async fn guarded_capture(
    mut state: Signal<ShellState>,
    generation: u64,
    valid: impl Fn(&ShellState) -> bool,
    editing: bool,
) -> Result<Departure, Error> {
    let binding = {
        let shell = state.peek();
        if !controller::callback_current(&shell, generation) || !valid(&shell) {
            return Err(unavailable());
        }
        shell.slack_lists.comments.drafts.active.clone()
    };
    let Some(binding) = binding else {
        return Ok(Departure {
            generation,
            saved: None,
            lease: None,
        });
    };
    let readiness = {
        let shell = state.peek();
        let e = entry(&shell, &binding.id).ok_or_else(unavailable)?;
        (
            binding.owner.serial == generation && binding.current(&shell) && usable(e),
            e.native.as_ref().map(|m| m.mount().to_owned()),
        )
    };
    if !readiness.0 {
        if editing {
            if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
                e.error.get_or_insert_with(|| "Finish composing or close the link editor and recover the reply draft before editing a comment.".into());
            }
        } else {
            report_refusal(&mut state.write(), &binding);
        }
        return Err(unavailable());
    }
    let mount = readiness.1;
    let lease = if mount.is_some() {
        match bridge::capture(state, &binding).await {
            Ok(lease) => Some(lease),
            Err(error) => {
                let mut shell = state.write();
                if valid(&shell) && binding.current(&shell) {
                    if let Some(e) = entry_mut(&mut shell, &binding.id) {
                        e.error.get_or_insert_with(|| if editing {"Finish composing or close the link editor before editing this comment. Your reply draft is retained."} else {"Finish composing or close the link editor before opening this file. Your draft is retained."}.into());
                    }
                }
                return Err(error);
            }
        }
    } else {
        None
    };
    let saved = {
        let shell = state.peek();
        if !valid(&shell) || !binding.current(&shell) {
            return Err(unavailable());
        }
        let e = entry(&shell, &binding.id).ok_or_else(unavailable)?;
        if !usable(e) || e.native.as_ref().map(|n| n.mount()) != mount.as_deref() {
            return Err(unavailable());
        }
        let blocks = lease
            .as_ref()
            .map_or_else(|| e.blocks.clone(), |l| l.blocks().to_vec());
        if e.blocks != blocks {
            return Err(unavailable());
        }
        Saved {
            binding: binding.clone(),
            blocks,
            capture: e.capture.clone(),
            sequence: e.sequence,
            mount,
            unread_empty: e.native.is_none() && e.inventory.is_none() && e.blocks.is_empty(),
        }
    };
    if !saved.unread_empty {
        flush(state, &binding, &saved.blocks).await?;
    }
    if !valid(&state.peek()) {
        return Err(unavailable());
    }
    if let Some(lease) = &lease {
        lease.validate().await?;
    }
    let departure = Departure {
        generation,
        saved: Some(saved),
        lease,
    };
    if !valid(&state.peek()) || !departure.current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(departure)
}
pub(crate) fn can_open(shell: &ShellState, generation: u64) -> bool {
    !super::super::own_edit::blocks_departure(shell, generation)
        && !super::super::own_delete::blocks_departure(shell, generation)
        && controller::callback_current(shell, generation)
        && shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .is_some_and(|e| !e.loading && e.page.is_some() && e.target.root_ts().is_some())
        && shell
            .slack_lists
            .comments
            .drafts
            .active
            .as_ref()
            .is_none_or(|b| !b.owner.active(shell))
}
pub(crate) fn open(mut state: Signal<ShellState>, generation: u64) {
    if !can_open(&state.peek(), generation) {
        return;
    }
    let captured = {
        let shell = state.peek();
        let e = shell.slack_lists.comments.entry.as_ref().unwrap();
        storage::Disk::capture(&shell, &e.owner, &e.target).map(|disk| (disk, e.owner.clone()))
    };
    let Ok((disk, owner)) = captured else {
        return;
    };
    let binding = {
        let mut shell = state.write();
        if !can_open(&shell, generation) {
            return;
        }
        let found = shell
            .slack_lists
            .comments
            .drafts
            .entries
            .iter()
            .find(|e| e.disk == disk)
            .map(|e| e.id.clone());
        let id = if let Some(id) = found {
            id
        } else {
            if shell.slack_lists.comments.drafts.entries.len() >= 32 {
                return;
            }
            let id = uuid::Uuid::new_v4().to_string();
            shell.slack_lists.comments.drafts.entries.push(Draft {
                id: id.clone(),
                disk: disk.clone(),
                blocks: Vec::new(),
                sequence: 0,
                capture: uuid::Uuid::new_v4().to_string(),
                inventory: None,
                native: None,
                native_binding: None,
                native_sequence: 0,
                generation: next_id(),
                composing: false,
                capture_failed: false,
                loading: false,
                saving: false,
                busy: false,
                error: None,
                status: None,
                retired_blocks: None,
                terminal: None,
            });
            id
        };
        let e = entry_mut(&mut shell, &id).unwrap();
        // A pending original-node tail cannot be reauthorized by a new visit.
        if e.native.is_some() {
            e.error=Some("The previous editor is still retaining its final draft. Retry after capture finishes.".into());
        } else {
            e.generation = next_id();
        }
        let binding = Binding {
            id,
            owner,
            generation: e.generation,
            root: disk.scope.root.clone(),
        };
        shell.slack_lists.comments.drafts.active = Some(binding.clone());
        binding
    };
    load(state, binding.id.clone());
}
pub(super) fn load(mut state: Signal<ShellState>, id: String) {
    let work = {
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &id) else {
            return;
        };
        if e.loading || e.inventory.is_some() {
            return;
        }
        e.loading = true;
        (e.disk.clone(), e.sequence)
    };
    dioxus::core::spawn_forever(async move {
        let scope = work.0.scope.clone();
        let result = work.0.work(move |j| j.inventory(&scope)).await;
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &id) else {
            return;
        };
        e.loading = false;
        match result {
            Ok(inventory) => {
                if e.sequence == work.1 {
                    e.blocks = inventory.blocks().to_vec();
                    if !inventory.capture().is_empty() {
                        e.capture = inventory.capture().into();
                    }
                }
                if inventory.attempt().is_some() {
                    e.status=Some("Delivery unconfirmed. Refresh comments to check. This attempt will not be sent again.".into());
                }
                adopt(e, inventory);
                e.error = None;
            }
            Err(_) => e.error = Some(
                "The saved comment draft could not be loaded. Retry local recovery before editing."
                    .into(),
            ),
        }
    });
}
pub(crate) fn retry(mut state: Signal<ShellState>, binding: &Binding) {
    if !binding.visible_mount(&state.peek()) {
        return;
    }
    let recover = {
        let shell = state.peek();
        entry(&shell, &binding.id).and_then(|e| {
            e.native
                .as_ref()
                .filter(|n| n.failed())
                .and(e.native_binding.clone())
        })
    };
    if let Some(original) = recover {
        bridge::retry(state, original);
        return;
    }
    if entry(&state.peek(), &binding.id).is_some_and(|e| e.terminal.is_some()) {
        reply::retry_resolution(state, binding.id.clone());
        return;
    }
    if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
        e.error = None;
    }
    if entry(&state.peek(), &binding.id).is_some_and(|e| e.inventory.is_none()) {
        load(state, binding.id.clone());
    } else {
        save(state, binding.id.clone());
    }
}
pub(crate) async fn depart(mut state: Signal<ShellState>, generation: u64) -> Result<(), Error> {
    if super::super::own_edit::blocks_departure(&state.peek(), generation)
        || super::super::own_delete::blocks_departure(&state.peek(), generation)
    {
        return Err(unavailable());
    }
    let binding = {
        let shell = state.peek();
        if !controller::callback_current(&shell, generation) {
            return Err(unavailable());
        }
        shell
            .slack_lists
            .comments
            .drafts
            .active
            .as_ref()
            .filter(|b| b.owner.serial == generation)
            .cloned()
    };
    let Some(binding) = binding else {
        return Ok(());
    };
    let needs_capture = {
        let shell = state.peek();
        let e = entry(&shell, &binding.id).ok_or_else(unavailable)?;
        if e.busy || e.loading || e.capture_failed {
            return Err(unavailable());
        }
        e.native.is_some()
    };
    if needs_capture {
        let lease = match bridge::capture(state, &binding).await {
            Ok(v) => v,
            Err(error) => {
                if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
                    e.error=Some("Finish composing or close the link editor before leaving. Your draft is retained.".into());
                }
                return Err(error);
            }
        };
        flush(state, &binding, lease.blocks()).await?;
        if !binding.visible_mount(&state.peek()) {
            return Err(unavailable());
        }
        lease.finish().await?;
    } else {
        let (blocks, unsaved_empty) = {
            let shell = state.peek();
            let e = entry(&shell, &binding.id).ok_or_else(unavailable)?;
            (
                e.blocks.clone(),
                e.inventory.is_none() && e.blocks.is_empty(),
            )
        };
        // A failed load never mounted an editor; leaving this empty local view
        // neither overwrites nor discards the unread journal on disk.
        if !unsaved_empty {
            flush(state, &binding, &blocks).await?;
        }
    }
    if !controller::callback_current(&state.peek(), generation) {
        return Err(unavailable());
    }
    state.write().slack_lists.comments.drafts.active = None;
    Ok(())
}
