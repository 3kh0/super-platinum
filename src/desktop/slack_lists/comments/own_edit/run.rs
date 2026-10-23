use super::*;
use std::time::Duration;
use super_platinum_core::{
    slack::{api, transport::ListCommentUpdateFailure},
    slack_lists::comments::{OwnEditAccepted, OwnEditIntent, ReplyQuery, Tombstones},
};
mod save;
#[cfg(test)]
pub(super) use save::settle;
pub(crate) use save::{check_result, save};

pub(crate) fn open(state: Signal<ShellState>, generation: u64, ts: String) {
    let source = {
        let shell = state.peek();
        if !available(&shell, generation, &ts) {
            return;
        }
        Arc::new(Source::capture(&shell, generation, &ts).unwrap())
    };
    dioxus::core::spawn_forever(async move {
        open_owned(state, source).await;
    });
}
async fn open_owned(mut state: Signal<ShellState>, source: Arc<Source>) {
    let source_generation = source.owner.serial;
    let suspension = match draft::suspend_for_edit(state, source.owner.serial, |s| {
        source.current(s) && !blocks_departure(s, source.owner.serial)
    })
    .await
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let serial = {
        let mut shell = state.write();
        if !source.current(&shell)
            || !suspension.current(&shell)
            || blocks_departure(&shell, source.owner.serial)
        {
            return;
        }
        retire(&mut shell);
        let Some((_, serial, _, _)) = controller::begin_preflight(&mut shell) else {
            return;
        };
        shell.slack_lists.comments.own_edit.pending = Some(source.clone());
        serial
    };
    let result = async {
        let disk = storage::Disk::capture(&state.peek(), &source.owner, &source.target)?;
        let durable = durable::Durable::load(disk.clone(), source.ts.clone()).await?;
        let inventory = durable.inventory().await;
        let (seed, mut plain_mount) = if let Some(seed) = inventory.seed() {
            // Inert saved content can be shown without granting fresh permission.
            // Save still performs the complete current-source/policy preflight.
            if seed.plain_delta().is_some() {
                // Current native behavior is acquired independently of this
                // inert saved seed; it grants no new Save permission.
                let (_, policy) = prepare(state, &source, serial).await?;
                (
                    seed.clone(),
                    Some(plain::acquire_mount(state, &source, serial, &policy).await?),
                )
            } else {
                (seed.clone(), None)
            }
        } else {
            let (loaded, policy) = prepare(state, &source, serial).await?;
            if policy.policy().mode()
                == super_platinum_core::slack_lists::comments::edit_seed::mode::EditMode::PlainText
            {
                let (seed, mount) =
                    plain::initial(state, &source, serial, &loaded, &policy).await?;
                (seed, Some(mount))
            } else {
                let shell = state.peek();
                if !source.current(&shell) || !suspension.current(&shell) || !policy.current(&shell)
                {
                    return Err(unavailable());
                }
                let known = selected(&loaded.page, &source.ts)?;
                let (_, team, user) = source.owner.scope.stable_owner();
                (
                    OwnEditSeed::capture(
                        &team,
                        &user,
                        &loaded.page,
                        known,
                        policy.policy(),
                        millis(),
                    )
                    .map_err(|e| Error::Api(e.to_string()))?,
                    None,
                )
            }
        };
        let restored = inventory.seed().is_some();
        let plain = if restored {
            inventory.plain_delta().cloned()
        } else {
            seed.plain_delta().cloned()
        };
        // Restored content can differ from both the server seed and original
        // seed Delta. Admit exactly the journal's reached embeds, against this
        // opening's fresh catalog; never borrow its previous mount appearance.
        if let (Some(mount), Some(delta)) = (&mut plain_mount, &plain) {
            mount.observe_content(delta)?;
            if !mount.current(&state.peek()) {
                return Err(unavailable());
            }
        }
        let blocks = if restored {
            inventory.blocks().to_vec()
        } else {
            seed.editable().to_vec()
        };
        let content = if let Some(delta) = &plain {
            Content::PlainDelta(delta.clone())
        } else {
            Content::RichBlocks(blocks.clone())
        };
        durable.capture_content(&seed, &content).await?;
        let inventory = durable.inventory().await;
        let attempt = durable::restored_attempt(&inventory, &disk.account);
        Ok((
            seed,
            blocks,
            plain,
            plain_mount,
            durable,
            attempt,
            restored,
            i64::try_from(inventory.retry_at()).unwrap_or(i64::MAX),
        ))
    }
    .await;
    let mut shell = state.write();
    publish::finish_read(&mut shell, &source.owner, serial, result.as_ref().err());
    let own = shell
        .slack_lists
        .comments
        .own_edit
        .pending
        .as_ref()
        .is_some_and(|s| Arc::ptr_eq(s, &source));
    if !own {
        return;
    }
    shell.slack_lists.comments.own_edit.pending = None;
    match result {
        Ok((seed, blocks, plain, plain_mount, durable, attempt, restored, local_retry_at))
            if source.current(&shell)
                && suspension.current(&shell)
                && plain_mount.as_ref().is_none_or(|m| m.current(&shell)) =>
        {
            let binding = Binding {
                source,
                token: next_id(),
            };
            if let Some(a) = &attempt {
                let disk_id = a.lock().unwrap().disk_id.clone();
                shell
                    .slack_lists
                    .comments
                    .own_edit
                    .attempts
                    .retain(|old| old.lock().unwrap().disk_id != disk_id);
                shell.slack_lists.comments.own_edit.attempts.push(a.clone());
            }
            if local_retry_at > now() {
                shell.slack_lists.comments.cooldown =
                    Some((binding.source.owner.scope.clone(), local_retry_at));
                if let Some(e) = shell.slack_lists.comments.entry.as_mut() {
                    e.retry_at = e.retry_at.max(local_retry_at);
                }
            }
            let notice = if attempt
                .as_ref()
                .is_some_and(|a| a.lock().unwrap().phase == Phase::Unknown)
            {
                Some("Your saved edit was recovered. Its previous save is unconfirmed and will not be sent again. Check the result.".into())
            } else if !shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .and_then(|e| source::message(e, &binding.source.ts))
                .is_some_and(|m| seed.current(m))
            {
                Some("Your saved edit was recovered, but the original comment has changed. Both drafts are retained; Cancel returns to your reply.".into())
            } else if restored {
                Some("Your saved edit was recovered. Current permissions are checked before any new save.".into())
            } else {
                None
            };
            shell.slack_lists.comments.own_edit.active = Some(Session {
                titles: None,
                binding,
                blocks,
                plain,
                plain_mount,
                seed: Arc::new(seed),
                revision: 0,
                native: None,
                native_sequence: 0,
                native_ready: false,
                composing: false,
                native_pending: false,
                capture_failed: false,
                busy: false,
                finishing: false,
                durable: Some(durable),
                local_retry_at,
                notice,
                suspension: Arc::new(Mutex::new(Some(suspension))),
                attempt,
            });
        }
        _ if source.owner.active(&shell) => {
            shell.slack_lists.comments.entry.as_mut().unwrap().error=Some("This comment could not be opened for editing. Saved edits and your reply draft are retained. Retry Edit when local storage and permissions are available.".into());
        }
        _ => {}
    }
    drop(shell);
    let binding = state
        .peek()
        .slack_lists
        .comments
        .own_edit
        .active
        .as_ref()
        .filter(|s| s.binding.source.owner.serial == source_generation)
        .map(|s| s.binding.clone());
    if let Some(binding) = binding {
        titles::prepare(state, binding);
    }
    controller::arm_retry(state, source_generation);
}
pub(super) fn millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
fn selected<'a>(page: &'a Page, ts: &str) -> Result<&'a Message, Error> {
    page.messages()
        .iter()
        .find(|m| m.ts.as_deref() == Some(ts))
        .ok_or_else(unavailable)
}
fn inputs(
    shell: &ShellState,
    source: &Source,
) -> Result<
    (
        super_platinum_core::slack::SlackClient,
        super_platinum_core::config::WorkspaceSession,
    ),
    Error,
> {
    if !source.owner_current(shell) {
        return Err(unavailable());
    }
    if crate::fixture::is_fixture() {
        if !fixture::own_edit::enabled(shell) {
            return Err(unavailable());
        }
    }
    let ctx = source.owner.scope.request(shell).ok_or_else(unavailable)?;
    Ok((ctx.client, ctx.session))
}
async fn loaded(
    state: Signal<ShellState>,
    source: &Source,
    serial: u64,
) -> Result<network::Loaded, Error> {
    let mut loaded = network::acquire(state, &source.owner, serial, &source.target, None).await?;
    if selected(&loaded.page, &source.ts).is_err() {
        let (client, session) = if crate::fixture::is_fixture() {
            let shell = state.peek();
            (
                shell.core.client.clone(),
                fixture::compose::session(&shell)?,
            )
        } else {
            inputs(&state.peek(), source)?
        };
        let request = api::list_record_comments_replies(
            &client,
            &session,
            loaded.page.target(),
            &ReplyQuery {
                oldest: Some(source.ts.clone()),
                ..Default::default()
            },
            Tombstones::Include,
        )?;
        loaded.page =
            network::reply_page(state, &source.owner, serial, request, loaded.page.target())
                .await?;
    }
    Ok(loaded)
}
async fn prepare(
    state: Signal<ShellState>,
    source: &Source,
    serial: u64,
) -> Result<(network::Loaded, own_edit_policy::Observation), Error> {
    let work = async {
        let loaded = loaded(state, source, serial).await?;
        let known = selected(&loaded.page, &source.ts)?;
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
            fixture::own_edit::policy(state, &source.owner, serial).await?
        } else {
            own_edit_policy::acquire(state, &source.owner, serial).await?
        };
        if !source.current(&state.peek())
            || !source.owner.reading(&state.peek(), serial)
            || !policy.current(&state.peek())
        {
            return Err(unavailable());
        }
        Ok((loaded, policy))
    };
    tokio::select! {biased;
        _=retired(state,||source.current(&state.peek())&&source.owner.reading(&state.peek(),serial))=>Err(unavailable()),
        result=work=>result,
    }
}
async fn retired(_state: Signal<ShellState>, current: impl Fn() -> bool) {
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !current() {
            return;
        }
    }
}
fn notice(mut state: Signal<ShellState>, binding: &Binding, message: &str) {
    if let Some(s) = session_mut(&mut state.write(), binding) {
        s.busy = false;
        s.notice = Some(message.into());
    }
}
fn suspension_current(shell: &ShellState, binding: &Binding) -> bool {
    session(shell, binding).is_some_and(|s| {
        s.suspension
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.current(shell))
    })
}
pub(crate) fn cancel(mut state: Signal<ShellState>, binding: Binding) {
    if !presentation(&state.peek())
        .is_some_and(|p| p.binding.token == binding.token && p.can_cancel)
    {
        return;
    }
    // No native buffer exists before the first mount. Retire the title job
    // synchronously, then use the same original durable clear/return path.
    {
        let mut shell = state.write();
        if session(&shell, &binding).is_some_and(|s| titles::can_cancel_unmounted(&shell, s)) {
            session_mut(&mut shell, &binding).unwrap().finishing = true;
        }
    }
    dioxus::core::spawn_forever(async move {
        if session(&state.peek(), &binding).is_some_and(|s| s.finishing) {
            finish_return(state, &binding).await;
            return;
        }
        match bridge::capture(state, &binding).await {
            Ok(lease) => {
                if !presentation(&state.peek())
                    .is_some_and(|p| p.binding.token == binding.token && p.can_cancel)
                {
                    return;
                }
                restore(state, &binding, lease).await;
            }
            Err(_) => notice(
                state,
                &binding,
                "Finish composing or close the link editor before cancelling. Both drafts are retained.",
            ),
        }
    });
}
async fn restore(state: Signal<ShellState>, binding: &Binding, lease: bridge::Lease) {
    restore_checked(state, binding, lease, &|shell| binding.visible_mount(shell)).await;
}
async fn restore_checked(
    mut state: Signal<ShellState>,
    binding: &Binding,
    lease: bridge::Lease,
    current: &impl Fn(&ShellState) -> bool,
) {
    if !binding.visible_mount(&state.peek()) {
        return;
    }
    if !current(&state.peek()) {
        notice(
            state,
            binding,
            "The original edit changed before returning. Both drafts are retained.",
        );
        return;
    }
    if lease.validate().await.is_err() {
        notice(
            state,
            binding,
            "The edit could not finish native capture. Both drafts are retained.",
        );
        return;
    }
    if !binding.visible_mount(&state.peek()) {
        return;
    }
    if !current(&state.peek()) {
        notice(
            state,
            binding,
            "The original edit changed before returning. Both drafts are retained.",
        );
        return;
    }
    if lease.finish().await.is_err() {
        notice(
            state,
            binding,
            "The edit could not finish native capture. Both drafts are retained.",
        );
        return;
    }
    if let Some(s) = session_mut(&mut state.write(), binding) {
        s.finishing = true;
        s.busy = false;
    }
    finish_return_checked(state, binding, current).await;
}
async fn finish_return(state: Signal<ShellState>, binding: &Binding) {
    finish_return_checked(state, binding, &|shell| binding.visible_mount(shell)).await;
}
async fn finish_return_checked(
    mut state: Signal<ShellState>,
    binding: &Binding,
    current: &impl Fn(&ShellState) -> bool,
) {
    if !current(&state.peek()) {
        notice(
            state,
            binding,
            "The original edit changed before returning. Both drafts are retained.",
        );
        return;
    }
    let durable = {
        let mut shell = state.write();
        let Some(s) = session_mut(&mut shell, binding) else {
            return;
        };
        if !s.finishing || s.busy {
            return;
        }
        s.busy = true;
        s.durable.clone()
    };
    let Some(durable) = durable else {
        notice(
            state,
            binding,
            "The edit could not finish local recovery. Retry return; both drafts are retained.",
        );
        return;
    };
    if durable.clear().await.is_err() {
        notice(
            state,
            binding,
            "The edit could not finish local recovery. Retry return; both drafts are retained.",
        );
        return;
    }
    let suspended = {
        let mut shell = state.write();
        if !binding.visible_mount(&shell) {
            return;
        }
        if !current(&shell) {
            if let Some(s) = session_mut(&mut shell, binding) {
                s.busy = false;
                s.notice = Some(
                    "The original edit changed after local cleanup. The edit remains in memory; retry return to your retained reply."
                        .into(),
                );
            }
            return;
        }
        let s = shell.slack_lists.comments.own_edit.active.take().unwrap();
        let value = s.suspension.lock().unwrap().take();
        value
    };
    if let Some(s) = suspended {
        let _ = s.release().await;
    }
}
