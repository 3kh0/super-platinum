//! Typed isolated seed. Initial drafts remain local-only unless the failure
//! scenario needs one real managed Editor whose submitted input fails to commit.
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
            return Err("Enrollment isolation missing".into());
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
    .map_err(|_| "Enrollment initialization task failed")??;
    let account = Account::isolated(ACCOUNT.into(), isolated.client());
    account.initialize().await.map_err(err)?;
    let client = account.client().map_err(err)?;
    let mut writers = BTreeMap::new();
    let mut failed = None;
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let owner = account.owner(key(target)).map_err(err)?;
        let empty = owner.local_inventory().await.map_err(err)?;
        let writer = local_writer::LocalWriter::renew(account.clone(), empty)
            .await
            .map_err(err)?;
        writer.submit(&local(INITIAL, target)).map_err(err)?;
        writer.flush().await.map_err(err)?;
        if scenario == "failed-editor-channel" && target == ComposerTarget::Channel {
            let exact = owner.local_inventory().await.map_err(err)?;
            account::wait(client.enroll_local(owner.scope().clone(), exact))
                .await
                .map_err(err)?;
            let editor = editor::Editor::open(owner).await.map_err(err)?;
            // Return to an actual local-only inventory while retaining the
            // exact released Editor. Failed input then exercises no_retained,
            // rather than the earlier already-managed eligibility refusal.
            editor.release().await.map_err(err)?;
            failed = Some(editor);
        } else {
            writers.insert(key(target), writer);
        }
    }
    let b = Arc::new(Binding {
        account,
        root,
        capture_dir,
        nonce,
        scenario,
        isolated,
        selected: Mutex::new((epoch, view)),
        discovery: Mutex::new(discovery::Discovery::default()),
        writers,
        failed,
        routed: AtomicU64::new(0),
        refused: AtomicU64::new(0),
        actions: Mutex::default(),
        native: Mutex::default(),
        result: Mutex::default(),
        gate: Mutex::default(),
        released: AtomicBool::new(false),
        changed: tokio::sync::Notify::new(),
    });
    if !b.current(&state.peek()) {
        return Err("Enrollment owner changed during install".into());
    }
    b.reload(state).await?;
    BINDING
        .set(b.clone())
        .map_err(|_| "Enrollment fixture already installed")?;
    Ok(b)
}
