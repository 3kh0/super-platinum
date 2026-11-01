//! Local original-packet reply reveal. No IO, canonical union or history grant.
use super::*;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Mutex,
};
use super_platinum_core::slack::models::ThreadViewItem;
mod context;
mod seed;
pub(super) use context::publish_earlier as publish_earlier_context;
pub(crate) use context::{
    Prepared as ContextPrepared, finish as finish_context, prepare as prepare_context,
};
pub(super) use context::{
    Scope as ContextScope, page as context_page, retire_missing as retire_contexts,
    visible_limit as context_visible_limit,
};
pub(super) use seed::Candidates;
use seed::Seed;
#[cfg(test)]
mod tests;

type Key = (String, String);
const INITIAL: usize = 100;
const MAXIMUM: usize = 200; // Native renderer budget, not Source global history.
#[derive(Clone)]
struct Window {
    seed: Seed,
    visible: usize,
}
impl Window {
    fn next(&self) -> Option<usize> {
        let older = self.seed.timestamps.len().saturating_sub(self.visible);
        let next = if older >= 100 {
            self.visible.checked_add(100)?
        } else if older > 0 && self.seed.reached_start {
            self.seed.timestamps.len()
        } else {
            return None;
        };
        (next > self.visible && next <= MAXIMUM).then_some(next)
    }
}
#[derive(Clone)]
pub(super) struct Scope {
    origin: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor_owner: Arc<crate::realtime::activity_counts::CursorOwner>,
    client: SlackClient,
    epoch: u64,
    team: String,
    windows: BTreeMap<Key, Window>,
    retired: Arc<AtomicBool>,
    retired_keys: Arc<Mutex<HashSet<Key>>>,
}
impl Scope {
    pub(super) fn capture(
        source: &Source,
        shell: &ShellState,
        incoming: Candidates,
        append: bool,
    ) -> Option<Self> {
        if !source.admitted_current(shell)
            || !source.client.same_configuration(&shell.core.client)
            || !shell.core.threads_view.loaded
            || shell.core.threads_view.loading
            || shell.thread_feed_reads.owned_sequence != Some(shell.core.threads_view.load_seq)
        {
            return None;
        }
        let previous = shell
            .thread_feed_reads
            .windows
            .as_ref()
            .filter(|p| append && p.current(shell));
        let mut windows = BTreeMap::new();
        if let Some(previous) = previous {
            for (key, window) in &previous.windows {
                if previous.window_current(shell, key) {
                    windows.insert(key.clone(), window.clone());
                }
            }
        }
        for (key, seed) in incoming.0 {
            windows.remove(&key);
            if let Some(seed) = seed.filter(|s| s.matches(shell, &source.team, &key)) {
                windows.insert(
                    key,
                    Window {
                        seed,
                        visible: INITIAL,
                    },
                );
            }
        }
        if windows.is_empty() {
            return None;
        }
        Some(Self {
            origin: source.root.clone(),
            owner: source.owner.clone(),
            cursor_owner: source.cursor_owner.clone(),
            client: source.client.clone(),
            epoch: source.epoch,
            team: source.team.clone(),
            windows,
            retired: Arc::new(AtomicBool::new(false)),
            retired_keys: Arc::new(Mutex::new(HashSet::new())),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let current = shell.signed_in
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && Arc::ptr_eq(&self.origin, &shell.thread_feed_reads.incarnation)
            && self.owner.current(&shell.core)
            && self.cursor_owner.current(&shell.core)
            && self.client.same_configuration(&shell.core.client)
            && shell
                .core
                .threads_view
                .matches_scope(self.epoch, &self.team)
            && shell.core.threads_view.loaded
            && shell.thread_feed_reads.owned_sequence == Some(shell.core.threads_view.load_seq)
            && shell
                .thread_feed_reads
                .windows
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), self));
        if !current {
            self.retired.store(true, Ordering::Release);
        }
        current
    }
    fn window_current(&self, shell: &ShellState, key: &Key) -> bool {
        if !self.current(shell) {
            return false;
        }
        let Ok(mut retired) = self.retired_keys.lock() else {
            return false;
        };
        if retired.contains(key) {
            return false;
        }
        let current = self
            .windows
            .get(key)
            .is_some_and(|w| w.seed.matches(shell, &self.team, key));
        if !current {
            retired.insert(key.clone());
        }
        current
    }
    pub(super) fn retire_missing(&self, shell: &ShellState) {
        for key in self.windows.keys() {
            self.window_current(shell, key);
        }
    }
}
fn key(item: &ThreadViewItem) -> Option<Key> {
    Some((item.root_msg.channel.clone()?, item.root_msg.ts.clone()?))
}
pub(crate) fn visible_limit(shell: &ShellState, item: &ThreadViewItem) -> Option<usize> {
    if shell.main_view != MainView::Threads {
        return None;
    }
    let key = key(item)?;
    let scope = shell.thread_feed_reads.windows.as_ref()?;
    scope
        .window_current(shell, &key)
        .then(|| scope.windows[&key].visible)
}
#[derive(Clone)]
pub(crate) struct Action {
    source: Source,
    scope: Arc<Scope>,
    key: Key,
    visible: usize,
}
impl Action {
    pub(super) fn capture(
        source: &Source,
        shell: &ShellState,
        item: &ThreadViewItem,
    ) -> Option<Self> {
        if !source.current(shell) || shell.overlay.is_some() {
            return None;
        }
        let scope = shell.thread_feed_reads.windows.as_ref()?.clone();
        let key = key(item)?;
        if !scope.window_current(shell, &key) {
            return None;
        }
        let window = scope.windows.get(&key)?;
        window.next()?;
        Some(Self {
            source: source.clone(),
            visible: window.visible,
            scope,
            key,
        })
    }
    pub(crate) fn control_id(&self) -> String {
        format!("thread-feed-earlier-{}-{}", self.key.0, self.key.1)
    }
    /// UI composition only. The synchronous local action below has no runtime
    /// requirement and does not start IO; this callback starts the independently
    /// qualified original profile reader after its local transition completes.
    pub(crate) fn start(&self, state: Signal<ShellState>) {
        if let Some(profiles_changed) = self.reveal_owned(state) {
            if profiles_changed {
                dioxus::core::spawn_forever(profiles::load(self.source.clone(), state));
            }
            super::document_names::schedule(state);
        }
    }
    pub(crate) fn reveal(&self, state: Signal<ShellState>) -> bool {
        self.reveal_owned(state).unwrap_or(false)
    }
    fn reveal_owned(&self, mut state: Signal<ShellState>) -> Option<bool> {
        let Ok(mut shell) = state.try_write() else {
            return None;
        };
        if !self.scope.window_current(&shell, &self.key)
            || !self.source.current(&shell)
            || shell.overlay.is_some()
        {
            return None;
        }
        let window = &self.scope.windows[&self.key];
        if window.visible != self.visible {
            return None;
        }
        let Some(visible) = window.next() else {
            return None;
        };
        let mut next = self.scope.as_ref().clone();
        next.windows.get_mut(&self.key).unwrap().visible = visible;
        next.retired = Arc::new(AtomicBool::new(false));
        // Publish before returning from this callback. Queued peer/old controls
        // retain the old pointer and cannot borrow the successor's revision.
        shell.thread_feed_reads.windows = Some(Arc::new(next));
        // Visibility only: the profile module must retain and requalify its
        // own ORIGINAL profile candidates. This window supplies no actors or
        // member data and cannot manufacture that independent purpose.
        Some(profiles::after_local_reveal(
            &mut shell,
            (&self.key.0, &self.key.1),
        ))
    }
}
