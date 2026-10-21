//! Draft custody survives visible visits; only a fresh Binding permits actions.
use super::*;
use crate::view::slack_lists::comments::bridge;
use super_platinum_core::slack_lists::comments::journal::Inventory;
mod lifecycle;
#[cfg(test)]
pub(super) mod tests;
pub(super) use lifecycle::guarded_departure;
pub(super) use lifecycle::{ReplySuspension, suspend_for_edit};
pub(crate) use lifecycle::{can_open, depart, open, retry};
#[derive(Clone, Default)]
pub(super) struct Root {
    pub entries: Vec<Draft>,
    pub(super) active: Option<Binding>,
}
#[derive(Clone)]
pub(crate) struct Binding {
    pub(super) id: String,
    pub(super) owner: Owner,
    generation: u64,
    root: String,
}
impl Binding {
    pub(crate) fn owner_team(&self) -> &str {
        &self.owner.scope.team
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn scope(&self) -> String {
        format!("{}:{}:{}", self.id, self.owner.serial, self.generation)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn visible_mount(&self, shell: &ShellState) -> bool {
        self.owner.active(shell)
            && shell
                .slack_lists
                .comments
                .drafts
                .active
                .as_ref()
                .is_some_and(|b| {
                    b.id == self.id
                        && b.generation == self.generation
                        && b.owner.serial == self.owner.serial
                })
    }
    pub fn retained_mount(&self, shell: &ShellState) -> bool {
        entry(shell, &self.id).is_some_and(|e| {
            e.generation == self.generation && e.native.as_ref().is_some_and(|n| n.matches(self))
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.visible_mount(shell)
            && shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .is_some_and(|e| e.target.root_ts() == Some(self.root.as_str()))
    }
}
#[derive(Clone)]
pub(super) struct Draft {
    pub id: String,
    pub disk: storage::Disk,
    pub blocks: Vec<Value>,
    pub sequence: u64,
    pub capture: String,
    pub inventory: Option<Inventory>,
    pub native: Option<bridge::Mounted>,
    pub native_binding: Option<Binding>,
    pub native_sequence: u64,
    pub generation: u64,
    pub composing: bool,
    pub capture_failed: bool,
    pub loading: bool,
    pub saving: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub status: Option<String>,
    pub retired_blocks: Option<Vec<Value>>,
    pub terminal: Option<reply::Terminal>,
}
#[derive(Clone)]
pub(crate) struct ComposeView {
    pub binding: Binding,
    pub blocks: String,
    pub loading: bool,
    pub saving: bool,
    pub busy: bool,
    pub locked: bool,
    pub ready: bool,
    pub can_send: bool,
    pub error: Option<String>,
    pub status: Option<String>,
    pub capture_failed: bool,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<ComposeView> {
    let binding = shell.slack_lists.comments.drafts.active.as_ref()?;
    if !binding.owner.active(shell) {
        return None;
    }
    let e = entry(shell, &binding.id)?;
    let locked = e.inventory.as_ref().is_some_and(|i| i.attempt().is_some());
    let current = binding.current(shell);
    let error = if !current {
        Some("The item's comment root changed. Your original draft is retained and cannot be sent to this new thread.".into())
    } else {
        e.error.clone()
    };
    Some(ComposeView {
        binding: binding.clone(),
        blocks: serde_json::to_string(&e.blocks).ok()?,
        loading: e.loading,
        saving: e.saving,
        busy: e.busy,
        locked,
        ready: e.inventory.is_some()
            && !e.loading
            && e.native.as_ref().is_none_or(|n| n.matches(binding)),
        can_send: current
            && !locked
            && !e.loading
            && !e.saving
            && !e.busy
            && !e.composing
            && !e.capture_failed
            && e.error.is_none()
            && e.terminal.is_none()
            && e.inventory.is_some()
            && cooldown(shell) <= now()
            && super_platinum_core::canvas::comments::content::fallback(&e.blocks)
                .is_ok_and(|s| !s.trim().is_empty()),
        error,
        status: e.status.clone(),
        capture_failed: e.capture_failed,
    })
}
pub(super) fn entry<'a>(shell: &'a ShellState, id: &str) -> Option<&'a Draft> {
    shell
        .slack_lists
        .comments
        .drafts
        .entries
        .iter()
        .find(|e| e.id == id)
}
pub(super) fn entry_mut<'a>(shell: &'a mut ShellState, id: &str) -> Option<&'a mut Draft> {
    shell
        .slack_lists
        .comments
        .drafts
        .entries
        .iter_mut()
        .find(|e| e.id == id)
}
pub(crate) fn install_native(
    shell: &mut ShellState,
    binding: &Binding,
    mounted: bridge::Mounted,
) -> bool {
    let visible = binding.visible_mount(shell);
    let retained = binding.retained_mount(shell);
    if !visible && !retained {
        return false;
    }
    let Some(e) = entry_mut(shell, &binding.id) else {
        return false;
    };
    let recovery = e
        .native
        .as_ref()
        .is_some_and(|old| old.failed() && old.mount() == mounted.mount());
    if (!visible && !recovery)
        || (e.native.is_some() && !recovery)
        || e.loading
        || e.busy
        || e.generation != binding.generation
        || e.inventory.is_none()
    {
        return false;
    }
    e.native = Some(mounted);
    e.native_binding = Some(binding.clone());
    if !recovery {
        e.native_sequence = 0;
        e.retired_blocks = None;
    }
    e.capture_failed = false;
    true
}
pub(crate) fn native(shell: &ShellState, binding: &Binding) -> Option<bridge::Mounted> {
    entry(shell, &binding.id)?.native.clone()
}
pub(crate) fn retain_capture(
    shell: &mut ShellState,
    binding: &Binding,
    mount: &str,
    sequence: u64,
    blocks: Vec<Value>,
    composing: bool,
    _detached: bool,
) -> Result<(), Error> {
    let e = entry_mut(shell, &binding.id).ok_or_else(unavailable)?;
    if e.generation != binding.generation
        || e.native.as_ref().is_none_or(|n| n.mount() != mount)
        || sequence <= e.native_sequence
        || sequence > 9_007_199_254_740_991
        || serde_json::to_vec(&blocks)
            .map_err(|_| unavailable())?
            .len()
            > 512 * 1024
    {
        return Err(unavailable());
    }
    e.native_sequence = sequence;
    e.composing = composing;
    if e.retired_blocks.as_ref() == Some(&blocks) {
        return Ok(());
    }
    if e.blocks != blocks {
        e.blocks = blocks;
        e.sequence = e.sequence.checked_add(1).ok_or_else(unavailable)?;
        e.capture = uuid::Uuid::new_v4().to_string();
        e.error = None;
        if e.terminal.is_none() && e.inventory.as_ref().is_none_or(|i| i.attempt().is_none()) {
            e.status = None;
        }
    }
    Ok(())
}
pub(super) fn rotate(shell: &mut ShellState, binding: &Binding) {
    let Some(e) = entry_mut(shell, &binding.id) else {
        return;
    };
    if e.native.is_some() {
        return;
    }
    e.generation = next_id();
    let generation = e.generation;
    if let Some(active) = shell
        .slack_lists
        .comments
        .drafts
        .active
        .as_mut()
        .filter(|b| b.id == binding.id && b.generation == binding.generation)
    {
        active.generation = generation;
    }
}
pub(crate) fn native_done(shell: &mut ShellState, binding: &Binding, mount: &str, success: bool) {
    if let Some(e) = entry_mut(shell, &binding.id).filter(|e| {
        e.generation == binding.generation && e.native.as_ref().is_some_and(|n| n.mount() == mount)
    }) {
        if success {
            e.native = None;
            e.native_binding = None;
            e.capture_failed = false;
            e.composing = false;
        } else {
            e.capture_failed = true;
            e.error = Some(
                "The original draft capture stopped. Retry capture before sending or leaving."
                    .into(),
            );
        }
    }
}
pub(crate) fn save(mut state: Signal<ShellState>, id: String) {
    {
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &id) else {
            return;
        };
        if e.saving || e.loading || e.inventory.is_none() {
            return;
        }
        e.saving = true;
    }
    dioxus::core::spawn_forever(async move {
        loop {
            let work = {
                let shell = state.peek();
                let Some(e) = entry(&shell, &id) else {
                    return;
                };
                let expected = e.inventory.clone().unwrap();
                if expected.blocks() == e.blocks && expected.capture() == e.capture {
                    None
                } else {
                    Some((
                        e.disk.clone(),
                        expected,
                        e.capture.clone(),
                        e.blocks.clone(),
                        e.sequence,
                    ))
                }
            };
            let Some((disk, expected, capture, blocks, sequence)) = work else {
                break;
            };
            let result = disk
                .work(move |j| j.save(&expected, &capture, blocks))
                .await;
            let mut shell = state.write();
            let Some(e) = entry_mut(&mut shell, &id) else {
                return;
            };
            match result {
                Ok(inventory) => {
                    adopt(e, inventory);
                    e.error = None;
                    if e.sequence == sequence {
                        break;
                    }
                }
                Err(_) => {
                    e.error=Some("Your draft is retained here but could not be saved locally. Retry local save before leaving.".into());
                    break;
                }
            }
        }
        if let Some(e) = entry_mut(&mut state.write(), &id) {
            e.saving = false;
        }
    });
}
pub(super) fn adopt(e: &mut Draft, inventory: Inventory) {
    if e.inventory
        .as_ref()
        .is_none_or(|i| i.revision() <= inventory.revision())
    {
        e.inventory = Some(inventory);
    }
}
pub(super) async fn flush(
    state: Signal<ShellState>,
    binding: &Binding,
    blocks: &[Value],
) -> Result<(), Error> {
    flush_id(state, &binding.id, blocks).await
}
pub(super) async fn flush_id(
    state: Signal<ShellState>,
    id: &str,
    blocks: &[Value],
) -> Result<(), Error> {
    save(state, id.to_owned());
    tokio::time::timeout(std::time::Duration::from_secs(6), async {
        loop {
            let done = {
                let shell = state.peek();
                let e = entry(&shell, id).ok_or_else(unavailable)?;
                flushed(e, blocks)?
            };
            if done {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| unavailable())?
}

fn flushed(e: &Draft, blocks: &[Value]) -> Result<bool, Error> {
    if e.blocks != blocks {
        return Err(unavailable());
    }
    let saved = e
        .inventory
        .as_ref()
        .is_some_and(|i| i.blocks() == blocks && i.capture() == e.capture);
    // A send/permission refusal is not a local draft failure. An exact durable
    // capture remains safe to leave while its delivery error stays visible.
    if !saved && e.error.is_some() {
        return Err(unavailable());
    }
    Ok(saved)
}

pub(crate) fn fixture_snapshot(shell: &ShellState) -> Value {
    if !fixture::compose::check_enabled()
        || ((shell.core.session.is_some() || shell.core.transport.is_some())
            && !fixture::completion::enabled(shell))
    {
        return Value::Null;
    }
    serde_json::json!({"active":shell.slack_lists.comments.drafts.active.as_ref().map(Binding::scope),"drafts":shell.slack_lists.comments.drafts.entries.iter().map(|e|serde_json::json!({"id":e.id,"scope":e.disk.scope,"blocks":e.blocks,"sequence":e.sequence,"capture":e.capture,"generation":e.generation,"busy":e.busy,"saving":e.saving,"loading":e.loading,"captureFailed":e.capture_failed,"native":e.native.is_some(),"error":e.error,"status":e.status,"savedRevision":e.inventory.as_ref().map(Inventory::revision),"attempt":e.inventory.as_ref().and_then(Inventory::attempt),"terminalPending":e.terminal.is_some()})).collect::<Vec<_>>()})
}

pub(crate) fn fixture_forget_saved(shell: &mut ShellState) -> bool {
    if !fixture::compose::enabled(shell) || shell.slack_lists.comments.drafts.active.is_some() {
        return false;
    }
    if shell.slack_lists.comments.drafts.entries.iter().any(|e| {
        e.native.is_some()
            || e.busy
            || e.loading
            || e.saving
            || e.terminal.is_some()
            || e.inventory
                .as_ref()
                .is_none_or(|i| i.blocks() != e.blocks || i.capture() != e.capture)
    }) {
        return false;
    }
    shell.slack_lists.comments.drafts.entries.clear();
    true
}
pub(super) async fn seal_for_render(
    mut state: Signal<ShellState>,
) -> Result<Option<bridge::Lease>, Error> {
    let binding = {
        let shell = state.peek();
        shell
            .slack_lists
            .comments
            .drafts
            .active
            .as_ref()
            .filter(|b| b.visible_mount(&shell))
            .cloned()
    };
    let Some(binding) = binding else {
        return Ok(None);
    };
    if entry(&state.peek(), &binding.id).is_none_or(|e| e.native.is_none()) {
        return Ok(None);
    }
    let lease = match bridge::capture(state, &binding).await {
        Ok(lease) => lease,
        Err(error) => {
            if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
                e.error=Some("Finish composing or close the link editor before refreshing. Your draft is retained.".into());
            }
            return Err(error);
        }
    };
    flush(state, &binding, lease.blocks()).await?;
    Ok(Some(lease))
}
