//! RepliesAPI publishes subscription and cursor authority together.
//! Retained S2 gantry-v2-shared.c8d6ded6605ce42d.min.js SHA256
//! 104c0dd99720b7391f5724ea2eb6d439a4249c017fb6f743f664fdc91ec4a693:
//! rGEL fetchReplies1430018 -> D8Sp preprocess; slurpThreadSubscriptions2807312
//! requires own subscribed and dispatches subscribed plus lastRead together.
use super::{HistoryPage, ShellState, ThreadKey};

#[derive(PartialEq)]
pub(super) struct SubscriptionBaseline {
    key: ThreadKey,
    active: Option<bool>,
    revision: u64,
    pub pending: bool,
}
pub(super) fn capture(shell: &ShellState, team: &str) -> Option<SubscriptionBaseline> {
    let key = (
        team.into(),
        shell.core.active_channel.clone()?,
        shell.thread_root.clone()?,
    );
    Some(for_key(shell, key))
}
fn for_key(shell: &ShellState, key: ThreadKey) -> SubscriptionBaseline {
    let registry = &shell.core.thread_subscriptions;
    let epoch = shell.core.account_epoch;
    SubscriptionBaseline {
        active: registry.active(epoch, &key),
        revision: registry.revision(epoch, &key),
        pending: registry.pending(epoch, &key),
        key,
    }
}
impl SubscriptionBaseline {
    pub(super) fn accepts(&self, shell: &ShellState, key: &ThreadKey) -> bool {
        !self.pending && &self.key == key && *self == for_key(shell, key.clone())
    }
}
pub(super) struct Proof {
    active: bool,
    cursor: String,
}
pub(super) fn proof(page: &HistoryPage, key: &ThreadKey) -> Option<Proof> {
    let mut roots = page
        .messages
        .iter()
        .filter(|m| m.ts.as_ref() == Some(&key.2));
    let root = roots.next()?;
    // Even equal duplicate roots are an unsupported receipt shape. Do not
    // select whichever copy happens to arrive first as cursor authority.
    if roots.next().is_some() {
        return None;
    }
    if root
        .channel
        .as_ref()
        .is_some_and(|channel| channel != &key.1)
    {
        return None;
    }
    let active = root.extra.get("subscribed")?.as_bool()?;
    // Missing last_read can be forwarded by Slack's subscription preprocessing,
    // but cannot prove full bulk read-state reconciliation. Leave this entire
    // stage unapplied and expose refresh failure for an explicit retry.
    let cursor = root.extra.get("last_read")?.as_str()?;
    // B2 getRecordThreadIsUnread7267950 and getSurfaceCardThreadIsUnread12028904
    // use explicit "0" as a thread lastRead baseline. This establishes selector
    // semantics, not an observed RepliesAPI wire value. Never synthesize it for
    // absence. Other values reuse core's supported numeric timestamp parser.
    if cursor != "0"
        && super_platinum_core::state::latest_channel_read_target(Some(cursor), std::iter::empty())
            != Some(cursor)
    {
        return None;
    }
    Some(Proof {
        active,
        cursor: cursor.into(),
    })
}
pub(super) fn apply(
    shell: &mut ShellState,
    key: &ThreadKey,
    baseline: &SubscriptionBaseline,
    proof: Proof,
) -> bool {
    // Validate before touching either metadata or content. All subsequent
    // updates occur inside the same serial shell write guard without an await.
    if !baseline.accepts(shell, key) {
        return false;
    }
    let epoch = shell.core.account_epoch;
    let accepted =
        shell
            .core
            .thread_subscriptions
            .snapshot(epoch, key, proof.active, baseline.revision);
    if !accepted {
        return false;
    }
    shell.core.apply_thread_mark(key, &proof.cursor);
    shell.core.reconcile_thread_subscription(key);
    true
}
