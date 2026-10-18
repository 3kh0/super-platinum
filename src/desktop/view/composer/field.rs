//! A textarea's Rust lifetime fences native projection across same-scope remounts.
use super::*;
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};

thread_local! {
    static VISITS: std::cell::RefCell<BTreeMap<(u64, u8), Weak<MountLease>>> = Default::default();
}
static NEXT_VISIT: AtomicU64 = AtomicU64::new(1);

pub(super) struct MountLease {
    pub(super) token: String,
    pub(super) scope: String,
    view: u64,
    target: u8,
    alive: Cell<bool>,
    mounted: Cell<bool>,
    slash: std::cell::RefCell<Vec<std::sync::Weak<crate::drafts::protected::owned_slash::Context>>>,
}

fn target_key(target: ComposerTarget) -> u8 {
    match target {
        ComposerTarget::Channel => 0,
        ComposerTarget::Thread => 1,
    }
}

impl MountLease {
    fn new(view: u64, target: ComposerTarget, scope: String) -> Rc<Self> {
        let lease = Rc::new(Self {
            token: NEXT_VISIT.fetch_add(1, Ordering::Relaxed).to_string(),
            scope,
            view,
            target: target_key(target),
            alive: Cell::new(true),
            mounted: Cell::new(false),
            slash: Default::default(),
        });
        VISITS.with_borrow_mut(|visits| {
            if let Some(old) = visits
                .insert((view, lease.target), Rc::downgrade(&lease))
                .and_then(|v| v.upgrade())
            {
                old.revoke_slash();
            }
        });
        lease
    }

    pub(super) fn current(&self) -> bool {
        self.alive.get()
            && self.mounted.get()
            && VISITS.with_borrow(|visits| {
                visits
                    .get(&(self.view, self.target))
                    .and_then(Weak::upgrade)
                    .is_some_and(|current| std::ptr::eq(current.as_ref(), self))
            })
    }

    pub(super) fn revoke_slash(&self) {
        for context in self
            .slash
            .borrow_mut()
            .drain(..)
            .filter_map(|v| v.upgrade())
        {
            context.revoke();
        }
    }
    pub(super) fn watch_slash(
        &self,
        context: &std::sync::Arc<crate::drafts::protected::owned_slash::Context>,
    ) -> bool {
        if !self.current() {
            return false;
        }
        let mut values = self.slash.borrow_mut();
        values.retain(|v| v.strong_count() != 0);
        if values.len() >= crate::drafts::protected::operation::MAX_OPERATIONS {
            return false;
        }
        values.push(std::sync::Arc::downgrade(context));
        true
    }
    fn retire(&self) {
        self.revoke_slash();
        self.alive.set(false);
        VISITS.with_borrow_mut(|visits| {
            let key = (self.view, self.target);
            if visits
                .get(&key)
                .and_then(Weak::upgrade)
                .is_some_and(|current| std::ptr::eq(current.as_ref(), self))
            {
                visits.remove(&key);
            }
        });
    }
}

pub(super) fn current(shell: &ShellState, target: ComposerTarget) -> Option<Rc<MountLease>> {
    VISITS.with_borrow(|visits| {
        visits
            .get(&(shell.draft_writes.view_id, target_key(target)))
            .and_then(Weak::upgrade)
            .filter(|lease| lease.current() && lease.scope == field_scope(shell, target))
    })
}

#[component]
pub(super) fn ComposerField(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
    scope: String,
    text: String,
    placeholder: String,
) -> Element {
    let view = state.peek().draft_writes.view_id;
    let mount_scope = scope.clone();
    let lease = use_hook(move || MountLease::new(view, target, mount_scope));
    let retired = lease.clone();
    use_drop(move || retired.retire());
    let id = match target {
        ComposerTarget::Channel => "channel-composer",
        ComposerTarget::Thread => "thread-composer",
    };
    let mounted = lease.clone();
    let input = lease.clone();
    let composing = lease.clone();
    let keydown = lease.clone();
    let destination = state.peek().composer_key(target);
    let account = state.peek().core.active_account.clone();
    let emoji_stamps = use_memo(move || {
        let shell = state.read();
        (
            crate::emoji_catalog::current(&shell)
                .map(|c| c.token.clone())
                .unwrap_or_default(),
            shell
                .core
                .active_team
                .as_ref()
                .and_then(|t| shell.core.workspaces.get(t))
                .map(|w| w.emoji_preferences.revision().to_string())
                .unwrap_or_default(),
        )
    });
    let (emoji_catalog, emoji_preferences) = emoji_stamps.read().clone();
    rsx! {
        textarea {
            id, "data-draft-scope": "{scope}", "data-projection-visit": "{lease.token}",
            "data-emoji-catalog": "{emoji_catalog}", "data-emoji-preferences": "{emoji_preferences}",
            initial_value: "{text}", placeholder: "{placeholder}",
            onmounted: move |_| {
                mounted.mounted.set(true);
                mount_autosize(id);
                projection::mount_native(id, &mounted.scope, &mounted.token);
            },
            oncompositionstart: move |_| composing.revoke_slash(),
            oninput: move |event| {
                input.revoke_slash();
                let value = event.value();
                let end = value.len();
                let mut shell = state.write();
                if !input.current() || field_scope(&shell, target) != input.scope
                    || shell.core.active_account != account || shell.composer_key(target) != destination { return; }
                shell.sync_composer_drafts();
                match target {
                    ComposerTarget::Channel => {
                        shell.core.composer.text = value;
                        shell.core.composer.set_selection(end, end);
                    }
                    ComposerTarget::Thread => {
                        shell.core.thread_composer.text = value;
                        shell.core.thread_composer.set_selection(end, end);
                    }
                }
                shell.checkpoint_composers();
                if target == ComposerTarget::Channel { shell.notify_typing(); }
            },
            onkeydown: move |event| {
                if !keydown.current() || field_scope(&state.peek(), target) != keydown.scope || event.is_composing() { return; }
                if let Some(mark) = format_shortcut(&event) {
                    event.prevent_default();
                    spawn(format_composer_dom(state, target, mark));
                } else if event.key() == Key::Enter && !event.modifiers().shift() {
                    event.prevent_default();
                    match target {
                        ComposerTarget::Channel => { spawn(crate::bootstrap::send_composer(state)); }
                        ComposerTarget::Thread => { spawn(crate::bootstrap::send_thread_composer(state)); }
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_scope_remount_retires_old_lease_and_late_cleanup_keeps_new() {
        let first = MountLease::new(8001, ComposerTarget::Channel, "scope".into());
        first.mounted.set(true);
        assert!(first.current());
        let next = MountLease::new(8001, ComposerTarget::Channel, "scope".into());
        next.mounted.set(true);
        assert!(!first.current());
        first.retire();
        assert!(next.current());
        next.retire();
        assert!(!next.current());
    }
    #[test]
    fn channel_and_thread_mount_lifetimes_are_independent() {
        let channel = MountLease::new(8002, ComposerTarget::Channel, "channel".into());
        let thread = MountLease::new(8002, ComposerTarget::Thread, "thread".into());
        channel.mounted.set(true);
        thread.mounted.set(true);
        thread.retire();
        assert!(channel.current());
        assert!(!thread.current());
        channel.retire();
    }
}

#[cfg(test)]
mod slash_tests {
    use super::*;
    use super_platinum_core::drafts::store::protected::owned_store::{
        OwnedRoute, SlashFixtureOutcome, SlashFixtureTarget,
    };
    #[test]
    fn exact_mount_input_and_replacement_permanently_revoke_only_owned_context() {
        let (_, first_context) = OwnedRoute::fresh_slash_fixture(
            SlashFixtureTarget::Channel,
            SlashFixtureOutcome::HeldClear,
        )
        .unwrap();
        let first_context = std::sync::Arc::new(first_context);
        let first = MountLease::new(8101, ComposerTarget::Channel, "scope".into());
        first.mounted.set(true);
        let first_watch =
            crate::drafts::protected::owned_slash::Context::fixture(first_context.clone());
        assert!(first.watch_slash(&first_watch));
        first.revoke_slash();
        assert!(!first_context.is_live());
        let (_, second_context) = OwnedRoute::fresh_slash_fixture(
            SlashFixtureTarget::Channel,
            SlashFixtureOutcome::HeldClear,
        )
        .unwrap();
        let second_context = std::sync::Arc::new(second_context);
        let second_watch =
            crate::drafts::protected::owned_slash::Context::fixture(second_context.clone());
        assert!(first.watch_slash(&second_watch));
        let second = MountLease::new(8101, ComposerTarget::Channel, "scope".into());
        second.mounted.set(true);
        assert!(!second_context.is_live());
        assert!(!first.watch_slash(&second_watch));
        first.retire();
        assert!(second.current());
        second.retire();
    }
}
