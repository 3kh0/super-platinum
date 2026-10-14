//! Explicit conversation read-cursor changes and their per-visit auto-read hold.

use crate::state::ShellState;
use dioxus::prelude::*;
use std::{collections::HashSet, sync::Arc};
use super_platinum_core::{
    domain::ReadTarget,
    slack::api::{self, HistoryArgs},
};

type Key = (u64, String, String);

#[derive(Default)]
pub(crate) struct UnreadState {
    pending: HashSet<Key>,
    /// Navigation scope and cursor. Reopening a conversation ends its hold.
    held: Option<(String, String)>,
}

fn key(shell: &ShellState, channel: &str) -> Option<Key> {
    Some((
        shell.core.account_epoch,
        shell.core.active_team.clone()?,
        channel.into(),
    ))
}

pub(crate) fn busy(shell: &ShellState, channel: &str) -> bool {
    key(shell, channel).is_some_and(|key| shell.manual_unread.pending.contains(&key))
}

pub(crate) fn busy_in(shell: &ShellState, epoch: u64, team: &str, channel: &str) -> bool {
    shell
        .manual_unread
        .pending
        .contains(&(epoch, team.into(), channel.into()))
}

pub(crate) fn held(shell: &ShellState) -> bool {
    shell
        .manual_unread
        .held
        .as_ref()
        .is_some_and(|(scope, _)| scope == &shell.timeline_scope())
}

pub(crate) fn release(shell: &mut ShellState) {
    shell.manual_unread.held = None;
}

pub(crate) fn blocks_auto_read(shell: &ShellState) -> bool {
    held(shell)
        || shell
            .core
            .active_channel
            .as_deref()
            .is_some_and(|channel| busy(shell, channel))
}

/// `Some(ts)` marks that message and everything after it unread. `None` marks
/// the open conversation read. Requests outlive their originating row.
pub(crate) fn change(mut state: Signal<ShellState>, channel: String, target: Option<String>) {
    let (request_key, scope, old_hold) = {
        let mut shell = state.write();
        let Some(request_key) = key(&shell, &channel) else {
            return;
        };
        if shell.core.active_channel.as_ref() != Some(&channel)
            || !shell.transcript_visible()
            || busy(&shell, &channel)
            || target
                .as_ref()
                .is_some_and(|ts| !shell.channel_rows.iter().any(|m| &m.ts == ts && !m.pending))
        {
            return;
        }
        let scope = shell.timeline_scope();
        let old_hold = shell.manual_unread.held.clone();
        shell.manual_unread.pending.insert(request_key.clone());
        if let Some(ts) = target.as_ref() {
            shell.manual_unread.held = Some((scope.clone(), ts.clone()));
        }
        (request_key, scope, old_hold)
    };
    dioxus::core::spawn_forever(async move {
        let result = run(state, &request_key, target.as_deref()).await;
        let mut shell = state.write();
        shell.manual_unread.pending.remove(&request_key);
        if shell.core.account_epoch != request_key.0 {
            return;
        }
        match result {
            Ok(Some(cursor)) => {
                if shell.timeline_scope() == scope {
                    if target.is_some() {
                        shell.manual_unread.held = Some((scope, cursor.clone()));
                        shell.unread_anchor = Some((channel.clone(), cursor));
                        shell.core.pending_scroll_to = None;
                        shell.stick_to_bottom = false;
                    } else {
                        shell.manual_unread.held = None;
                    }
                }
                shell.refresh_from_core();
            }
            Ok(None) => {
                // Another client moved the cursor while this request completed.
                if shell.timeline_scope() == scope {
                    release(&mut shell);
                }
            }
            Err(error) => {
                restore_hold(&mut shell, &scope, old_hold);
                if let Some(error) = error {
                    shell.report_failure(
                        &error,
                        "Read status cannot change while offline.",
                        || "Could not change read status. Please try again.".into(),
                    );
                }
            }
        }
        drop(shell);
        crate::bootstrap::persist_workspace(&state, &request_key.1);
    });
}

async fn run(
    mut state: Signal<ShellState>,
    key: &Key,
    target: Option<&str>,
) -> Result<Option<String>, Option<super_platinum_core::slack::Error>> {
    if crate::fixture::is_fixture() {
        let mut shell = state.write();
        let cursor = local_cursor(&shell, target).ok_or(None)?;
        apply(&mut shell, key, &cursor, target.is_some());
        return Ok(Some(cursor));
    }
    let (transport, client, sessions) = crate::bootstrap::credentials(&state).ok_or(None)?;
    let session = sessions
        .into_iter()
        .find(|session| session.team_id == key.1)
        .ok_or(None)?;
    let current = |shell: &ShellState| {
        shell.core.account_epoch == key.0
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &transport))
    };
    let read_target = ReadTarget::Conversation {
        team: key.1.clone(),
        channel: key.2.clone(),
    };
    // Let an automatic mark already in flight finish first. New automatic
    // marks are blocked by `pending`, so they cannot undo this explicit action.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if !current(&state.peek()) {
            return Err(None);
        }
        if !state
            .peek()
            .core
            .pending_marks
            .iter()
            .any(|(target, _)| target == &read_target)
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err(Some(super_platinum_core::slack::Error::Transport(
                "Read status is still updating".into(),
            )));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let cursor = if let Some(target) = target {
        // The prior visible row can skip deleted/hidden messages or a gap in
        // cached history. Ask Slack for the actual preceding server message.
        let page = api::fetch_history(
            &transport,
            &client,
            &session,
            HistoryArgs {
                channel: key.2.clone(),
                latest: Some(target.into()),
                inclusive: false,
                limit: Some(1),
                ..Default::default()
            },
        )
        .await
        .map_err(Some)?;
        page.messages
            .iter()
            .filter_map(|message| message.ts.as_deref())
            .filter(|ts| super_platinum_core::state::cmp_ts(Some(ts), Some(target)).is_lt())
            .max_by(|a, b| super_platinum_core::state::cmp_ts(Some(a), Some(b)))
            .unwrap_or("0.000000")
            .to_owned()
    } else {
        state
            .peek()
            .core
            .workspaces
            .get(&key.1)
            .and_then(|w| w.messages.get(&key.2))
            .and_then(|list| list.messages.last())
            .and_then(|message| message.ts.clone())
            .ok_or(None)?
    };
    if !current(&state.peek()) {
        return Err(None);
    }
    let owner = crate::realtime::activity_counts::CursorOwner::capture(
        &state.peek().core,
        &transport,
        &session,
    )
    .ok_or(None)?;
    let previous = state
        .peek()
        .core
        .workspaces
        .get(&key.1)
        .and_then(|w| w.read_cursor(&key.2))
        .map(str::to_owned);
    let revision = state
        .peek()
        .core
        .workspaces
        .get(&key.1)
        .map_or(0, |w| w.read_revision(&key.2));
    api::mark_channel(&transport, &client, &session, key.2.clone(), cursor.clone())
        .await
        .map_err(Some)?;
    let mut shell = state.write();
    if !current(&shell) || !owner.current(&shell.core) {
        return Err(None);
    }
    Ok(confirm(
        &mut shell,
        key,
        &cursor,
        previous.as_deref(),
        revision,
        target.is_some(),
    )
    .then_some(cursor))
}

fn confirm(
    shell: &mut ShellState,
    key: &Key,
    cursor: &str,
    previous: Option<&str>,
    revision: u64,
    unread: bool,
) -> bool {
    let Some(workspace) = shell.core.workspaces.get(&key.1) else {
        return false;
    };
    if !workspace.accepts_read_ack(&key.2, cursor, previous, revision) {
        return false;
    }
    let current = workspace.read_cursor(&key.2);
    // Socket acknowledgements carry exact counts; never replace them with an
    // estimate. A different intervening cursor outranks a delayed HTTP response.
    if current == Some(cursor) {
        crate::realtime::activity_counts::mark_channel_read(
            &mut shell.core,
            &key.1,
            &key.2,
            cursor,
        );
        return true;
    }
    if current != previous {
        return false;
    }
    apply(shell, key, cursor, unread);
    true
}

fn restore_hold(shell: &mut ShellState, scope: &str, previous: Option<(String, String)>) {
    if shell
        .manual_unread
        .held
        .as_ref()
        .is_some_and(|(held_scope, _)| held_scope == scope)
    {
        shell.manual_unread.held = previous;
    }
}

fn local_cursor(shell: &ShellState, target: Option<&str>) -> Option<String> {
    match target {
        None => shell.channel_rows.last().map(|m| m.ts.clone()),
        Some(target) => Some(
            shell
                .channel_rows
                .iter()
                .filter(|m| super_platinum_core::state::cmp_ts(Some(&m.ts), Some(target)).is_lt())
                .map(|m| m.ts.as_str())
                .max_by(|a, b| super_platinum_core::state::cmp_ts(Some(a), Some(b)))
                .unwrap_or("0.000000")
                .into(),
        ),
    }
}

fn apply(shell: &mut ShellState, key: &Key, cursor: &str, unread: bool) {
    if let Some(workspace) = shell.core.workspaces.get_mut(&key.1) {
        if unread {
            workspace.mark_unread_locally(&key.2, cursor);
        } else {
            let previous = workspace.read_cursor(&key.2).map(str::to_owned);
            workspace.confirm_read(&key.2, cursor, previous.as_deref());
        }
    }
    crate::realtime::activity_counts::mark_channel_read(&mut shell.core, &key.1, &key.2, cursor);
}

pub(crate) fn fixture(shell: &mut ShellState) {
    let Some(channel) = shell.core.active_channel.clone() else {
        return;
    };
    let Some(key) = key(shell, &channel) else {
        return;
    };
    shell.refresh_from_core();
    let target = shell
        .channel_rows
        .get(shell.channel_rows.len().saturating_sub(2))
        .map(|m| m.ts.clone());
    let Some(cursor) = local_cursor(shell, target.as_deref()) else {
        return;
    };
    apply(shell, &key, &cursor, true);
    shell.manual_unread.held = Some((shell.timeline_scope(), cursor.clone()));
    shell.unread_anchor = Some((channel, cursor));
    shell.refresh_from_core();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_cursors_use_complete_history_when_rich_rows_are_windowed() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages = (1..=1000)
            .map(|index| {
                serde_json::from_value(serde_json::json!({
                    "ts":format!("{index}.000001"),"user":"U1","text":"Synthetic history"
                }))
                .unwrap()
            })
            .collect();
        shell.core.pending_scroll_to = None;
        shell.stick_to_bottom = false;
        shell.refresh_from_core();
        shell.set_timeline_window(400, 410, [], false);
        assert!(shell.messages.len() <= 100);
        assert_eq!(local_cursor(&shell, None).as_deref(), Some("1000.000001"));
        assert_eq!(
            local_cursor(&shell, Some("900.000001")).as_deref(),
            Some("899.000001")
        );
        assert_eq!(
            local_cursor(&shell, Some("1.000001")).as_deref(),
            Some("0.000000")
        );
    }
    use crate::media::MediaRegistry;

    #[test]
    fn unread_hold_blocks_automatic_marks_until_reopening() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell.core.pending_scroll_to = None;
        shell.stick_to_bottom = true;
        let scope = shell.timeline_scope();
        shell.manual_unread.held = Some((scope, "100.0".into()));
        assert!(held(&shell));
        assert!(!shell.reading_latest(true));
        shell.channel_generation += 1;
        assert!(!held(&shell));
        assert!(shell.reading_latest(true));
    }

    #[test]
    fn pending_request_blocks_only_its_account_workspace_and_channel() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let channel = shell.core.active_channel.clone().unwrap();
        shell
            .manual_unread
            .pending
            .insert(key(&shell, &channel).unwrap());
        assert!(blocks_auto_read(&shell));
        assert!(!busy(&shell, "other"));
        shell.core.account_epoch += 1;
        assert!(!blocks_auto_read(&shell));
    }

    #[test]
    fn first_message_uses_zero_cursor_and_fixture_projects_the_divider() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let first = shell.channel_rows.first().unwrap().ts.clone();
        assert_eq!(
            local_cursor(&shell, Some(&first)).as_deref(),
            Some("0.000000")
        );
        fixture(&mut shell);
        assert!(held(&shell));
        assert_eq!(
            shell
                .channel_rows
                .iter()
                .filter(|m| m.show_unread_divider)
                .count(),
            1
        );
    }
    #[test]
    fn delayed_http_result_preserves_socket_counts_and_newer_cursor() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let channel = shell.core.active_channel.clone().unwrap();
        let key = key(&shell, &channel).unwrap();
        let list = shell
            .core
            .workspaces
            .get_mut(&key.1)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap();
        list.last_read = Some("50.0".into());
        list.unread_count = 7;
        assert!(confirm(&mut shell, &key, "50.0", Some("100.0"), 0, true));
        assert_eq!(
            shell.core.workspaces[&key.1].messages[&channel].unread_count,
            7
        );
        assert!(!confirm(&mut shell, &key, "25.0", Some("100.0"), 0, true));
        assert_eq!(
            shell.core.workspaces[&key.1].read_cursor(&channel),
            Some("50.0")
        );
    }

    #[test]
    fn explicit_backward_ack_only_reads_activity_at_or_before_its_cursor() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let channel = shell.core.active_channel.clone().unwrap();
        let key = key(&shell, &channel).unwrap();
        shell.core.activity.ensure_scope(key.0, &key.1);
        shell.core.activity.items.clear();
        for ts in ["25.0", "75.0"] {
            shell.core.activity.upsert(
                serde_json::from_value(serde_json::json!({
                    "key":ts,"feed_ts":ts,"is_unread":true,
                    "item":{"type":"at_user","message":{"channel":channel,"ts":ts}}
                }))
                .unwrap(),
            );
        }
        shell
            .core
            .workspaces
            .get_mut(&key.1)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap()
            .last_read = Some("100.0".into());
        let revision = shell.core.workspaces[&key.1].read_revision(&channel);
        assert!(confirm(
            &mut shell,
            &key,
            "50.0",
            Some("100.0"),
            revision,
            true
        ));
        assert_eq!(shell.core.activity.unread_count(), 1);
        assert!(!confirm(&mut shell, &key, "90.0", Some("100.0"), 0, false));
        assert!(
            shell
                .core
                .activity
                .items
                .iter()
                .find(|item| item.key == "75.0")
                .unwrap()
                .is_unread
        );
    }

    #[test]
    fn explicit_ack_rejects_an_intervening_cursor_aba() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let channel = shell.core.active_channel.clone().unwrap();
        let key = key(&shell, &channel).unwrap();
        let workspace = shell.core.workspaces.get_mut(&key.1).unwrap();
        workspace.mark_unread_locally(&channel, "100.0");
        let revision = workspace.read_revision(&channel);
        workspace.mark_unread_locally(&channel, "300.0");
        workspace.mark_unread_locally(&channel, "100.0");
        for unread in [false, true] {
            assert!(!confirm(
                &mut shell,
                &key,
                "200.0",
                Some("100.0"),
                revision,
                unread
            ));
            assert_eq!(
                shell.core.workspaces[&key.1].read_cursor(&channel),
                Some("100.0")
            );
        }
    }

    #[test]
    fn failure_restores_prior_hold_without_touching_a_new_visit() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let scope = shell.timeline_scope();
        let previous = Some((scope.clone(), "50.0".into()));
        shell.manual_unread.held = Some((scope.clone(), "100.0".into()));
        restore_hold(&mut shell, &scope, previous.clone());
        assert_eq!(shell.manual_unread.held, previous);
        shell.channel_generation += 1;
        let next = Some((shell.timeline_scope(), "200.0".into()));
        shell.manual_unread.held = next.clone();
        restore_hold(&mut shell, &scope, None);
        assert_eq!(shell.manual_unread.held, next);
    }
}
