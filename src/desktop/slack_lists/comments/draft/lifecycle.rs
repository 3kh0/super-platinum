use super::*;
pub(crate) fn can_open(shell: &ShellState, generation: u64) -> bool {
    controller::callback_current(shell, generation)
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
