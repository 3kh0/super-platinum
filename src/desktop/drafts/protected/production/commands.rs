//! Two visible composer choosers; actual dispatched futures belong to ActionRoot.
//! No timer, discovery-on-render, generic queue, or alternate storage owner.
use super::send::Captured;
use super::*;
use crate::state::ComposerTarget;
use crate::view::composer::{
    protected_actions as actions, protected_enrollment as native,
    protected_operations::Root as ActionRoot,
};
use dioxus::prelude::*;
use durable::owned_store as o;
use std::{cell::RefCell, rc::Rc, time::Instant};
mod model;
pub(crate) mod recovery;
use super_platinum_core::slack::Error as NetworkError;

static NEXT_CHOICE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
thread_local! { static COOLDOWNS: RefCell<BTreeMap<(String,String),Option<Instant>>> = RefCell::new(BTreeMap::new()); }
thread_local! { static SLOTS: RefCell<[Option<Rc<Entry>>;2]> = RefCell::new([None,None]); }
struct Entry {
    serial: u64,
    captured: Captured,
    root: Rc<ActionRoot>,
    phase: RefCell<Phase>,
}
enum Phase {
    Loading,
    Ready {
        original: Option<native::Capture>,
        context: Arc<owned_slash::Context>,
        selected: Option<usize>,
    },
    Failed {
        message: &'static str,
        cooldown: Option<Instant>,
    },
    Running,
}
impl Drop for Entry {
    fn drop(&mut self) {
        if let Phase::Ready { context, .. } = self.phase.get_mut() {
            context.revoke();
        }
    }
}
fn index(target: ComposerTarget) -> usize {
    usize::from(target == ComposerTarget::Thread)
}

/// Source command reset retires discovery/choice only. Already dispatched
/// commands and their durable reconciliation retain their original custody.
pub(crate) fn invalidate_catalogs(account: &str, team: &str) {
    SLOTS.with_borrow_mut(|slots| {
        for slot in slots.iter_mut() {
            let retire = slot.as_ref().is_some_and(|entry| {
                entry.captured.account.id() == account
                    && entry.captured.key.team == team
                    && !matches!(*entry.phase.borrow(), Phase::Running)
            });
            if retire {
                *slot = None;
            }
        }
    });
}
fn same(entry: &Rc<Entry>) -> bool {
    SLOTS.with_borrow(|slots| {
        slots[index(entry.captured.target)]
            .as_ref()
            .is_some_and(|v| Rc::ptr_eq(v, entry))
    })
}
fn finish_flag(mut state: Signal<ShellState>, captured: &Captured) {
    let mut shell = state.write();
    if shell.core.active_account.as_deref() == Some(captured.account.id())
        && shell.core.account_epoch == captured.epoch
    {
        shell
            .draft_writes
            .finish_send(captured.account.id(), captured.epoch, &captured.key);
    }
}
pub(super) fn open(mut state: Signal<ShellState>, root: Rc<ActionRoot>, captured: Captured) {
    let blocked = COOLDOWNS.with_borrow_mut(|values| {
        values.retain(|_, until| until.is_none_or(|until| Instant::now() < until));
        values.contains_key(&(captured.account.id().to_owned(), captured.key.team.clone()))
    });
    if blocked {
        finish_flag(state, &captured);
        state
            .write()
            .show_toast("Slack asked to wait before loading commands again.");
        return;
    }
    let existing = SLOTS.with_borrow(|s| s[index(captured.target)].clone());
    if let Some(entry) = existing.filter(|e| e.captured.unchanged(&state.peek())) {
        finish_flag(state, &captured);
        if let Phase::Failed {
            cooldown: Some(until),
            ..
        } = *entry.phase.borrow()
        {
            if Instant::now() < until {
                state
                    .write()
                    .show_toast("Slack asked to wait before loading commands again.");
                return;
            }
        } else {
            return;
        }
    }
    let Ok(serial) = NEXT_CHOICE.fetch_update(
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
        |v| v.checked_add(1),
    ) else {
        finish_flag(state, &captured);
        return;
    };
    let entry = Rc::new(Entry {
        serial,
        captured,
        root,
        phase: RefCell::new(Phase::Loading),
    });
    SLOTS.with_borrow_mut(|s| s[index(entry.captured.target)] = Some(entry.clone()));
    dioxus::core::spawn_forever(async move {
        let result = discover(state, &entry).await;
        // A closed/replaced chooser still observes this exact read's rate limit.
        let retained_cooldown =
            if let Err(Discovery::Network(o::SlashDiscoveryError::Transport(error))) = &result {
                remember_cooldown(&entry, error)
            } else {
                None
            };
        if same(&entry) {
            let phase = match result {
                Ok((original, context)) => Phase::Ready {
                    original: Some(original),
                    context,
                    selected: None,
                },
                Err(error) => {
                    let mut cooldown = None;
                    let message = match error {
                        Discovery::Storage(_) => {
                            "The draft changed or its saved status needs review. Your text is retained."
                        }
                        Discovery::Network(o::SlashDiscoveryError::Transport(error)) => {
                            cooldown = retained_cooldown;
                            if entry.captured.current(&state.peek()) {
                                state.write().report_failure(
                                    &error,
                                    "Connect to Slack to load commands.",
                                    || {
                                        "Commands could not be loaded. Your draft is retained."
                                            .into()
                                    },
                                );
                            }
                            "Commands could not be loaded. Your draft is retained."
                        }
                        Discovery::Network(_) => {
                            "This command catalog or account is no longer current."
                        }
                    };
                    Phase::Failed { message, cooldown }
                }
            };
            *entry.phase.borrow_mut() = phase;
            finish_flag(state, &entry.captured);
        }
    });
}
fn remember_cooldown(entry: &Entry, error: &NetworkError) -> Option<Instant> {
    COOLDOWNS.with_borrow_mut(|values| {
        model::remember_cooldown(
            values,
            (
                entry.captured.account.id().into(),
                entry.captured.key.team.clone(),
            ),
            error,
            Instant::now(),
        )
    })
}

enum Discovery {
    Storage(durable::Error),
    Network(o::SlashDiscoveryError),
}
impl From<durable::Error> for Discovery {
    fn from(v: durable::Error) -> Self {
        Self::Storage(v)
    }
}
async fn discover(
    state: Signal<ShellState>,
    entry: &Rc<Entry>,
) -> Result<(native::Capture, Arc<owned_slash::Context>), Discovery> {
    let c = &entry.captured;
    let (original, _editor) = super::send::prepare_source(state, c).await?;
    if !same(entry) || !c.unchanged(&state.peek()) {
        return Err(durable::Error::Stale.into());
    }
    let ops = &c
        .account
        .owned
        .as_ref()
        .ok_or(durable::Error::Unsupported)?
        .operations;
    let stream = c.source()?.stream(&state.peek())?;
    let scope = Scope {
        account_id: c.account.id().into(),
        key: c.key.clone(),
    };
    let context = c
        .binding
        .discover_slash(ops, stream.clone(), scope)
        .await
        .map_err(Discovery::Network)?;
    if !same(entry) || !c.unchanged(&state.peek()) {
        return Err(durable::Error::Stale.into());
    }
    Ok((
        original,
        owned_slash::Context::configured(context, c.binding.clone(), stream),
    ))
}

pub(crate) struct Presentation {
    pub owner: u64,
    pub loading: bool,
    pub running: bool,
    pub message: Option<&'static str>,
    pub choices: Vec<(usize, String)>,
    pub selected: Option<usize>,
}
pub(crate) fn presentation(shell: &ShellState, target: ComposerTarget) -> Option<Presentation> {
    let entry = SLOTS.with_borrow(|s| s[index(target)].clone())?;
    if !entry.captured.unchanged(shell) {
        return None;
    }
    let owner = entry.serial;
    let mut result = Presentation {
        owner,
        loading: false,
        running: false,
        message: None,
        choices: Vec::new(),
        selected: None,
    };
    match &*entry.phase.borrow() {
        Phase::Loading => result.loading = true,
        Phase::Running => result.running = true,
        Phase::Failed { message, .. } => result.message = Some(message),
        Phase::Ready {
            context, selected, ..
        } => {
            if !context.is_live() {
                result.message =
                    Some("The draft changed. Close this chooser and load commands again.");
            } else {
                result.choices =
                    model::choices(context.catalog(), &entry.captured.local.composer.text);
                if result.choices.is_empty() {
                    result.message = Some(
                        "No supported installed app matches this command. Your text is retained.",
                    );
                }
                result.selected = *selected;
            }
        }
    }
    Some(result)
}
fn get(target: ComposerTarget, owner: u64) -> Option<Rc<Entry>> {
    SLOTS
        .with_borrow(|s| s[index(target)].clone())
        .filter(|e| e.serial == owner)
}
fn dismiss_entry(entry: &Rc<Entry>) {
    if let Phase::Ready { context, .. } = &*entry.phase.borrow() {
        context.revoke();
    }
    SLOTS.with_borrow_mut(|s| {
        if s[index(entry.captured.target)]
            .as_ref()
            .is_some_and(|e| Rc::ptr_eq(e, entry))
        {
            s[index(entry.captured.target)] = None;
        }
    });
}
pub(crate) fn cancel(mut state: Signal<ShellState>, target: ComposerTarget, owner: u64) {
    if let Some(e) = get(target, owner) {
        if !matches!(*e.phase.borrow(), Phase::Running) {
            dismiss_entry(&e);
            finish_flag(state, &e.captured);
        }
    }
    let _ = state.write();
}
pub(crate) fn select(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
    owner: u64,
    value: &str,
) {
    let Some(e) = get(target, owner) else { return };
    if !e.captured.unchanged(&state.peek()) {
        return;
    }
    let Ok(index) = value.parse::<usize>() else {
        return;
    };
    let mut phase = e.phase.borrow_mut();
    if let Phase::Ready {
        context, selected, ..
    } = &mut *phase
    {
        if context.is_live()
            && model::choice(context.catalog(), &e.captured.local.composer.text, index).is_some()
        {
            *selected = Some(index);
        }
    }
    let _ = state.write();
}
pub(crate) fn run(mut state: Signal<ShellState>, target: ComposerTarget, owner: u64) {
    let Some(e) = get(target, owner) else { return };
    if !e.captured.unchanged(&state.peek()) {
        return;
    }
    let request = {
        let mut phase = e.phase.borrow_mut();
        let Phase::Ready {
            original,
            context,
            selected: Some(selected),
        } = &mut *phase
        else {
            return;
        };
        if !context.is_live() || original.is_none() || e.root.has_capacity().is_err() {
            return;
        }
        if !state.write().draft_writes.begin_send(
            e.captured.account.id(),
            e.captured.epoch,
            e.captured.key.clone(),
        ) {
            return;
        }
        let Some(original) = original.take() else {
            return;
        };
        let Some(choice) = context.catalog().choice(*selected) else {
            return;
        };
        let request = (original, context.clone(), choice);
        *phase = Phase::Running;
        request
    };
    dioxus::core::spawn_forever(async move {
        let result = execute(state, &e, request).await;
        if result.is_err() && e.captured.current(&state.peek()) {
            state.write().show_toast(
                "The command is retained. Review its saved outcome before trying again.",
            );
        }
        // The renderer Root and account retain every admitted operation even
        // after this visible chooser is removed. Never replay on UI failure.
        if same(&e) {
            finish_flag(state, &e.captured);
            if result.is_ok() {
                dismiss_entry(&e);
            } else {
                // Keep a visible terminal disposition. This phase has no Run
                // action and cannot recreate or replay the retained operation.
                *e.phase.borrow_mut() = Phase::Failed {
                    message: "The command outcome is retained. Review its saved status before trying again.",
                    cooldown: None,
                };
                let _ = state.write();
            }
        }
    });
}
async fn execute(
    state: Signal<ShellState>,
    e: &Rc<Entry>,
    (original, context, choice): (
        native::Capture,
        Arc<owned_slash::Context>,
        super_platinum_core::slash_commands::catalog::Choice,
    ),
) -> Result<(), durable::Error> {
    struct Revoke(Arc<owned_slash::Context>);
    impl Drop for Revoke {
        fn drop(&mut self) {
            self.0.revoke();
        }
    }
    // Preparation failures also retire the catalog capability; the renderer
    // Root keeps any already-admitted child and does not gain replay authority.
    let _revoke = Revoke(context.clone());
    let c = &e.captured;
    let editor =
        super::enroll::editor(state, c.target, c.account.clone(), &c.key, &c.local).await?;
    let before = editor.flush().await?;
    c.source()?
        .prepare_basis(
            state,
            editor.owner.clone(),
            before.view.inventory().managed()?.clone(),
        )
        .await?;
    let after = editor.refresh_receipt().await?;
    if !same(e) || !c.unchanged(&state.peek()) || after.serial != before.serial {
        return Err(durable::Error::Stale);
    }
    let previous = session_view(&state.peek(), &c.key).ok_or(durable::Error::Stale)?;
    if !publish_session(&state.peek(), &previous, after.view.clone()) {
        return Err(durable::Error::Stale);
    }
    let admission = original.lock_preserving(&editor, after.serial).await?;
    if !admission.current(&state.peek()) || !c.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let stream = c.source()?.stream(&state.peek())?;
    if !context.current(&state.peek(), &stream) {
        return Err(durable::Error::Stale);
    }
    let mut state = state;
    let action = crate::app_modal::register_viewed_action(
        &mut state.write(),
        &choice.command().app,
        Some(c.key.channel.clone()),
    )
    .ok_or(durable::Error::Stale)?;
    drop(admission);
    let current_registry = registry(&state.peek())?;
    let parent = actions::start_slash(
        &e.root,
        actions::StartSlash {
            state,
            target: c.target,
            registry: current_registry,
            editor: editor.clone(),
            visit: Visit::new(),
            context: context.clone(),
            stream,
            choice,
            action,
        },
    )
    .map_err(|refused| {
        crate::app_modal::reject_action(&mut state.write(), &refused.request.action);
        refused.error
    })?;
    let status = parent.ready().await?;
    if status != operation::Status::Finished {
        return Err(durable::Error::ReviewRequired);
    }
    let current = editor.archive_slash(&parent).await?;
    // The storage archive is historical. Fresh current storage adoption above
    // and this new native proof are required before any memory acknowledgment.
    let captured = native::capture(state, c.target, current.local()).await?;
    let admission = captured
        .lock_preserving(&editor, editor.snapshot().serial)
        .await?;
    if !admission.current(&state.peek()) || !c.current(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let previous = session_view(&state.peek(), &c.key).ok_or(durable::Error::Stale)?;
    if !publish_session(
        &state.peek(),
        &previous,
        View::new(Inventory::Managed(current.clone()), None),
    ) {
        return Err(durable::Error::Stale);
    }
    editor.acknowledge_slash(&parent, &current, || {
        admission.current(&state.peek()) && c.current(&state.peek())
    })?;
    drop(admission);
    e.root.acknowledge(&parent)?;
    Ok(())
}
