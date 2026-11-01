//! Original native index rows authorize only the existing explicit pane open.
//! This is not SourceReady, canonical body/history, or file/action authority.
use super::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};
use super_platinum_core::slack::models::ThreadViewItem;

type Key = (String, String);
fn identity(item: &ThreadViewItem) -> Option<(&String, &String)> {
    let channel = item.root_msg.channel.as_ref()?;
    let root = item.root_msg.ts.as_ref()?;
    if channel.is_empty()
        || root.is_empty()
        || item.root_msg.thread_ts.as_ref().is_some_and(|r| r != root)
        || item.channel() != Some(channel)
        || item.root_ts() != Some(root)
    {
        return None;
    }
    Some((channel, root))
}
fn key(item: &ThreadViewItem) -> Option<Key> {
    identity(item).map(|(channel, root)| (channel.clone(), root.clone()))
}

// No Debug/serialization or public constructor. All retained fields are Send;
// the UI Source/Visit is not stored in ShellState.
pub(super) struct Accepted {
    origin: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor_owner: Arc<crate::realtime::activity_counts::CursorOwner>,
    client: SlackClient,
    epoch: u64,
    team: String,
    keys: HashSet<Key>,
    retired: AtomicBool,
    retired_keys: Mutex<HashSet<Key>>,
    pub(super) markers: Arc<crate::thread_feed::unread::Markers>,
}
impl Accepted {
    // Sole production call is after original Source/ticket/Core acceptance and
    // replay in run_with. A loaded vector or public FeedRequest cannot mint it.
    pub(super) fn capture(
        source: &Source,
        shell: &ShellState,
        previous_markers: Option<&crate::thread_feed::unread::Markers>,
    ) -> Option<Self> {
        if !source.admitted_current(shell)
            || !shell.core.client.same_configuration(&source.client)
            || !shell
                .core
                .threads_view
                .matches_scope(source.epoch, &source.team)
            || !shell.core.threads_view.loaded
            || shell.core.threads_view.loading
        {
            return None;
        }
        let mut counts = HashMap::<Key, usize>::new();
        for item in &shell.core.threads_view.items {
            if let Some(key) = key(item) {
                *counts.entry(key).or_default() += 1;
            }
        }
        let keys = counts
            .into_iter()
            .filter_map(|(key, count)| (count == 1).then_some(key))
            .collect::<HashSet<_>>();
        let markers = Arc::new(crate::thread_feed::unread::Markers::new(
            &keys,
            previous_markers,
        ));
        Some(Self {
            origin: source.root.clone(),
            owner: source.owner.clone(),
            cursor_owner: source.cursor_owner.clone(),
            client: source.client.clone(),
            epoch: source.epoch,
            team: source.team.clone(),
            keys,
            retired: AtomicBool::new(false),
            retired_keys: Mutex::new(HashSet::new()),
            markers,
        })
    }
    fn lineage_current(&self, shell: &ShellState) -> bool {
        shell.signed_in
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && Arc::ptr_eq(&self.origin, &shell.thread_feed_reads.incarnation)
            && self.owner.current(&shell.core)
            && shell.core.client.same_configuration(&self.client)
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let current = self.lineage_current(shell)
            && self.cursor_owner.current(&shell.core)
            && shell
                .core
                .threads_view
                .matches_scope(self.epoch, &self.team)
            && shell.core.threads_view.loaded
            && shell
                .thread_feed_reads
                .accepted
                .as_ref()
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
            && shell.thread_feed_reads.owned_sequence == Some(shell.core.threads_view.load_seq)
            && shell
                .thread_feed_reads
                .pending
                .as_ref()
                .is_none_or(|p| p.active(shell));
        if !current {
            self.retired.store(true, Ordering::Release);
        }
        current
    }
    pub(super) fn retire_missing(&self, shell: &ShellState) {
        if !self.current(shell) {
            return;
        }
        let mut counts = HashMap::<(&str, &str), usize>::new();
        for item in &shell.core.threads_view.items {
            if let Some((channel, root)) = identity(item) {
                *counts.entry((channel.as_str(), root.as_str())).or_default() += 1;
            }
        }
        let Ok(mut retired) = self.retired_keys.lock() else {
            self.retired.store(true, Ordering::Release);
            return;
        };
        for key in &self.keys {
            if counts.get(&(key.0.as_str(), key.1.as_str())) != Some(&1) {
                retired.insert(key.clone());
            }
        }
    }
    fn contains(&self, key: &Key) -> bool {
        self.keys.contains(key)
            && self
                .retired_keys
                .lock()
                .is_ok_and(|retired| !retired.contains(key))
    }
    pub(super) fn unread_marker(
        &self,
        shell: &ShellState,
        item: &ThreadViewItem,
        replies: &[&str],
    ) -> Option<crate::thread_feed::unread::Cue> {
        let key = key(item)?;
        if shell.main_view != MainView::Threads
            || !self.current(shell)
            || !self.contains(&key)
            || item
                .root_msg
                .team
                .as_deref()
                .is_some_and(|s| s != self.team)
            || shell
                .core
                .threads_view
                .items
                .iter()
                .filter(|i| {
                    identity(i).is_some_and(|(channel, root)| channel == &key.0 && root == &key.1)
                })
                .count()
                != 1
        {
            return None;
        }
        let current = item.root_msg.extra.get("last_read")?.as_str()?;
        if shell.core.thread_cursor(&(self.team.clone(), key.0, key.1)) != Some(current) {
            return None;
        }
        self.markers.cue(item, replies)
    }
}

#[derive(Clone)]
pub(crate) struct Row {
    source: Source,
    accepted: Arc<Accepted>,
    key: Key,
}
impl Row {
    pub(super) fn capture(
        source: &Source,
        shell: &ShellState,
        item: &ThreadViewItem,
    ) -> Option<Self> {
        let row = Self {
            source: source.clone(),
            accepted: shell.thread_feed_reads.accepted.clone()?,
            key: key(item)?,
        };
        row.current(shell).then_some(row)
    }
    fn available(&self, shell: &ShellState) -> Option<usize> {
        let channel = &self.key.0;
        let canonical = shell
            .core
            .workspaces
            .get(&self.source.team)?
            .channels
            .get(channel)?;
        if &canonical.id != channel {
            return None;
        }
        shell.channels.iter().position(|c| &c.id == channel)
    }
    fn current(&self, shell: &ShellState) -> bool {
        // Cached current rows may still open during a native refresh/failure.
        // This does not relax the separate getView Source.current admission.
        self.source.visit_current(shell)
            && shell.overlay.is_none()
            && self.source.stamp == Stamp::capture(shell, &self.source.team)
            && self.accepted.current(shell)
            && self.accepted.contains(&self.key)
            && self.available(shell).is_some()
            && shell
                .core
                .threads_view
                .items
                .iter()
                .filter(|i| {
                    identity(i).is_some_and(|(channel, root)| {
                        channel == &self.key.0 && root == &self.key.1
                    })
                })
                .count()
                == 1
    }
    fn admit(&self, shell: &mut ShellState) -> Option<String> {
        if !self.current(shell) {
            return None;
        }
        let (channel, root) = &self.key;
        let index = self.available(shell)?;
        // An explicit list-row open owns this surface even when Home happened
        // to leave the same channel selected. Always install its target.
        shell.select_channel(index, crate::state::ChannelOpen::InSurface);
        shell.begin_thread(root.clone(), None);
        shell.remember_surface();
        Some(shell.thread_scope())
    }
    fn begin(
        &self,
        mut state: Signal<ShellState>,
    ) -> Option<(String, crate::bootstrap::ThreadReadCheck)> {
        let (scope, request) = {
            let Ok(mut shell) = state.try_write() else {
                return None;
            };
            let Some(scope) = self.admit(&mut shell) else {
                return None;
            };
            (scope, shell.reconnect_generation)
        };
        let original = self.accepted.clone();
        let expected_scope = scope.clone();
        let check: crate::bootstrap::ThreadReadCheck = Arc::new(move |shell| {
            original.lineage_current(shell)
                && shell.thread_scope() == expected_scope
                && shell.reconnect_generation == request
        });
        Some((scope, check))
    }
    pub(crate) fn start(&self, state: Signal<ShellState>) {
        let Some((scope, check)) = self.begin(state) else {
            return;
        };
        // The original explicit user action is admitted above, before spawn.
        // Continue that exact pane; never recapture a new navigation from IDs.
        dioxus::core::spawn_forever(crate::bootstrap::load_started_thread_checked(
            state,
            self.key.0.clone(),
            self.key.1.clone(),
            scope,
            check,
        ));
    }
}

#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
