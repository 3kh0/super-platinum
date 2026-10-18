//! Actual fresh Protected7 schema seed before inspection, no configured path/SQL.
use super::*;
fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(
    v: Result<o::StorageJob<I, T>, o::AdmissionRefused<I>>,
) -> Result<T, String> {
    let j = v.map_err(|e| format!("admit {:?}", e.reason))?;
    j.submit().map_err(|e| format!("submit {e:?}"))?;
    if j.wait().map_err(|e| format!("wait {e:?}"))? != o::JobState::Ready {
        return Err("storage not Ready".into());
    }
    j.result()
        .ok_or("result absent")?
        .map_err(|e| format!("{e:?}"))
}
pub(super) async fn install(mut state: Signal<ShellState>) -> Result<Arc<Run>, String> {
    let config = isolation::configuration()?;
    {
        let s = state.peek();
        if RUN.get().is_some()
            || s.core.session.is_some()
            || s.core.transport.is_some()
            || s.core.cache.is_some()
            || s.core.active_account.is_some()
            || s.core.active_channel.as_deref() != Some("C1")
            || s.core.active_team.as_deref() != Some("T1")
        {
            return Err("Slash fixture isolation refused".into());
        }
    }
    let target = if config.scenario.ends_with("thread") {
        ComposerTarget::Thread
    } else {
        ComposerTarget::Channel
    };
    let checked = config.clone();
    let (initialized, fixture, stream) = tokio::task::spawn_blocking(move || {
        isolation::validate(&checked)?;
        let kind = if target == ComposerTarget::Thread {
            o::SlashFixtureTarget::Thread
        } else {
            o::SlashFixtureTarget::Channel
        };
        let (route, fixture) =
            o::OwnedRoute::fresh_slash_fixture(kind, o::SlashFixtureOutcome::HeldClear)
                .map_err(|e| format!("{e:?}"))?;
        let inspected = route
            .inspect()
            .map_err(|e| format!("{e:?}"))?
            .wait()
            .map_err(|e| format!("{e:?}"))?;
        if inspected.metadata().kind != o::StoreKind::Protected7 {
            return Err("Slash schema seed missing".into());
        }
        let initialized = inspected
            .open()
            .map_err(|e| format!("{e:?}"))?
            .initialize_cold(&o::FactoryIdentity::default())
            .map_err(|e| format!("{e:?}"))?
            .wait()
            .map_err(|e| format!("{e:?}"))?;
        let ops = initialized.operations();
        let scope = fixture.scope().clone();
        let review = done(ops.enroll(done(ops.local_inventory(scope.clone()))?))?;
        let expected = done(ops.stream_state(o::TeamInput {
            scope: scope.clone(),
        }))?;
        let stream = done(ops.open_stream(o::OpenInput { scope, expected }))?;
        done(ops.verify_local(o::MutationInput {
            stream: stream.clone(),
            expected: review,
        }))?;
        Ok::<_, String>((initialized, Arc::new(fixture), stream))
    })
    .await
    .map_err(|_| "Slash seed task failed")??;
    let account = Account::from_retained(initialized);
    let editor = editor::Editor::open(
        account
            .owner(fixture.scope().key.clone())
            .map_err(|e| format!("{e:?}"))?,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    let local = LocalDraft {
        composer: super_platinum_core::ComposerState::with_text(INITIAL),
        attachments: vec![],
    };
    editor.submit(&local).map_err(|e| format!("{e:?}"))?;
    editor.flush().await.map_err(|e| format!("{e:?}"))?;
    let registry = Arc::new(registry::Registry::default());
    let mut shell = state.write();
    // Recheck isolation after every off-renderer initialization stage before install.
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.cache.is_some()
        || shell.core.active_account.is_some()
        || shell.core.active_channel.as_deref() != Some("C1")
    {
        fixture.revoke();
        return Err("Slash seed owner changed".into());
    }
    shell.core.account_epoch = shell
        .core
        .account_epoch
        .checked_add(1)
        .ok_or("epoch exhausted")?;
    let epoch = shell.core.account_epoch;
    shell.core.active_account = Some(account.id().into());
    shell.core.transport = Some(fixture.transport());
    shell
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("team absent")?
        .self_user_id = "U1".into();
    registry.select(account.id().into(), epoch);
    registry
        .replace(
            account.id(),
            epoch,
            &fixture.scope().key,
            None,
            editor.clone(),
        )
        .map_err(|e| format!("{e:?}"))?;
    shell.thread_root = fixture.scope().key.thread.clone();
    if let Some(root) = &fixture.scope().key.thread {
        let mut bag = crate::thread_vm::fixture_bag(4);
        bag.messages[0].ts = Some(root.clone());
        shell
            .core
            .threads
            .insert(("T1".into(), "C1".into(), root.clone()), bag);
    }
    shell.core.drafts = Default::default();
    shell
        .core
        .drafts
        .items
        .insert(fixture.scope().key.clone(), local.clone());
    match target {
        ComposerTarget::Channel => shell.core.composer = local.composer,
        ComposerTarget::Thread => shell.core.thread_composer = local.composer,
    }
    shell.sync_composer_drafts();
    shell.refresh_thread_from_core();
    let run = Arc::new(Run {
        account,
        registry,
        editor,
        fixture,
        stream,
        target,
        epoch,
        view_id: shell.draft_writes.view_id,
        scenario: config.scenario,
        nonce: config.nonce,
        root: config.root,
        captures: config.captures,
        parent: Mutex::default(),
        native: Mutex::default(),
        actions: Mutex::default(),
    });
    if !run.current(&shell) {
        return Err("Slash installed tuple differs".into());
    }
    RUN.set(run.clone())
        .map_err(|_| "Slash fixture already seeded")?;
    Ok(run)
}
