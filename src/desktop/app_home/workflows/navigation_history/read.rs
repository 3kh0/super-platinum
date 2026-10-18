//! Read the existing account history under a caller's own immutable lease. This
//! creates no second reducer, storage key, writer, or action authority.
use super::*;

pub(in crate::app_home) async fn capture_for(
    mut state: Signal<ShellState>,
    account: &str,
    team: &str,
    current: impl Fn(&ShellState) -> bool,
) -> Result<(Value, Witness), Error> {
    let shared = prepare(&mut state.write(), account, &current)?;
    // The same gate also serializes any already captured A -> B -> A writer.
    // Caller cancellation leaves that original store's history/queue intact.
    load_and_flush(account, &shared).await?;
    finish(&state.peek(), account, team, &current, &shared)
}

fn prepare(
    shell: &mut ShellState,
    account: &str,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<Arc<Mutex<Shared>>, Error> {
    if !current(shell) || identity(shell).as_deref() != Some(account) {
        return Err(unavailable());
    }
    ensure(shell).ok_or_else(unavailable)
}

fn finish(
    shell: &ShellState,
    account: &str,
    team: &str,
    current: &impl Fn(&ShellState) -> bool,
    shared: &Arc<Mutex<Shared>>,
) -> Result<(Value, Witness), Error> {
    if !current(shell)
        || identity(shell).as_deref() != Some(account)
        || !shell
            .app_home
            .workflow_navigation
            .as_ref()
            .and_then(|store| store.accounts.get(account))
            .is_some_and(|installed| Arc::ptr_eq(installed, shared))
    {
        return Err(unavailable());
    }
    let mut inner = shared.lock().unwrap_or_else(|e| e.into_inner());
    // Native multi-workspace product capability, identical to the workflow
    // caller. Team is the caller's bound logical team, not the visible channel.
    let projected = inner
        .history
        .as_mut()
        .ok_or_else(unavailable)?
        .project(true, Some(team), true)
        .map_err(|_| unavailable())?;
    let witness = Witness {
        account: account.into(),
        shared: shared.clone(),
        revision: inner.revision,
    };
    drop(inner);
    if !witness.current(shell) {
        return Err(unavailable());
    }
    Ok((nav::projection_json(&projected), witness))
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: &str = "00000000-0000-4000-8000-000000000001";
    const B: &str = "00000000-0000-4000-8000-000000000002";

    fn shell() -> ShellState {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some(A.into());
        shell.app_home = Default::default();
        shell
    }
    fn seeded(shell: &mut ShellState) -> Arc<Mutex<Shared>> {
        let shared = prepare(shell, A, &|_| true).unwrap();
        shared.lock().unwrap().history = Some(
            nav::History::initialize(nav::StorageRead::Parsed(serde_json::json!([
                {"primaryView":"C00000001","teamId":"T00000001","ts":1000},
                {"primaryView":"C00000002","teamId":"T00000002","ts":2000}
            ])))
            .unwrap(),
        );
        shared
    }
    #[test]
    fn two_readers_rejoin_the_exact_retained_store_without_new_writer_or_key() {
        let mut shell = shell();
        let original = seeded(&mut shell);
        let gate = original.lock().unwrap().gate.clone();
        let held = gate.try_lock().unwrap();
        let another = prepare(&mut shell, A, &|_| true).unwrap();
        assert!(Arc::ptr_eq(&original, &another));
        assert!(another.lock().unwrap().gate.try_lock().is_err());
        let (first, witness) = finish(&shell, A, "T00000001", &|_| true, &original).unwrap();
        let (second, _) = finish(&shell, A, "T00000002", &|_| true, &another).unwrap();
        assert_eq!(first, second);
        assert!(first.get("C00000001").is_some());
        assert!(first.get("C00000002").is_some());
        assert!(witness.current(&shell));
        assert!(original.lock().unwrap().writes.is_empty());
        drop(held);
    }
    #[test]
    fn foreign_account_or_expired_caller_never_initializes_or_projects_history() {
        let mut shell = shell();
        assert!(prepare(&mut shell, B, &|_| true).is_err());
        assert!(prepare(&mut shell, A, &|_| false).is_err());
        assert!(shell.app_home.workflow_navigation.is_none());
        let original = seeded(&mut shell);
        let before = original
            .lock()
            .unwrap()
            .history
            .as_ref()
            .unwrap()
            .snapshot();
        assert!(finish(&shell, A, "T00000001", &|_| false, &original).is_err());
        shell.core.active_account = Some(B.into());
        assert!(finish(&shell, A, "T00000001", &|_| true, &original).is_err());
        assert_eq!(
            original
                .lock()
                .unwrap()
                .history
                .as_ref()
                .unwrap()
                .snapshot(),
            before
        );
    }
    #[test]
    fn same_account_replacement_store_and_new_navigation_invalidate_observation() {
        let mut shell = shell();
        let old = seeded(&mut shell);
        let (_, witness) = finish(&shell, A, "T00000001", &|_| true, &old).unwrap();
        old.lock().unwrap().revision += 1;
        assert!(!witness.current(&shell));
        shell.app_home.workflow_navigation = None;
        let new = seeded(&mut shell);
        assert!(!Arc::ptr_eq(&old, &new));
        assert!(finish(&shell, A, "T00000001", &|_| true, &old).is_err());
        assert!(finish(&shell, A, "T00000001", &|_| true, &new).is_ok());
        assert!(!witness.current(&shell));
    }

    #[test]
    fn retired_reader_leaves_pending_entries_and_original_writer_gate_intact() {
        let mut shell = shell();
        let original = seeded(&mut shell);
        let gate = original.lock().unwrap().gate.clone();
        let writer = gate.try_lock().unwrap();
        let entry = nav::Entry::new(
            serde_json::json!({"primaryView":"C00000NEW","teamId":"T00000001","ts":3000}),
        );
        {
            let mut inner = original.lock().unwrap();
            let write = inner.history.as_mut().unwrap().push(Some(entry), &[]);
            inner.writes.push_back(write);
            inner.revision += 1;
        }
        let before = original
            .lock()
            .unwrap()
            .history
            .as_ref()
            .unwrap()
            .snapshot();
        assert!(finish(&shell, A, "T00000001", &|_| false, &original).is_err());
        assert!(prepare(&mut shell, A, &|_| false).is_err());
        assert!(original.lock().unwrap().gate.try_lock().is_err());
        assert_eq!(original.lock().unwrap().writes.len(), 1);
        assert_eq!(
            original
                .lock()
                .unwrap()
                .history
                .as_ref()
                .unwrap()
                .snapshot(),
            before
        );
        let another = prepare(&mut shell, A, &|_| true).unwrap();
        assert!(Arc::ptr_eq(&original, &another));
        drop(writer);
        assert!(gate.try_lock().is_ok());
    }

    #[test]
    fn original_consumer_context_is_checked_again_before_history_projection() {
        for change in 0..5 {
            let mut shell = shell();
            crate::app_home::fixture::install(&mut shell);
            let original = seeded(&mut shell);
            let context = Context::capture(&shell).unwrap();
            let cookie = shell.core.session.as_ref().unwrap().d_cookie.clone();
            let current = |s: &ShellState| {
                context.current(s)
                    && s.core.session.as_ref().is_some_and(|session| {
                        session.d_cookie == cookie
                            && session
                                .workspaces
                                .get(&context.session.team_id)
                                .is_some_and(|actual| {
                                    crate::emoji_preferences::same_session(actual, &context.session)
                                })
                    })
            };
            assert!(finish(&shell, A, &context.session.team_id, &current, &original).is_ok());
            let before = original
                .lock()
                .unwrap()
                .history
                .as_ref()
                .unwrap()
                .snapshot();
            match change {
                0 => shell.core.account_epoch += 1,
                1 => shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .d_cookie
                    .push_str("changed"),
                2 => shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&context.session.team_id)
                    .unwrap()
                    .token
                    .push_str("changed"),
                3 => shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&context.session.team_id)
                    .unwrap()
                    .url
                    .push_str("changed"),
                _ => {
                    shell.core.transport = Some(Arc::new(
                        super_platinum_core::slack::Transport::new("synthetic").unwrap(),
                    ))
                }
            }
            assert!(
                finish(&shell, A, &context.session.team_id, &current, &original).is_err(),
                "change {change}"
            );
            assert_eq!(
                original
                    .lock()
                    .unwrap()
                    .history
                    .as_ref()
                    .unwrap()
                    .snapshot(),
                before
            );
        }
    }
}
