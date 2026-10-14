//! Explicit thread read state and the automatic-read hold for one pane visit.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    domain::{ReadTarget, ThreadKey},
    slack::{Error, api},
};

#[derive(Default)]
pub(crate) struct ThreadUnreadState {
    pending: HashSet<(u64, ThreadKey)>,
    held: Option<String>,
    anchor: Option<(String, String)>,
}

pub(crate) fn key(shell: &ShellState) -> Option<ThreadKey> {
    Some((
        shell.core.active_team.clone()?,
        shell.core.active_channel.clone()?,
        shell.thread_root.clone()?,
    ))
}
pub(crate) fn busy(shell: &ShellState) -> bool {
    key(shell).is_some_and(|key| {
        shell
            .thread_unread
            .pending
            .contains(&(shell.core.account_epoch, key))
    })
}
pub(crate) fn held(shell: &ShellState) -> bool {
    shell.thread_unread.held.as_deref() == Some(shell.thread_scope().as_str())
}
pub(crate) fn busy_workspace(shell: &ShellState, epoch: u64, team: &str) -> bool {
    shell
        .thread_unread
        .pending
        .iter()
        .any(|(e, (t, _, _))| *e == epoch && t == team)
}

pub(crate) fn shortcut_initial_cursor(shell: &ShellState) -> Option<&str> {
    shell
        .thread_unread
        .anchor
        .as_ref()
        .filter(|(scope, _)| scope == &shell.thread_scope())
        .map(|(_, cursor)| cursor.as_str())
}

/// An explicit Escape consumes this pane's retained divider without moving its
/// scroll position or replacing the local thread draft.
pub(crate) fn clear_shortcut_divider(shell: &mut ShellState) {
    let cursor = key(shell)
        .and_then(|key| shell.core.thread_cursor(&key))
        .map(str::to_owned);
    shell.thread_unread.anchor = cursor.map(|cursor| (shell.thread_scope(), cursor));
    shell.thread_unread.held = None;
}
pub(crate) fn blocks_auto_read(shell: &ShellState) -> bool {
    busy(shell) || held(shell)
}

/// Another Slack client can read or reposition the same thread. Reflect its
/// confirmed cursor without retaining an obsolete local status banner.
pub(crate) fn reconcile(shell: &mut ShellState) {
    if !held(shell) || busy(shell) {
        return;
    }
    let Some(key) = key(shell) else {
        return;
    };
    let Some(cursor) = shell.core.thread_cursor(&key).map(str::to_owned) else {
        return;
    };
    if shell
        .thread_unread
        .anchor
        .as_ref()
        .is_some_and(|(_, anchor)| anchor == &cursor)
    {
        return;
    }
    shell.thread_unread.anchor = Some((shell.thread_scope(), cursor));
    if shell
        .core
        .threads
        .get(&key)
        .is_some_and(|bag| bag.unread_count == 0)
    {
        shell.thread_unread.held = None;
    }
}

pub(crate) fn remember_anchor(shell: &mut ShellState) {
    let scope = shell.thread_scope();
    if shell
        .thread_unread
        .anchor
        .as_ref()
        .is_some_and(|(visit, _)| visit == &scope)
    {
        return;
    }
    if let Some(cursor) = key(shell)
        .and_then(|key| shell.core.thread_cursor(&key))
        .map(str::to_owned)
    {
        shell.thread_unread.anchor = Some((scope, cursor));
    }
}

pub(crate) fn annotate(shell: &mut ShellState) {
    let scope = shell.thread_scope();
    let cursor = shell
        .thread_unread
        .anchor
        .as_ref()
        .filter(|(visit, _)| visit == &scope)
        .map(|(_, ts)| ts.clone())
        .or_else(|| {
            key(shell)
                .and_then(|key| shell.core.thread_cursor(&key))
                .map(str::to_owned)
        });
    let exact = shell
        .thread_window
        .rows
        .iter()
        .find(|row| {
            shell.thread_root.as_ref() != Some(&row.ts)
                && cursor.as_deref().is_some_and(|cursor| {
                    super_platinum_core::state::cmp_ts(Some(&row.ts), Some(cursor)).is_gt()
                })
        })
        .map(|row| row.ts.as_str());
    let mut found = false;
    for message in &mut shell.thread_messages {
        message.show_unread_divider = exact.is_none_or(|ts| ts == message.ts)
            && !found
            && shell.thread_root.as_ref() != Some(&message.ts)
            && cursor.as_deref().is_some_and(|cursor| {
                super_platinum_core::state::cmp_ts(Some(&message.ts), Some(cursor)).is_gt()
            });
        if message.show_unread_divider {
            message.compact = false;
            found = true;
        }
    }
}

pub(crate) fn change(mut state: Signal<ShellState>, target: Option<String>) {
    let (key, epoch, scope, previous_hold) = {
        let mut shell = state.write();
        let Some(key) = key(&shell) else { return };
        if crate::read_shortcuts::bulk::blocks_reads(&shell)
            || crate::read_shortcuts::busy_thread(&shell)
            || busy(&shell)
            || shell.thread_loading
            || target.as_ref().is_some_and(|ts| {
                !shell
                    .thread_messages
                    .iter()
                    .any(|m| &m.ts == ts && !m.pending)
            })
        {
            return;
        }
        let epoch = shell.core.account_epoch;
        let scope = shell.thread_scope();
        let previous_hold = shell.thread_unread.held.clone();
        shell.thread_unread.pending.insert((epoch, key.clone()));
        if target.is_some() {
            shell.thread_unread.held = Some(scope.clone());
        }
        (key, epoch, scope, previous_hold)
    };
    dioxus::core::spawn_forever(async move {
        let result = run(state, &key, epoch, target.as_deref()).await;
        let mut shell = state.write();
        shell.thread_unread.pending.remove(&(epoch, key.clone()));
        if shell.core.account_epoch != epoch {
            return;
        }
        match result {
            Ok(cursor) => {
                if shell.thread_scope() == scope {
                    if let Some(cursor) = cursor.filter(|_| target.is_some()) {
                        shell.thread_unread.held = Some(scope.clone());
                        shell.thread_unread.anchor = Some((scope, cursor));
                        shell.thread_target = None;
                        shell.thread_at_bottom = false;
                    } else {
                        shell.thread_unread.held = None;
                    }
                }
                shell.refresh_from_core();
            }
            Err(error) => {
                if shell.thread_scope() == scope {
                    shell.thread_unread.held = previous_hold;
                }
                if let Some(error) = error {
                    shell.report_failure(
                        &error,
                        "Thread read status cannot change while offline.",
                        || "Could not change thread read status. Please try again.".into(),
                    );
                }
            }
        }
    });
}

async fn run(
    mut state: Signal<ShellState>,
    key: &ThreadKey,
    epoch: u64,
    target: Option<&str>,
) -> Result<Option<String>, Option<Error>> {
    let ts = target
        .map(str::to_owned)
        .or_else(|| {
            state
                .peek()
                .core
                .threads
                .get(key)
                .and_then(|bag| {
                    bag.messages
                        .iter()
                        .rev()
                        .find(|message| message.ts.as_deref().is_some_and(|ts| !bag.is_pending(ts)))
                })
                .and_then(|m| m.ts.clone())
        })
        .ok_or(None)?;
    let cursor = if target.is_some() {
        super_platinum_core::thread_reads::before(&ts).ok_or(None)?
    } else {
        ts.clone()
    };
    if crate::fixture::is_fixture() {
        let mut shell = state.write();
        shell.core.apply_thread_mark(key, &cursor);
        crate::realtime::activity_counts::mark_thread_read(
            &mut shell.core,
            &key.0,
            &key.1,
            &key.2,
            &cursor,
        );
        return Ok(Some(cursor));
    }
    let (transport, client, sessions) = crate::bootstrap::credentials(&state).ok_or(None)?;
    let session = sessions
        .into_iter()
        .find(|session| session.team_id == key.0)
        .ok_or(None)?;
    let current = |shell: &ShellState| {
        shell.core.account_epoch == epoch
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &transport))
    };
    let read_target = ReadTarget::Thread {
        team: key.0.clone(),
        channel: key.1.clone(),
        root_ts: key.2.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(30);
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
        if Instant::now() >= deadline {
            return Err(Some(Error::Transport(
                "Thread read status is still updating".into(),
            )));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let revision = state.peek().core.thread_read_revision(key);
    let owner = crate::realtime::activity_counts::CursorOwner::capture(
        &state.peek().core,
        &transport,
        &session,
    )
    .ok_or(None)?;
    api::set_thread_read(
        &transport,
        &client,
        &session,
        key.1.clone(),
        key.2.clone(),
        ts,
        target.is_none(),
    )
    .await
    .map_err(Some)?;
    let mut shell = state.write();
    if !current(&shell) || !owner.current(&shell.core) {
        return Err(None);
    }
    Ok(crate::realtime::activity_counts::confirm_thread_read(
        &mut shell.core,
        key,
        &cursor,
        revision,
    )
    .then_some(cursor))
}

pub(crate) fn fixture(shell: &mut ShellState) {
    let root = "1719800400.000000";
    shell.thread_root = Some(root.into());
    let key = key(shell).unwrap();
    let bag = shell.core.threads.entry(key.clone()).or_default();
    for index in 0..5 {
        bag.upsert(serde_json::from_value(serde_json::json!({"ts":format!("{}.000000",1719800400+index),"user":"U1","thread_ts":root,"text":if index==0 {"A thread with its own unread state.".into()}else{format!("Reply {index}: keep this unread until I return.")}})).unwrap());
    }
    bag.loaded = true;
    let cursor = "1719800402.999999";
    shell.core.apply_thread_mark(&key, cursor);
    shell.thread_unread.held = Some(shell.thread_scope());
    shell.thread_unread.anchor = Some((shell.thread_scope(), cursor.into()));
    shell.refresh_from_core();
}

pub(crate) fn list_fixture(shell: &mut ShellState) {
    fixture(shell);
    let key = key(shell).unwrap();
    let messages = shell.core.threads[&key].messages.clone();
    let mut root = messages[0].clone();
    root.channel = Some(key.1.clone());
    shell
        .core
        .threads_view
        .ensure_scope(shell.core.account_epoch, &key.0);
    shell.core.threads_view.loaded = true;
    shell.core.threads_view.items = vec![super_platinum_core::slack::models::ThreadViewItem {
        root_msg: root,
        latest_replies: messages[1..].to_vec(),
        unread_replies: messages[3..].to_vec(),
        ..Default::default()
    }];
    shell.main_view = crate::state::MainView::Threads;
    shell.close_thread();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_hold_survives_focus_but_ends_when_thread_reopens() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture(&mut shell);
        assert!(held(&shell));
        shell.thread_at_bottom = true;
        assert!(!shell.reading_thread(true, &key(&shell).unwrap().1, &key(&shell).unwrap().2));
        assert_eq!(
            shell
                .thread_messages
                .iter()
                .filter(|m| m.show_unread_divider)
                .count(),
            1
        );
        assert!(shell.thread_messages[3].show_unread_divider);
        assert!(!shell.thread_messages[3].compact);
        shell.thread_generation += 1;
        assert!(!held(&shell));
        assert!(shell.reading_thread(true, &key(&shell).unwrap().1, &key(&shell).unwrap().2));
    }
    #[test]
    fn pending_and_divider_state_cannot_cross_accounts_or_threads() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture(&mut shell);
        shell
            .thread_unread
            .pending
            .insert((shell.core.account_epoch, key(&shell).unwrap()));
        assert!(busy(&shell));
        shell.thread_root = Some("other".into());
        assert!(!blocks_auto_read(&shell));
        shell.core.account_epoch += 1;
        assert!(!blocks_auto_read(&shell));
    }
    #[test]
    fn another_client_reading_the_thread_clears_the_local_hold() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture(&mut shell);
        let key = key(&shell).unwrap();
        shell.core.apply_thread_mark(&key, "1719800404.000000");
        reconcile(&mut shell);
        assert!(!held(&shell));
        shell.refresh_from_core();
        assert!(!shell.thread_messages.iter().any(|m| m.show_unread_divider));
    }
}
