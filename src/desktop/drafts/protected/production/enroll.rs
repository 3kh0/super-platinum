//! Ordinary Send's exact local enrollment and retained editor installation.
//! This is storage/renderer preparation, never a transport or verification shortcut.
use super::*;
use crate::{
    state::{ComposerTarget, ShellState},
    view::composer::protected_enrollment as native,
};
use dioxus::prelude::*;

pub(super) async fn editor(
    state: Signal<ShellState>,
    target: ComposerTarget,
    account: Arc<Account>,
    key: &DraftKey,
    local: &LocalDraft,
) -> Result<Arc<editor::Editor>, durable::Error> {
    editor_with_policy(state, target, account, key, local, CapturePolicy::Plain).await
}
pub(super) async fn editor_hosted(
    state: Signal<ShellState>,
    target: ComposerTarget,
    account: Arc<Account>,
    key: &DraftKey,
    local: &LocalDraft,
) -> Result<Arc<editor::Editor>, durable::Error> {
    editor_with_policy(state, target, account, key, local, CapturePolicy::Hosted).await
}
pub(super) async fn editor_baseline(
    state: Signal<ShellState>,
    target: ComposerTarget,
    account: Arc<Account>,
    key: &DraftKey,
    local: &LocalDraft,
) -> Result<Arc<editor::Editor>, durable::Error> {
    editor_with_policy(state, target, account, key, local, CapturePolicy::Baseline).await
}
#[derive(Clone, Copy)]
enum CapturePolicy {
    Plain,
    Hosted,
    Baseline,
}
impl CapturePolicy {
    async fn capture(
        self,
        state: Signal<ShellState>,
        target: ComposerTarget,
        local: &LocalDraft,
    ) -> Result<native::Capture, durable::Error> {
        match self {
            Self::Plain => native::capture(state, target, local).await,
            Self::Hosted => native::capture_hosted(state, target, local).await,
            Self::Baseline => native::capture_baseline(state, target, local).await,
        }
    }
}
async fn editor_with_policy(
    state: Signal<ShellState>,
    target: ComposerTarget,
    account: Arc<Account>,
    key: &DraftKey,
    local: &LocalDraft,
    policy: CapturePolicy,
) -> Result<Arc<editor::Editor>, durable::Error> {
    let (selected, prior) = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
        let selected = root.host_mut().select(&shell, account.clone())?;
        (selected, root.host().installation(&account, key))
    };
    if let Some(prior) = prior {
        let editor = prior.editor().clone();
        editor.flush().await?;
        reload(state).await?;
        let registry = registry(&state.peek())?;
        if !registry.current(account.id(), selected.epoch(), key, &editor) {
            // Replacement account/view requires fresh native installation of
            // this exact retained editor; never register a second by key.
            editor.refresh_receipt().await?;
            let capture = policy.capture(state, target, local).await?;
            let request = {
                let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
                root()
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .host()
                    .prepare_rebind(&shell, &selected, &prior, &capture, Visit::new())?
            };
            let ready = request.prepare().await?;
            let admission = capture.lock().await?;
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            root()
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .host_mut()
                .rebind_native(&shell, ready, &admission)?;
        }
        return Ok(editor);
    }
    let managed = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
        let host = root.host_mut();
        match host.route_snapshot(&shell, key) {
            Ok(route) if route.kind == "opening-managed" => Some(
                host.retained_managed(&account)
                    .into_iter()
                    .find(|opened| opened.id() == route.identity)
                    .ok_or(durable::Error::Stale)?,
            ),
            Ok(_) => None,
            Err(_) => match host
                .discovery()
                .entry(&shell, key)
                .map(|v| v.inventory().clone())
            {
                Some(Inventory::Managed(expected)) => {
                    Some(host.begin_managed(&shell, &selected, expected)?)
                }
                _ => None,
            },
        }
    };
    if let Some(opened) = managed {
        opened.ready().await;
        let editor = root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .host()
            .managed(&opened)?;
        let flushed = editor.flush().await?;
        if !same_local(flushed.view.local(), local) {
            return Err(durable::Error::Stale);
        }
        reload(state).await?;
        let capture = policy.capture(state, target, local).await?;
        let request = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            root()
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .host()
                .prepare_managed_installation(&shell, &selected, &opened, &capture, Visit::new())?
        };
        let ready = request.prepare().await?;
        let admission = capture.lock().await?;
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let installed = root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .host_mut()
            .install_managed_native(&shell, ready, &admission)?;
        return Ok(installed.editor().clone());
    }
    let opened = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
        let host = root.host_mut();
        match host.route_snapshot(&shell, key) {
            Ok(route) if route.kind == "local" => host
                .retained_locals(&account)
                .into_iter()
                .find(|opened| opened.id() == route.identity)
                .ok_or(durable::Error::Stale)?,
            Ok(_) => return Err(durable::Error::ReviewRequired),
            Err(_) => host.open_local_current(&shell, &selected, key.clone())?,
        }
    };
    opened.ready().await;
    let lane = root()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .host()
        .local(&opened)?;
    let before = lane.writer().flush().await?;
    if !before
        .inventory
        .as_ref()
        .is_some_and(|v| same_local(v.local(), local))
        || before.pending.is_some()
        || !before.failed_inputs.is_empty()
    {
        return Err(durable::Error::ReviewRequired);
    }
    reload(state).await?;
    let inventory = session_view(&state.peek(), key).ok_or(durable::Error::Stale)?;
    if matches!(inventory.inventory(), Inventory::Local(_)) {
        let capture = policy.capture(state, target, local).await?;
        let request = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            root()
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .host()
                .enrollment(&shell, key, Visit::new())?
        };
        let prepared = request.prepare().await?;
        let admission = capture.lock().await?;
        let wait = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            root()
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .host_mut()
                .enqueue_enrollment(prepared, &shell, &admission)?
        };
        drop(admission); // Only the synchronous admission owns this short lease.
        wait.ready().await;
        let outcome = root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .host_mut()
            .take_enrollment(&wait)?;
        outcome.result()?;
        reload(state).await?;
    }
    if lane.writer().snapshot().serial != before.serial {
        return Err(durable::Error::Stale);
    }
    let expected = session_view(&state.peek(), key)
        .ok_or(durable::Error::Stale)?
        .inventory()
        .managed()?
        .clone();
    if !same_local(expected.local(), local) {
        return Err(durable::Error::Stale);
    }
    let adoption = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .host_mut()
            .begin_adoption(&shell, &selected, &opened, expected, Visit::new())?
    };
    adoption.ready().await;
    let capture = policy.capture(state, target, local).await?;
    let request = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .host()
            .prepare_installation(&shell, &adoption, &capture, Visit::new())?
    };
    let ready = request.prepare().await?;
    let admission = capture.lock().await?;
    let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
    let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
    let installed = root.host_mut().install_native(&shell, ready, &admission)?;
    root.host_mut().acknowledge_installation(&installed)?;
    Ok(installed.editor().clone())
}

async fn reload(state: Signal<ShellState>) -> Result<(), durable::Error> {
    let wait = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .host_mut()
            .begin_load(&shell)?
    };
    wait.ready().await;
    let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
    root()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .host_mut()
        .finish_load(&shell, &wait)
}
