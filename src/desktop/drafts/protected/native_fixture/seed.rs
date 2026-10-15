//! Seed through ordinary typed isolated commands; no SQL or legacy Stored.
use super::*;
pub(crate) async fn install(state: Signal<ShellState>) -> Result<Arc<Binding>, String> {
    let (root, nonce, scenario) = isolation::configuration()?;
    let (epoch, view_id) = {
        let s = state.peek();
        if s.core.active_account.as_deref() != Some(ACCOUNT)
            || s.core.session.is_some()
            || s.core.transport.is_some()
            || s.core.cache.is_some()
        {
            return Err("Protected fixture shell isolation missing".into());
        }
        (s.core.account_epoch, s.draft_writes.view_id)
    };
    let check_root = root.clone();
    let check_nonce = nonce.clone();
    let isolated = tokio::task::spawn_blocking(move || {
        isolation::validate_directory(&check_root, &check_nonce)?;
        durable::test_support::Isolated::create(ACCOUNT.into(), check_root.join("worker"))
            .map_err(err)
    })
    .await
    .map_err(|_| "Protected isolated initialization task failed")??;
    let account = Account::isolated(ACCOUNT.into(), isolated.client());
    account.initialize().await.map_err(err)?;
    let client = account.client().map_err(err)?;
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let owner = account.owner(key(target)).map_err(err)?;
        let empty = owner.local_inventory().await.map_err(err)?;
        let lease = account::wait(client.renew_local(empty.clone()))
            .await
            .map_err(err)?;
        let first = lease.initial_serial();
        let inventory = match account::wait(client.write_local(
            lease,
            first,
            first + 1,
            empty.local().clone(),
            local(INITIAL, target),
        ))
        .await
        .map_err(err)?
        {
            durable::LocalWriteResult::Applied(value) => value,
            _ => return Err("Isolated seed was not applied".into()),
        };
        account::wait(client.enroll_local(owner.scope().clone(), inventory))
            .await
            .map_err(err)?;
    }
    let stream = if scenario == "failed-write-return-channel" {
        None
    } else {
        let expected = account::wait(client.stream_state("T1".into()))
            .await
            .map_err(err)?;
        let stream = account::wait(client.open_stream("T1".into(), expected))
            .await
            .map_err(err)?;
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let owner = account.owner(key(target)).map_err(err)?;
            let review = owner.review().await.map_err(err)?;
            account::wait(client.verify_local(stream.clone(), review))
                .await
                .map_err(err)?;
        }
        Some(stream)
    };
    let registry = Arc::new(registry::Registry::default());
    registry.select(ACCOUNT.into(), epoch);
    let mut writer_ids = BTreeMap::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let editor = editor::Editor::open(account.owner(key(target)).map_err(err)?)
            .await
            .map_err(err)?;
        writer_ids.insert(key(target), editor.retained_id().to_owned());
        registry
            .replace(ACCOUNT, epoch, &key(target), None, editor)
            .map_err(err)?;
    }
    let b = Arc::new(Binding {
        account,
        registry,
        root,
        nonce,
        scenario,
        epoch: AtomicU64::new(epoch),
        view_id,
        isolated,
        stream,
        writer_ids,
        routed: AtomicU64::new(0),
        refused: AtomicU64::new(0),
        actions: Mutex::new(vec![]),
        native: Mutex::new(vec![]),
        held: AtomicBool::new(false),
        released: AtomicBool::new(false),
        inflight: AtomicBool::new(false),
        result: Mutex::new(None),
        changed: tokio::sync::Notify::new(),
    });
    if !b.current(&state.peek()) {
        return Err("Protected owner changed during installation".into());
    }
    BINDING
        .set(b.clone())
        .map_err(|_| "Protected fixture already installed")?;
    Ok(b)
}
