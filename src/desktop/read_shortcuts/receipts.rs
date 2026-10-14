//! Cursor writes are single attempt. Uncertain receipts survive shell replacement
//! and require a separate read-only reconciliation followed by a fresh gesture.
use crate::{realtime::activity_counts::CursorOwner, state::ShellState};
use dioxus::prelude::*;
use std::collections::HashMap;
use super_platinum_core::{domain::ReadTarget, slack::api};

type Key = (String, String, ReadTarget);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Sending,
    Uncertain,
    Checking,
    Refreshed,
}
pub(super) struct Receipt {
    epoch: u64,
    ticket: u64,
    ts: String,
    phase: Phase,
    proof: Option<Proof>,
}
struct Proof {
    owner: CursorOwner,
    cursor: String,
    revision: u64,
    subscription: Option<(bool, u64)>,
}
#[derive(Default)]
pub(super) struct Registry {
    entries: HashMap<Key, Receipt>,
}

fn team(target: &ReadTarget) -> &str {
    match target {
        ReadTarget::Conversation { team, .. } | ReadTarget::Thread { team, .. } => team,
    }
}
fn owner_key(shell: &ShellState, target: &ReadTarget) -> Option<Key> {
    Some((
        shell.core.active_account.clone()?,
        shell
            .core
            .workspaces
            .get(team(target))?
            .self_user_id
            .clone(),
        target.clone(),
    ))
}
pub(super) fn busy(shell: &ShellState, target: &ReadTarget) -> bool {
    owner_key(shell, target)
        .is_some_and(|key| shell.read_shortcuts.receipts.entries.contains_key(&key))
}
pub(crate) fn busy_workspace(shell: &ShellState, workspace: &str) -> bool {
    let Some(account) = shell.core.active_account.as_ref() else {
        return false;
    };
    let Some(user) = shell
        .core
        .workspaces
        .get(workspace)
        .map(|w| &w.self_user_id)
    else {
        return false;
    };
    shell
        .read_shortcuts
        .receipts
        .entries
        .keys()
        .any(|(a, u, target)| a == account && u == user && team(target) == workspace)
}
pub(super) fn begin(shell: &mut ShellState, target: &ReadTarget, ts: &str, ticket: u64) -> bool {
    let Some(key) = owner_key(shell, target) else {
        return false;
    };
    if shell.read_shortcuts.receipts.entries.contains_key(&key) {
        return false;
    }
    shell.read_shortcuts.receipts.entries.insert(
        key,
        Receipt {
            epoch: shell.core.account_epoch,
            ticket,
            ts: ts.into(),
            phase: Phase::Sending,
            proof: None,
        },
    );
    true
}
/// Return true only when this exact completion still owns the retained receipt.
/// Stale credential/account responses may preserve uncertainty but cannot ACK.
pub(super) fn finish(
    shell: &mut ShellState,
    account: &str,
    user: &str,
    target: &ReadTarget,
    ticket: u64,
    confirmed: bool,
) -> bool {
    let key = (account.into(), user.into(), target.clone());
    let Some(receipt) = shell
        .read_shortcuts
        .receipts
        .entries
        .get_mut(&key)
        .filter(|r| r.ticket == ticket)
    else {
        return false;
    };
    if confirmed {
        remove(shell, &key);
    } else {
        receipt.phase = Phase::Uncertain;
        receipt.proof = None;
    }
    true
}
fn remove(shell: &mut ShellState, key: &Key) {
    let Some(receipt) = shell.read_shortcuts.receipts.entries.remove(key) else {
        return;
    };
    let pending = (receipt.epoch, key.2.clone());
    if shell.read_shortcuts.pending.get(&pending) == Some(&receipt.ticket) {
        shell.read_shortcuts.pending.remove(&pending);
        if owner_key(shell, &key.2).as_ref() == Some(key) {
            shell
                .core
                .pending_marks
                .remove(&(key.2.clone(), receipt.ts));
        }
    }
}
pub(crate) fn carry_pending(old: &mut ShellState, next: &mut ShellState) {
    next.read_shortcuts
        .pending
        .extend(std::mem::take(&mut old.read_shortcuts.pending));
    next.read_shortcuts
        .receipts
        .entries
        .extend(std::mem::take(&mut old.read_shortcuts.receipts.entries));
}

fn revision(shell: &ShellState, target: &ReadTarget) -> u64 {
    match target {
        ReadTarget::Conversation { team, channel } => shell
            .core
            .workspaces
            .get(team)
            .map_or(0, |w| w.read_revision(channel)),
        ReadTarget::Thread {
            team,
            channel,
            root_ts,
        } => shell
            .core
            .thread_read_revision(&(team.clone(), channel.clone(), root_ts.clone())),
    }
}
fn cursor(shell: &ShellState, target: &ReadTarget) -> Option<String> {
    match target {
        ReadTarget::Conversation { team, channel } => shell
            .core
            .workspaces
            .get(team)?
            .read_cursor(channel)
            .map(str::to_owned),
        ReadTarget::Thread {
            team,
            channel,
            root_ts,
        } => shell
            .core
            .thread_cursor(&(team.clone(), channel.clone(), root_ts.clone()))
            .map(str::to_owned),
    }
}

struct Readback {
    cursor: String,
    subscribed: Option<bool>,
    channel: Option<super_platinum_core::slack::models::Channel>,
}
fn channel_baseline(shell: &ShellState, target: &ReadTarget) -> Option<serde_json::Value> {
    let ReadTarget::Conversation { team, channel } = target else {
        return None;
    };
    let workspace = shell.core.workspaces.get(team)?;
    let row = workspace.channels.get(channel)?;
    let bag = workspace.messages.get(channel);
    Some(serde_json::json!([
        row.last_read,
        row.has_unreads,
        row.unread_count,
        row.unread_count_display,
        row.mention_count,
        row.extra.get("latest"),
        row.extra.get("vip_count"),
        bag.map(|b| (&b.latest, b.unread_count, b.mention_count, b.vip_count))
    ]))
}
fn apply_channel_readback(
    shell: &mut ShellState,
    target: &ReadTarget,
    observed: &Readback,
    previous: Option<&str>,
    read_revision: u64,
    baseline: &Option<serde_json::Value>,
) -> bool {
    if baseline.is_none() || channel_baseline(shell, target) != *baseline {
        return false;
    }
    let ReadTarget::Conversation { team, channel } = target else {
        return false;
    };
    let Some(row) = observed.channel.as_ref().filter(|row| {
        &row.id == channel && row.last_read.as_deref() == Some(observed.cursor.as_str())
    }) else {
        return false;
    };
    let Some(workspace) = shell.core.workspaces.get_mut(team) else {
        return false;
    };
    if workspace.read_revision(channel) != read_revision
        || workspace.read_cursor(channel) != previous
    {
        return false;
    }
    workspace.apply_channel_counts_read_state(row);
    workspace.read_cursor(channel) == Some(observed.cursor.as_str())
}
fn thread_key(target: &ReadTarget) -> Option<super_platinum_core::domain::ThreadKey> {
    match target {
        ReadTarget::Thread {
            team,
            channel,
            root_ts,
        } => Some((team.clone(), channel.clone(), root_ts.clone())),
        _ => None,
    }
}
fn subscription(shell: &ShellState, target: &ReadTarget) -> Option<(bool, u64)> {
    let key = thread_key(target)?;
    let registry = &shell.core.thread_subscriptions;
    let epoch = shell.core.account_epoch;
    if registry.pending(epoch, &key) {
        return None;
    }
    Some((
        registry.active(epoch, &key)?,
        registry.revision(epoch, &key),
    ))
}
fn apply_thread_readback(
    shell: &mut ShellState,
    target: &ReadTarget,
    observed: &Readback,
    read_revision: u64,
    subscription_revision: u64,
) -> bool {
    let Some(key) = thread_key(target) else {
        return false;
    };
    let Some(active) = observed.subscribed else {
        return false;
    };
    let epoch = shell.core.account_epoch;
    if revision(shell, target) != read_revision
        || shell.core.thread_subscriptions.pending(epoch, &key)
        || shell.core.thread_subscriptions.revision(epoch, &key) != subscription_revision
    {
        return false;
    }
    if !crate::realtime::activity_counts::confirm_thread_read(
        &mut shell.core,
        &key,
        &observed.cursor,
        read_revision,
    ) {
        return false;
    }
    // The serial shell boundary has no await between the two guarded updates.
    if !shell
        .core
        .thread_subscriptions
        .snapshot(epoch, &key, active, subscription_revision)
    {
        return false;
    }
    shell.core.reconcile_thread_subscription(&key);
    true
}
fn thread_readback(
    messages: &[super_platinum_core::slack::models::Message],
    root: &str,
) -> Option<Readback> {
    let message = messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(root))?;
    // RepliesAPI only publishes subscription cursors for roots carrying their
    // own subscription field. Both fields must be reconciled together.
    let subscribed = message
        .extra
        .get("subscribed")
        .and_then(serde_json::Value::as_bool)?;
    let cursor = message
        .extra
        .get("last_read")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)?;
    Some(Readback {
        cursor,
        subscribed: Some(subscribed),
        channel: None,
    })
}

fn release_refreshed(shell: &mut ShellState, entry: &Key, target: &ReadTarget) -> bool {
    let Some(receipt) = shell.read_shortcuts.receipts.entries.get(entry) else {
        return false;
    };
    if receipt.phase != Phase::Refreshed {
        return false;
    }
    if receipt.proof.as_ref().is_some_and(|proof| {
        proof.owner.current(&shell.core)
            && revision(shell, target) == proof.revision
            && cursor(shell, target).as_deref() == Some(proof.cursor.as_str())
            && subscription(shell, target) == proof.subscription
    }) {
        remove(shell, entry);
        return true;
    }
    let receipt = shell
        .read_shortcuts
        .receipts
        .entries
        .get_mut(entry)
        .unwrap();
    receipt.phase = Phase::Uncertain;
    receipt.proof = None;
    false
}

/// True consumes this gesture in reconciliation. A successful GET never sends
/// a mark; a subsequent gesture releases Refreshed and makes a new decision.
pub(super) async fn handle(
    mut state: Signal<ShellState>,
    thread: bool,
    focus_generation: u64,
    view_key: &str,
) -> bool {
    let target = {
        let shell = state.peek();
        let (Some(team), Some(channel)) = (
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
        ) else {
            return false;
        };
        if thread {
            let Some(root_ts) = shell.thread_root.clone() else {
                return false;
            };
            ReadTarget::Thread {
                team,
                channel,
                root_ts,
            }
        } else {
            ReadTarget::Conversation { team, channel }
        }
    };
    let (entry, ticket) = {
        let mut shell = state.write();
        let Some(entry) = owner_key(&shell, &target) else {
            return false;
        };
        let Some(receipt) = shell.read_shortcuts.receipts.entries.get(&entry) else {
            return false;
        };
        let ticket = receipt.ticket;
        if receipt.phase == Phase::Refreshed {
            if release_refreshed(&mut shell, &entry, &target) {
                return false;
            }
        }
        let receipt = &shell.read_shortcuts.receipts.entries[&entry];
        if receipt.phase != Phase::Uncertain {
            return true;
        }
        (entry, ticket)
    };
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        return true;
    };
    let Some(session) = sessions.into_iter().find(|s| s.team_id == team(&target)) else {
        return true;
    };
    let (owner, previous, read_revision, subscription_revision, counts_baseline) = {
        let shell = state.peek();
        let Some(owner) = CursorOwner::capture(&shell.core, &transport, &session) else {
            return true;
        };
        let sub_revision = thread_key(&target).map(|key| {
            shell
                .core
                .thread_subscriptions
                .revision(shell.core.account_epoch, &key)
        });
        if thread_key(&target).is_some_and(|key| {
            shell
                .core
                .thread_subscriptions
                .pending(shell.core.account_epoch, &key)
        }) {
            return true;
        }
        (
            owner,
            cursor(&shell, &target),
            revision(&shell, &target),
            sub_revision,
            channel_baseline(&shell, &target),
        )
    };
    state
        .write()
        .read_shortcuts
        .receipts
        .entries
        .get_mut(&entry)
        .unwrap()
        .phase = Phase::Checking;
    let result = match &target {
        ReadTarget::Conversation { channel, .. } => {
            api::fetch_counts(&transport, &client, &session)
                .await
                .map(|page| {
                    page.all_channels()
                        .into_iter()
                        .find(|c| &c.id == channel)
                        .and_then(|row| {
                            Some(Readback {
                                cursor: row.last_read.clone()?,
                                subscribed: None,
                                channel: Some(row),
                            })
                        })
                })
        }
        ReadTarget::Thread {
            channel, root_ts, ..
        } => api::fetch_replies(
            &transport,
            &client,
            &session,
            api::RepliesArgs {
                channel: channel.clone(),
                ts: root_ts.clone(),
                limit: Some(1),
                ..Default::default()
            },
        )
        .await
        .map(|page| thread_readback(&page.messages, root_ts)),
    };
    let focused = document::eval(&format!(
        "return window.superPlatinumReadShortcuts?.currentFocus({}, {});",
        focus_generation,
        serde_json::to_string(view_key).unwrap()
    ))
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    let mut shell = state.write();
    if !shell
        .read_shortcuts
        .receipts
        .entries
        .get(&entry)
        .is_some_and(|r| r.ticket == ticket && r.phase == Phase::Checking)
    {
        return true;
    }
    shell
        .read_shortcuts
        .receipts
        .entries
        .get_mut(&entry)
        .unwrap()
        .phase = Phase::Uncertain;
    if !owner.current(&shell.core)
        || !focused
        || super::key(&shell) != view_key
        || !crate::read_visibility::focused()
        || owner_key(&shell, &target).as_ref() != Some(&entry)
        || revision(&shell, &target) != read_revision
    {
        return true;
    }
    let Ok(Some(observed)) = result else {
        shell.show_toast(
            "Read status is still unconfirmed. Escape can check again without resending.",
        );
        return true;
    };
    if super_platinum_core::state::latest_channel_read_target(Some(&observed.cursor), []).is_none()
    {
        return true;
    }
    let accepted = match &target {
        ReadTarget::Conversation { .. } => apply_channel_readback(
            &mut shell,
            &target,
            &observed,
            previous.as_deref(),
            read_revision,
            &counts_baseline,
        ),
        ReadTarget::Thread { .. } => apply_thread_readback(
            &mut shell,
            &target,
            &observed,
            read_revision,
            subscription_revision.unwrap(),
        ),
    };
    if accepted {
        let proof = Proof {
            owner,
            cursor: observed.cursor,
            revision: revision(&shell, &target),
            subscription: subscription(&shell, &target),
        };
        shell
            .read_shortcuts
            .receipts
            .entries
            .get_mut(&entry)
            .unwrap()
            .phase = Phase::Refreshed;
        shell
            .read_shortcuts
            .receipts
            .entries
            .get_mut(&entry)
            .unwrap()
            .proof = Some(proof);
        shell.refresh_from_core();
        shell.show_toast("Read status refreshed. Press Escape again to continue.");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> ShellState {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.core.active_account = Some("isolated-receipt-account".into());
        s.core.pending_scroll_to = None;
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .get_mut(&channel)
            .unwrap()
            .extra
            .insert("is_member".into(), true.into());
        let bag = s
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap();
        bag.last_read = Some("10.000001".into());
        bag.loaded = true;
        s
    }
    #[test]
    fn lost_ack_blocks_second_escape_and_automatic_read_across_account_shell_return() {
        let mut s = shell();
        let target = ReadTarget::Conversation {
            team: s.core.active_team.clone().unwrap(),
            channel: s.core.active_channel.clone().unwrap(),
        };
        let account = s.core.active_account.clone().unwrap();
        let user = s.core.workspaces[team(&target)].self_user_id.clone();
        assert!(crate::read_preferences::renderer::can_read(&s, true, true));
        assert!(begin(&mut s, &target, "20.000001", 9));
        assert!(finish(&mut s, &account, &user, &target, 9, false));
        assert!(busy(&s, &target));
        assert!(!begin(&mut s, &target, "20.000001", 10));
        assert!(super::super::busy_channel(&s));
        assert!(!crate::read_preferences::renderer::can_read(&s, true, true));
        let mut next = shell();
        next.core.account_epoch += 1;
        carry_pending(&mut s, &mut next);
        assert!(busy(&next, &target));
        assert!(!begin(&mut next, &target, "20.000001", 11));
        assert!(!finish(&mut next, &account, &user, &target, 99, true));
        assert!(busy(&next, &target));
        assert!(finish(&mut next, &account, &user, &target, 9, true));
        assert!(!busy(&next, &target));
    }

    fn refreshed() -> (ShellState, ReadTarget, Key) {
        let mut s = shell();
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        let target = ReadTarget::Conversation {
            team: team.clone(),
            channel,
        };
        let session = super_platinum_core::config::WorkspaceSession {
            team_id: team.clone(),
            user_id: s.core.workspaces[&team].self_user_id.clone(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-token".into(),
        };
        let transport = std::sync::Arc::new(
            super_platinum_core::slack::Transport::new("fixture-cookie").unwrap(),
        );
        s.core.transport = Some(transport.clone());
        s.core.session = Some(super_platinum_core::config::Session {
            d_cookie: "fixture-cookie".into(),
            workspaces: [(team, session.clone())].into(),
        });
        let proof = Proof {
            owner: CursorOwner::capture(&s.core, &transport, &session).unwrap(),
            cursor: cursor(&s, &target).unwrap(),
            revision: revision(&s, &target),
            subscription: None,
        };
        let key = owner_key(&s, &target).unwrap();
        begin(&mut s, &target, "20.000001", 12);
        let receipt = s.read_shortcuts.receipts.entries.get_mut(&key).unwrap();
        receipt.phase = Phase::Refreshed;
        receipt.proof = Some(proof);
        (s, target, key)
    }
    #[test]
    fn refreshed_proof_cannot_release_after_cursor_revision_or_credential_changes() {
        for mutation in 0..3 {
            let (mut s, target, key) = refreshed();
            if mutation == 0 {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(team(&target))
                    .unwrap()
                    .token = "rotated-fixture-token".into();
            } else if let ReadTarget::Conversation { team, channel } = &target {
                let bag = s
                    .core
                    .workspaces
                    .get_mut(team)
                    .unwrap()
                    .messages
                    .get_mut(channel)
                    .unwrap();
                if mutation == 1 {
                    bag.read_revision += 1;
                } else {
                    bag.last_read = Some("99.000001".into());
                }
            }
            assert!(!release_refreshed(&mut s, &key, &target));
            assert!(busy(&s, &target));
            assert_eq!(
                s.read_shortcuts.receipts.entries[&key].phase,
                Phase::Uncertain
            );
        }
        let (mut s, target, key) = refreshed();
        assert!(release_refreshed(&mut s, &key, &target));
        assert!(!busy(&s, &target));
    }

    #[test]
    fn unsubscribed_thread_readback_recovers_without_reading_again_and_guards_subscription_aba() {
        for mutation in 0..3 {
            let (mut s, _, channel_entry) = refreshed();
            let owner = s
                .read_shortcuts
                .receipts
                .entries
                .remove(&channel_entry)
                .unwrap()
                .proof
                .unwrap()
                .owner;
            crate::thread_unread::fixture(&mut s);
            s.thread_loading = false;
            let key = crate::thread_unread::key(&s).unwrap();
            let epoch = s.core.account_epoch;
            let target = ReadTarget::Thread {
                team: key.0.clone(),
                channel: key.1.clone(),
                root_ts: key.2.clone(),
            };
            s.core.thread_subscriptions.apply(epoch, &key, true);
            s.core.reconcile_thread_subscription(&key);
            let sub_revision = s.core.thread_subscriptions.revision(epoch, &key);
            let read_revision = revision(&s, &target);
            let root: super_platinum_core::slack::models::Message = serde_json::from_value(
                serde_json::json!({"ts":key.2,"subscribed":false,"last_read":"1719800404.000000"}),
            )
            .unwrap();
            let observed = thread_readback(&[root], &key.2).unwrap();
            assert!(apply_thread_readback(
                &mut s,
                &target,
                &observed,
                read_revision,
                sub_revision
            ));
            assert_eq!(crate::thread_subscription::active(&s), Some(false));
            assert!(begin(&mut s, &target, &observed.cursor, 15));
            let entry = owner_key(&s, &target).unwrap();
            let proof = Proof {
                owner,
                cursor: observed.cursor,
                revision: revision(&s, &target),
                subscription: subscription(&s, &target),
            };
            let receipt = s.read_shortcuts.receipts.entries.get_mut(&entry).unwrap();
            receipt.phase = Phase::Refreshed;
            receipt.proof = Some(proof);
            if mutation == 1 {
                s.core.thread_subscriptions.apply(epoch, &key, true);
                s.core.thread_subscriptions.apply(epoch, &key, false);
            }
            if mutation == 2 {
                s.core
                    .thread_subscriptions
                    .begin(epoch, &key, true)
                    .unwrap();
            }
            assert_eq!(release_refreshed(&mut s, &entry, &target), mutation == 0);
            assert_eq!(busy(&s, &target), mutation != 0);
            if mutation == 0 {
                assert!(!busy_workspace(&s, &key.0));
                assert_eq!(
                    super_platinum_core::state::thread_escape(
                        crate::thread_subscription::active(&s),
                        None,
                        s.core.thread_cursor(&key),
                        Some("1719800404.000000")
                    ),
                    super_platinum_core::state::ThreadEscape::NoRead
                );
            }
        }
    }
    #[test]
    fn thread_readback_cannot_override_subscription_change_during_get() {
        let mut s = shell();
        crate::thread_unread::fixture(&mut s);
        let key = crate::thread_unread::key(&s).unwrap();
        let epoch = s.core.account_epoch;
        let target = ReadTarget::Thread {
            team: key.0.clone(),
            channel: key.1.clone(),
            root_ts: key.2.clone(),
        };
        let sub_revision = s.core.thread_subscriptions.revision(epoch, &key);
        let read_revision = revision(&s, &target);
        let previous = cursor(&s, &target);
        s.core.thread_subscriptions.apply(epoch, &key, true);
        assert!(!apply_thread_readback(
            &mut s,
            &target,
            &Readback {
                cursor: "1719800404.000000".into(),
                subscribed: Some(false),
                channel: None
            },
            read_revision,
            sub_revision
        ));
        assert_eq!(cursor(&s, &target), previous);
        assert_eq!(crate::thread_subscription::active(&s), Some(true));
    }

    #[test]
    fn lower_counts_cursor_recovers_uncertain_write_without_erasing_remote_unread() {
        for mutation in 0..4 {
            let (mut s, target, entry) = refreshed();
            let ReadTarget::Conversation { team, channel } = &target else {
                unreachable!()
            };
            let previous = cursor(&s, &target);
            let read_revision = revision(&s, &target);
            let baseline = channel_baseline(&s, &target);
            let observed=Readback{cursor:"5.000001".into(),subscribed:None,channel:Some(serde_json::from_value(serde_json::json!({"id":channel,"last_read":"5.000001","has_unreads":true,"unread_count":4,"mention_count":2})).unwrap())};
            if mutation == 1 {
                s.core
                    .workspaces
                    .get_mut(team)
                    .unwrap()
                    .messages
                    .get_mut(channel)
                    .unwrap()
                    .read_revision += 1;
            }
            if mutation == 2 {
                let bag = s
                    .core
                    .workspaces
                    .get_mut(team)
                    .unwrap()
                    .messages
                    .get_mut(channel)
                    .unwrap();
                bag.unread_count += 1;
                bag.mention_count += 1;
                bag.latest = Some("9999999999.000001".into());
            }
            if mutation == 3 {
                s.core
                    .workspaces
                    .get_mut(team)
                    .unwrap()
                    .messages
                    .entry("unrelated-channel".into())
                    .or_default()
                    .unread_count += 1;
            }
            let rejected = mutation == 1 || mutation == 2;
            let current_counts = {
                let bag = &s.core.workspaces[team].messages[channel];
                (bag.unread_count, bag.mention_count, bag.latest.clone())
            };
            assert_eq!(
                apply_channel_readback(
                    &mut s,
                    &target,
                    &observed,
                    previous.as_deref(),
                    read_revision,
                    &baseline
                ),
                !rejected
            );
            if rejected {
                assert_eq!(cursor(&s, &target), previous);
                let bag = &s.core.workspaces[team].messages[channel];
                assert_eq!(
                    (bag.unread_count, bag.mention_count, bag.latest.clone()),
                    current_counts
                );
                continue;
            }
            assert_eq!(cursor(&s, &target).as_deref(), Some("5.000001"));
            let bag = &s.core.workspaces[team].messages[channel];
            assert_eq!((bag.unread_count, bag.mention_count), (4, 2));
            let revision = revision(&s, &target);
            let proof = s
                .read_shortcuts
                .receipts
                .entries
                .get_mut(&entry)
                .unwrap()
                .proof
                .as_mut()
                .unwrap();
            proof.cursor = observed.cursor;
            proof.revision = revision;
            assert!(release_refreshed(&mut s, &entry, &target));
            assert!(!busy_workspace(&s, team));
        }
    }
}
