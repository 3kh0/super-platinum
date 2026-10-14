//! Readbacks after a workspace-wide mutation. These requests never mark read.
//!
//! The baseline starts BEFORE the mutation, not after its acknowledgement.
//! A concurrent cursor/event wins even if it arrives during the write. We use a
//! conservative whole-snapshot barrier: a busy workspace can require an explicit
//! refresh, but an old response cannot erase a newer unread or message edit.
use super::{Context, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{
    domain::ThreadKey,
    slack::{
        api,
        models::{CountsPage, HistoryPage},
    },
    state::ChannelMessages,
};

pub(crate) struct Baseline {
    snapshot: Snapshot,
}

#[derive(PartialEq)]
struct Snapshot {
    timeline: String,
    thread: String,
    revisions: [u64; 4],
    channels: BTreeMap<String, ChannelRead>,
    messages: BTreeMap<String, BagRead>,
    threads: BTreeMap<ThreadKey, BagRead>,
    visible_channel: Option<Value>,
    visible_thread: Option<Value>,
    activity: Value,
    thread_feed: Value,
}

// Counts may replace these fields, but cannot replace a channel's transcript.
// Keep the workspace-wide barrier proportional to conversations, not messages.
#[derive(PartialEq)]
struct ChannelRead {
    cursor: Option<String>,
    counts: [Option<u32>; 3],
    flags: [bool; 7],
    membership: [Option<Value>; 2],
}

#[derive(PartialEq)]
struct BagRead {
    cursor: Option<String>,
    revision: u64,
    counts: [u32; 3],
    latest: Option<String>,
    last_message: Option<String>,
}

impl BagRead {
    fn capture(bag: &ChannelMessages) -> Self {
        Self {
            cursor: bag.last_read.clone(),
            revision: bag.read_revision,
            counts: [bag.unread_count, bag.mention_count, bag.vip_count],
            latest: bag.latest.clone(),
            last_message: bag.messages.last().and_then(|m| m.ts.clone()),
        }
    }
}

impl Baseline {
    pub(crate) fn capture(shell: &ShellState, context: &Context) -> Self {
        Self {
            snapshot: Snapshot::capture(shell, &context.owner.team),
        }
    }
}

impl Snapshot {
    fn capture(shell: &ShellState, team: &str) -> Self {
        let workspace = &shell.core.workspaces[team];
        Self {
            timeline: shell.timeline_scope(),
            thread: shell.thread_scope(),
            revisions: [
                workspace.channel_lifecycle.revision(),
                workspace.thread_counts.revision(),
                workspace.activity_counts.revision(),
                workspace.dock_counts.revision(),
            ],
            channels: workspace
                .channels
                .iter()
                .map(|(id, c)| {
                    (
                        id.clone(),
                        ChannelRead {
                            cursor: c.last_read.clone(),
                            counts: [c.unread_count, c.unread_count_display, c.mention_count],
                            flags: [
                                c.has_unreads,
                                c.is_starred,
                                c.is_archived,
                                c.is_im,
                                c.is_mpim,
                                c.is_channel,
                                c.is_group,
                            ],
                            membership: [
                                c.extra.get("is_member").cloned(),
                                c.extra.get("is_open").cloned(),
                            ],
                        },
                    )
                })
                .collect(),
            messages: workspace
                .messages
                .iter()
                .map(|(id, bag)| (id.clone(), BagRead::capture(bag)))
                .collect(),
            threads: shell
                .core
                .threads
                .iter()
                .filter(|(key, _)| key.0 == team)
                .map(|(key, bag)| (key.clone(), BagRead::capture(bag)))
                .collect(),
            visible_channel: shell
                .transcript_visible()
                .then(|| {
                    shell
                        .core
                        .active_channel
                        .as_ref()
                        .and_then(|channel| workspace.messages.get(channel))
                        .map(bag_state)
                })
                .flatten(),
            visible_thread: shell
                .thread_root
                .as_ref()
                .zip(shell.core.active_channel.as_ref())
                .and_then(|(root, channel)| {
                    shell
                        .core
                        .threads
                        .get(&(team.into(), channel.clone(), root.clone()))
                })
                .map(bag_state),
            activity: json!([shell.core.activity.items, workspace.activity_unread_count]),
            thread_feed: json!(shell.core.threads_view.items),
        }
    }
    fn current(&self, shell: &ShellState, context: &Context, id: u64) -> bool {
        context.visit_current(shell)
            && shell
                .bulk_read
                .entries
                .get(&context.owner)
                .is_some_and(|e| e.reconciling == Some(id))
            && !super::cursor_busy(shell, &context.owner.team)
            && *self == Self::capture(shell, &context.owner.team)
    }
}

// Exact values, including the read revision, catch cursor ABA and message edits.
// Sorting tombstones avoids HashSet iteration order changing the comparison.
fn bag_state(bag: &ChannelMessages) -> Value {
    let mut deleted: Vec<_> = bag.remote_deletions.iter().collect();
    deleted.sort();
    json!([
        bag.messages,
        deleted,
        bag.pending,
        bag.last_read,
        bag.read_revision,
        bag.unread_count,
        bag.mention_count,
        bag.vip_count,
        bag.latest
    ])
}

pub(crate) async fn run(
    mut state: Signal<ShellState>,
    context: Arc<Context>,
    baseline: Arc<Baseline>,
    id: u64,
) {
    let success = refresh(state, &context, &baseline, id).await;
    let mut shell = state.write();
    super::finish_reconcile(&mut shell, &context, id, success);
    if context.current(&shell) {
        shell.refresh_from_core();
    }
}

async fn refresh(
    mut state: Signal<ShellState>,
    context: &Context,
    baseline: &Baseline,
    id: u64,
) -> bool {
    // Isolated fixtures use their own raw-response ledger; never acquire live
    // credentials or synthesize counts when no transport was captured.
    let Some(transport) = context.transport.as_ref() else {
        return false;
    };
    if !baseline.snapshot.current(&state.peek(), context, id) {
        return false;
    }
    let counts = api::fetch_counts(transport, &context.client, &context.session).await;
    let mut expected = {
        let mut shell = state.write();
        if !baseline.snapshot.current(&shell, context, id) {
            return false;
        }
        let Ok(counts) = counts else { return false };
        if !apply_counts(&mut shell, &context.owner.team, &baseline.snapshot, counts) {
            return false;
        }
        shell.refresh_from_core();
        Snapshot::capture(&shell, &context.owner.team)
    };
    if !threads(state, context, id, &mut expected).await {
        return false;
    }
    if !activity(state, context, id, &mut expected).await {
        return false;
    }
    if !history(state, context, id, &mut expected).await {
        return false;
    }
    if !replies(state, context, id, &mut expected).await {
        return false;
    }
    if !expected.current(&state.peek(), context, id) {
        return false;
    }
    crate::bootstrap::persist_workspace(&state, &context.owner.team);
    true
}

fn apply_counts(
    shell: &mut ShellState,
    team: &str,
    baseline: &Snapshot,
    counts: CountsPage,
) -> bool {
    if *baseline != Snapshot::capture(shell, team) {
        return false;
    }
    let zero = counts
        .activity_v2
        .as_ref()
        .is_some_and(|counts| counts.total() == 0);
    let [membership, threads, activity, dock] = baseline.revisions;
    shell
        .core
        .workspaces
        .get_mut(team)
        .unwrap()
        .apply_counts_snapshot_with_dock(counts, membership, threads, activity, dock);
    // Only a complete authoritative zero, under the same barrier, clears rows.
    if zero
        && shell
            .core
            .activity
            .matches_scope(shell.core.account_epoch, team)
    {
        shell.core.activity.confirm_zero_unreads();
    }
    shell.refresh_dock_badge_for(team);
    true
}

/// Isolated checker coverage is deliberately counts-only. It exercises the
/// production owner/baseline barrier and reducer, not live feeds or history.
pub(crate) fn apply_fixture(
    shell: &mut ShellState,
    context: &Context,
    baseline: &Baseline,
    id: u64,
    raw: Value,
) -> bool {
    if !super::fixture::enabled(shell)
        || context.transport.is_some()
        || !baseline.snapshot.current(shell, context, id)
        || raw.get("ok").and_then(Value::as_bool) != Some(true)
        || raw.get("error").is_some_and(|error| !error.is_null())
    {
        return false;
    }
    let Ok(counts) = serde_json::from_value(raw) else {
        return false;
    };
    let applied = apply_counts(shell, &context.owner.team, &baseline.snapshot, counts);
    if applied {
        shell.refresh_from_core();
    }
    applied
}

async fn threads(
    mut state: Signal<ShellState>,
    context: &Context,
    id: u64,
    expected: &mut Snapshot,
) -> bool {
    let team = &context.owner.team;
    let (request, cursors) = {
        let mut shell = state.write();
        if !expected.current(&shell, context, id) {
            return false;
        }
        shell.core.threads_view.ensure_scope(context.epoch, team);
        let Some(request) = shell.core.threads_view.begin(false) else {
            return false;
        };
        let cursors = shell
            .core
            .threads
            .iter()
            .filter(|(key, _)| &key.0 == team)
            .map(|(key, bag)| (key.clone(), bag.read_revision))
            .collect();
        *expected = Snapshot::capture(&shell, team);
        (request, cursors)
    };
    let result = api::fetch_threads_view(
        context.transport.as_ref().unwrap(),
        &context.client,
        &context.session,
        30,
        request.cursor.clone(),
        false,
    )
    .await;
    let mut shell = state.write();
    if !expected.current(&shell, context, id) {
        if shell.core.threads_view.matches_scope(context.epoch, team) {
            shell.core.threads_view.fail(&request);
        }
        return false;
    }
    let Ok(mut page) = result else {
        shell.core.threads_view.fail(&request);
        return false;
    };
    let counts = page.threads_state.take();
    if !shell
        .core
        .apply_thread_feed_page(team, &request, page, &cursors)
    {
        return false;
    }
    if let Some(counts) = counts {
        shell
            .core
            .workspaces
            .get_mut(team)
            .unwrap()
            .thread_counts
            .apply_snapshot(counts, expected.revisions[1]);
    }
    // Do not derive Activity state from this partial page; Activity has its own
    // authoritative readback next. Neither feed's row count is a badge total.
    shell.refresh_from_core();
    *expected = Snapshot::capture(&shell, team);
    true
}

async fn activity(
    mut state: Signal<ShellState>,
    context: &Context,
    id: u64,
    expected: &mut Snapshot,
) -> bool {
    let team = &context.owner.team;
    let request = {
        let mut shell = state.write();
        if !expected.current(&shell, context, id) {
            return false;
        }
        shell.core.activity.ensure_scope(context.epoch, team);
        let Some(request) = shell.core.activity.begin(false) else {
            return false;
        };
        *expected = Snapshot::capture(&shell, team);
        request
    };
    let result = api::fetch_activity_feed(
        context.transport.as_ref().unwrap(),
        &context.client,
        &context.session,
        20,
        request.cursor.clone(),
        false,
    )
    .await;
    let mut shell = state.write();
    if !expected.current(&shell, context, id) {
        shell.core.activity.fail(&request);
        return false;
    }
    let Ok(page) = result else {
        shell.core.activity.fail(&request);
        return false;
    };
    if !shell.core.activity.apply_page(&request, page) {
        return false;
    }
    shell.refresh_from_core();
    *expected = Snapshot::capture(&shell, team);
    true
}

async fn history(
    mut state: Signal<ShellState>,
    context: &Context,
    id: u64,
    expected: &mut Snapshot,
) -> bool {
    let channel = {
        let shell = state.peek();
        if !expected.current(&shell, context, id) {
            return false;
        }
        if !shell.transcript_visible() {
            return true;
        }
        let Some(channel) = shell.core.active_channel.clone() else {
            return true;
        };
        channel
    };
    let result = api::fetch_history(
        context.transport.as_ref().unwrap(),
        &context.client,
        &context.session,
        api::HistoryArgs {
            channel: channel.clone(),
            limit: Some(100),
            ..Default::default()
        },
    )
    .await;
    let mut shell = state.write();
    if !expected.current(&shell, context, id) {
        return false;
    }
    let Ok(page) = result else { return false };
    let bag = shell
        .core
        .workspaces
        .get_mut(&context.owner.team)
        .unwrap()
        .messages
        .entry(channel)
        .or_default();
    merge_history(bag, page, None);
    shell.refresh_from_core();
    *expected = Snapshot::capture(&shell, &context.owner.team);
    true
}

async fn replies(
    mut state: Signal<ShellState>,
    context: &Context,
    id: u64,
    expected: &mut Snapshot,
) -> bool {
    let (channel, root) = {
        let shell = state.peek();
        if !expected.current(&shell, context, id) {
            return false;
        }
        let Some(root) = shell.thread_root.clone() else {
            return true;
        };
        let Some(channel) = shell.core.active_channel.clone() else {
            return false;
        };
        (channel, root)
    };
    let result = api::fetch_replies(
        context.transport.as_ref().unwrap(),
        &context.client,
        &context.session,
        api::RepliesArgs {
            channel: channel.clone(),
            ts: root.clone(),
            limit: Some(100),
            ..Default::default()
        },
    )
    .await;
    let mut shell = state.write();
    if !expected.current(&shell, context, id) {
        return false;
    }
    let Ok(page) = result else { return false };
    let key = (context.owner.team.clone(), channel, root.clone());
    let cursor = page
        .messages
        .iter()
        .find(|m| m.ts.as_ref() == Some(&root))
        .and_then(|m| m.extra.get("last_read"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    merge_history(
        shell.core.threads.entry(key.clone()).or_default(),
        page,
        Some(&root),
    );
    if let Some(cursor) = cursor {
        shell.core.apply_thread_mark(&key, &cursor);
    }
    shell.refresh_from_core();
    *expected = Snapshot::capture(&shell, &context.owner.team);
    true
}

fn merge_history(bag: &mut ChannelMessages, page: HistoryPage, root: Option<&str>) {
    for message in page.messages {
        if message.ts.as_deref().is_some_and(|ts| bag.was_deleted(ts)) {
            continue;
        }
        let message = super_platinum_core::state::visible_message(message);
        if root.is_some() || super_platinum_core::state::is_channel_timeline_visible(&message) {
            bag.upsert(message);
        }
    }
    // This is a bounded refresh, not a replacement of the user's paging window.
    // Do not change pagination anchors, scroll intent, manual holds, or cursors.
    bag.loaded = true;
    bag.history_failed = false;
    if let Some(root) = root {
        bag.recount_thread_unreads(root);
    }
}

#[cfg(test)]
mod tests;
