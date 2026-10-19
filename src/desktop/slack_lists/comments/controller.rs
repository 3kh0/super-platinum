use super::*;

pub(crate) fn open(mut state: Signal<ShellState>, generation: u64) {
    if presentation(&state.peek()).is_some() || Owner::capture(&state.peek(), generation).is_none()
    {
        return;
    }
    if open_shell(&mut state.write(), generation) {
        let generation = state
            .peek()
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .owner
            .serial;
        refresh(state, generation);
        arm_retry(state, generation);
    }
}
pub(super) fn open_shell(shell: &mut ShellState, generation: u64) -> bool {
    if presentation(shell).is_some() {
        return false;
    }
    let Some((owner, target, title)) = Owner::capture(shell, generation) else {
        return false;
    };
    let retry_at = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .filter(|e| e.owner.same_record(&owner))
        .map_or(0, |e| e.retry_at)
        .max(cooldown(shell));
    shell.slack_lists.comments.entry = Some(Entry {
        owner,
        target,
        title,
        read: 0,
        read_replaces_page: false,
        loading: false,
        page: None,
        projection: None,
        accepted: Vec::new(),
        live: Vec::new(),
        live_reactions: Default::default(),
        live_edits: Default::default(),
        deleted: Default::default(),
        reveal: None,
        rows: Arc::new(Vec::new()),
        error: None,
        retry_at,
        cursors: Vec::new(),
    });
    shell.slack_lists.comments.open = true;
    true
}
pub(crate) fn close(mut state: Signal<ShellState>, generation: u64) {
    if callback_current(&state.peek(), generation) {
        close_shell(&mut state.write());
    }
}
pub(super) fn close_shell(shell: &mut ShellState) {
    shell.slack_lists.comments.open = false;
    if let Some(e) = &mut shell.slack_lists.comments.entry {
        e.loading = false;
        e.read = next_id();
        e.live_edits.clear();
    }
}
pub(crate) fn refresh(state: Signal<ShellState>, generation: u64) {
    load(state, generation, false);
}
pub(crate) fn next_page(state: Signal<ShellState>, generation: u64) {
    load(state, generation, true);
}
fn load(mut state: Signal<ShellState>, generation: u64, paging: bool) {
    if draft::presentation(&state.peek()).is_some_and(|d| d.busy) {
        return;
    }
    if !callback_current(&state.peek(), generation) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let Ok(lease) = draft::seal_for_render(state).await else {
            return;
        };
        if !callback_current(&state.peek(), generation) {
            return;
        }
        let Some((owner, serial, target, cursor)) = begin(&mut state.write(), paging) else {
            return;
        };
        if let Some(lease) = lease {
            let _ = lease.release().await;
        }
        let result = network::acquire(state, &owner, serial, &target, cursor.clone()).await;
        if !owner.reading(&state.peek(), serial) {
            return;
        }
        let Ok(lease) = draft::seal_for_render(state).await else {
            if owner.reading(&state.peek(), serial) {
                state
                    .write()
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .loading = false;
            }
            return;
        };
        if !owner.reading(&state.peek(), serial) {
            return;
        }
        let observed = result.as_ref().ok().map(|loaded| loaded.page.clone());
        finish(&mut state.write(), &owner, serial, cursor, result);
        if let Some(lease) = lease {
            let _ = lease.release().await;
        }
        if let Some(page) = observed {
            reply::observe(state, &owner, &page);
        }
        arm_retry(state, owner.serial);
    });
}

pub(super) fn callback_current(shell: &ShellState, generation: u64) -> bool {
    shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .is_some_and(|e| e.owner.serial == generation && e.owner.active(shell))
}
// One expiry notification per actual rate-limited read or reopened cooldown.
// It never retries HTTP and never writes into a replacement panel.
pub(super) fn arm_retry(mut state: Signal<ShellState>, generation: u64) {
    let captured = {
        let shell = state.peek();
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .filter(|e| {
                e.owner.serial == generation && e.owner.active(&shell) && e.retry_at > now()
            })
            .map(|e| (e.owner.clone(), e.read, e.retry_at))
    };
    let Some((owner, serial, deadline)) = captured else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(
            deadline.saturating_sub(now()).max(0) as u64,
        ))
        .await;
        if !retry_current(&state.peek(), &owner, serial, deadline) {
            return;
        }
        let mut shell = state.write();
        if retry_current(&shell, &owner, serial, deadline) {
            shell.slack_lists.comments.entry.as_mut().unwrap().retry_at = 0;
        }
    });
}
pub(super) fn retry_current(shell: &ShellState, owner: &Owner, serial: u64, deadline: i64) -> bool {
    owner.active(shell)
        && shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .is_some_and(|e| e.read == serial && e.retry_at == deadline && deadline <= now())
}
pub(super) fn begin(
    shell: &mut ShellState,
    paging: bool,
) -> Option<(Owner, u64, Target, Option<String>)> {
    let e = shell.slack_lists.comments.entry.as_ref()?;
    if !e.owner.active(shell) || e.loading || e.retry_at.max(cooldown(shell)) > now() {
        return None;
    }
    let cursor = if paging {
        let page = e.page.as_ref()?;
        if !page.has_more() {
            return None;
        }
        let cursor = page.cursor()?.to_owned();
        if e.cursors.contains(&cursor) {
            return None;
        }
        Some(cursor)
    } else {
        None
    };
    let e = shell.slack_lists.comments.entry.as_mut()?;
    e.loading = true;
    e.read_replaces_page = true;
    e.error = None;
    e.read = next_id();
    Some((e.owner.clone(), e.read, e.target.clone(), cursor))
}
pub(super) fn begin_preflight(
    shell: &mut ShellState,
) -> Option<(Owner, u64, Target, Option<String>)> {
    let read = begin(shell, false)?;
    shell
        .slack_lists
        .comments
        .entry
        .as_mut()?
        .read_replaces_page = false;
    Some(read)
}
pub(super) fn finish(
    shell: &mut ShellState,
    owner: &Owner,
    serial: u64,
    cursor: Option<String>,
    result: Result<network::Loaded, Error>,
) {
    if !owner.reading(shell, serial) {
        return;
    }
    match result {
        Ok(mut loaded) => {
            let previous = shell.slack_lists.comments.entry.as_ref().unwrap();
            if previous.target == *loaded.page.target() {
                let channel = loaded.page.target().channel_id().to_owned();
                for ts in &previous.deleted {
                    loaded.page.remove_reply(&channel, ts);
                }
            }
            let accepted = if previous.target == *loaded.page.target() {
                accepted::remaining(&loaded.page, &previous.accepted)
            } else {
                Vec::new()
            };
            let live = if previous.target == *loaded.page.target() {
                accepted::remaining(&loaded.page, &previous.live)
                    .into_iter()
                    .filter(|m| {
                        !m.ts
                            .as_ref()
                            .is_some_and(|ts| previous.deleted.contains(ts))
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let Some(rows) = accepted::rows_with_live(
                shell,
                owner,
                &loaded.page,
                &loaded.projection,
                &accepted,
                &live,
            ) else {
                return;
            };
            let e = shell.slack_lists.comments.entry.as_mut().unwrap();
            e.loading = false;
            if cursor.is_none() {
                e.cursors.clear();
            }
            if let Some(cursor) = cursor {
                e.cursors.push(cursor);
            }
            if e.target != *loaded.page.target() {
                e.deleted.clear();
            }
            e.target = loaded.page.target().clone();
            e.rows = rows;
            e.projection = Some(loaded.projection);
            e.accepted = accepted;
            e.live = live;
            e.live_reactions
                .retain(|ts| e.live.iter().any(|m| m.ts.as_ref() == Some(ts)));
            e.live_edits
                .retain(|ts, _| e.live.iter().any(|m| m.ts.as_ref() == Some(ts)));
            if e.reveal
                .as_ref()
                .is_some_and(|(_, ts)| !e.rows.iter().any(|row| &row.ts == ts))
            {
                e.reveal = None;
            }
            e.page = Some(loaded.page);
            e.error = None;
            e.retry_at = 0;
        }
        Err(error) => {
            let deadline = match &error {
                Error::RateLimited {
                    retry_after_secs: None,
                }
                | Error::HttpStatus {
                    status: 429,
                    retry_after_secs: None,
                } => now().saturating_add(30),
                _ => retry_at(&error),
            };
            if deadline > now() {
                shell.slack_lists.comments.cooldown =
                    Some((owner.scope.receipt_only(), deadline.max(cooldown(shell))));
            }
            let e = shell.slack_lists.comments.entry.as_mut().unwrap();
            e.loading = false;
            e.retry_at = e.retry_at.max(deadline);
            // Retain the last successfully parsed page and its cursor.
            e.error = Some(match error {
                Error::Offline(_) => "You are offline. Previously loaded comments are retained.",
                Error::RateLimited { .. } | Error::HttpStatus { status:429, .. } =>
                    "Slack asked us to wait. Previously loaded comments are retained.",
                _ => "Comments could not be loaded. Previously loaded comments are retained. Refresh to try again.",
            }.into());
        }
    }
}
