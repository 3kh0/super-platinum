//! Seed only local lanes through the shared Host, then retain that Host outside
//! the replaceable ShellState. Enrollment and installation happen through UI.
use super::*;
pub(crate) async fn install(state: Signal<ShellState>) -> Result<Arc<Binding>, String> {
    let (root, capture_dir, nonce, scenario) = isolation::configuration()?;
    let (epoch, view) = {
        let s = state.peek();
        if s.core.active_account.as_deref() != Some(ACCOUNT)
            || s.core.session.is_some()
            || s.core.transport.is_some()
            || s.core.cache.is_some()
        {
            return Err("Adoption isolation missing".into());
        }
        (s.core.account_epoch, s.draft_writes.view_id)
    };
    let (owned_root, owned_nonce) = (root.clone(), nonce.clone());
    let isolated = tokio::task::spawn_blocking(move || {
        isolation::validate_directory(&owned_root, &owned_nonce)?;
        durable::test_support::Isolated::create(ACCOUNT.into(), owned_root.join("worker"))
            .map_err(err)
    })
    .await
    .map_err(|_| "Adoption initialization task failed")??;
    let account = Account::isolated(ACCOUNT.into(), isolated.client());
    account.initialize().await.map_err(err)?;
    let mut host = lifecycle::Host::default();
    let selected = host.select(&state.peek(), account.clone()).map_err(err)?;
    let mut opened = BTreeMap::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let owner = account.owner(key(target)).map_err(err)?;
        let empty = owner.local_inventory().await.map_err(err)?;
        let request = host
            .open_local(&state.peek(), &selected, empty)
            .map_err(err)?;
        request.ready().await;
        let lane = host.local(&request).map_err(err)?;
        lane.writer().submit(&local(INITIAL, target)).map_err(err)?;
        lane.writer().flush().await.map_err(err)?;
        opened.insert(key(target), request);
    }
    let b = Arc::new(Binding {
        account,
        root,
        capture_dir,
        nonce,
        scenario,
        isolated,
        host: Mutex::new(host),
        selected: Mutex::new(selected),
        selected_view: Mutex::new((epoch, view)),
        opened,
        adoption: Mutex::default(),
        registered: Mutex::default(),
        install_visit: Mutex::default(),
        acknowledged: Mutex::default(),
        registry_labels: Mutex::default(),
        armed: Mutex::default(),
        routed: AtomicU64::new(0),
        refused: AtomicU64::new(0),
        actions: Mutex::default(),
        native: Mutex::default(),
        result: Mutex::default(),
        gate: Mutex::default(),
        releases: Mutex::default(),
        changed: tokio::sync::Notify::new(),
    });
    if !b.current(&state.peek()) {
        return Err("Adoption owner changed during seed".into());
    }
    b.reload(state).await?;
    BINDING
        .set(b.clone())
        .map_err(|_| "Adoption fixture already installed")?;
    Ok(b)
}
