//! Display-only read-boundary memory; never a row/read/action capability.
mod model;
use crate::state::{MainView, ShellState};
pub(super) use model::Cue;
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};
use super_platinum_core::slack::models::ThreadViewItem;

type Key = (String, String);

pub(super) fn key(item: &ThreadViewItem) -> Option<Key> {
    let channel = item.root_msg.channel.as_ref().filter(|s| !s.is_empty())?;
    let root = item.root_msg.ts.as_ref().filter(|s| !s.is_empty())?;
    if item.root_msg.thread_ts.as_ref().is_some_and(|s| s != root) {
        return None;
    }
    Some((channel.clone(), root.clone()))
}

pub(super) struct Markers {
    states: Mutex<HashMap<Key, model::State>>,
}
impl Markers {
    pub(super) fn new(keys: &HashSet<Key>, previous: Option<&Self>) -> Self {
        let previous = previous.and_then(|p| p.states.lock().ok());
        Self {
            states: Mutex::new(
                keys.iter()
                    .map(|key| {
                        (
                            key.clone(),
                            previous
                                .as_ref()
                                .and_then(|p| p.get(key))
                                .cloned()
                                .unwrap_or_default(),
                        )
                    })
                    .collect(),
            ),
        }
    }
    pub(super) fn cue(&self, item: &ThreadViewItem, replies: &[&str]) -> Option<Cue> {
        let key = key(item)?;
        if super_platinum_core::channel_threads::hidden(&item.root_msg) {
            return None;
        }
        let current = item.root_msg.extra.get("last_read")?.as_str()?;
        let subscribed = item.root_msg.extra.get("subscribed")?.as_bool()?;
        // One bounded entry update, like other render-owned visual registries.
        // No signal write, I/O, task, canonical mutation or authority escapes.
        self.states
            .lock()
            .ok()?
            .get_mut(&key)?
            .observe(&key.1, current, subscribed, replies)
    }
}

/// Credential-free fixture decoration, separate from native Accepted entirely.
/// This may paint a label; native controls still require the original receipt.
pub(super) struct Presentation {
    epoch: u64,
    team: String,
    markers: Markers,
}
impl Presentation {
    pub(super) fn cue(
        &self,
        shell: &ShellState,
        item: &ThreadViewItem,
        replies: &[&str],
    ) -> Option<Cue> {
        if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_none()
            || shell.core.active_account.is_some()
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.main_view != MainView::Threads
            || shell.core.account_epoch != self.epoch
            || shell.core.active_team.as_ref() != Some(&self.team)
            || !shell
                .core
                .threads_view
                .matches_scope(self.epoch, &self.team)
        {
            return None;
        }
        self.markers.cue(item, replies)
    }
}

pub(super) fn fixture(shell: &mut ShellState, ghost: bool) {
    shell.core.active_account = None;
    shell.core.session = None;
    shell.core.transport = None;
    let mut counts = HashMap::<Key, usize>::new();
    for item in &shell.core.threads_view.items {
        if let Some(key) = key(item) {
            *counts.entry(key).or_default() += 1;
        }
    }
    let keys = counts
        .into_iter()
        .filter_map(|(key, n)| (n == 1).then_some(key))
        .collect();
    let markers = Markers::new(&keys, None);
    if let Some(item) = shell.core.threads_view.items.first_mut() {
        item.root_msg.extra.insert("subscribed".into(), true.into());
        item.root_msg
            .extra
            .insert("last_read".into(), "1719800401.000000".into());
        let replies = item.replies();
        let timestamps = replies
            .iter()
            .filter_map(|m| m.ts.as_deref())
            .collect::<Vec<_>>();
        let _ = markers.cue(item, &timestamps);
        if ghost {
            item.root_msg
                .extra
                .insert("last_read".into(), "1719800403.000000".into());
        }
    }
    shell.thread_feed_reads.presentation = Some(Presentation {
        epoch: shell.core.account_epoch,
        team: shell.core.active_team.clone().unwrap(),
        markers,
    });
}
