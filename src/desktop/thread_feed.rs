//! Native loading for the workspace Threads feed; no read marks are sent here.
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::{domain::ThreadsState, slack::api};

pub fn visible_state(shell: &ShellState) -> Option<&ThreadsState> {
    shell
        .core
        .threads_view
        .matches_scope(shell.core.account_epoch, shell.core.active_team.as_deref()?)
        .then_some(&shell.core.threads_view)
}

pub async fn load(mut state: Signal<ShellState>, append: bool) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        return;
    };
    let Some(team) = state.peek().core.active_team.clone() else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|s| s.team_id == team) else {
        return;
    };
    let (epoch, request, cursors, counts_revision) = {
        let mut shell = state.write();
        let epoch = shell.core.account_epoch;
        shell.core.threads_view.ensure_scope(epoch, &team);
        let Some(request) = shell.core.threads_view.begin(append) else {
            return;
        };
        let cursors = shell
            .core
            .threads
            .iter()
            .filter(|(key, _)| key.0 == team)
            .map(|(key, bag)| (key.clone(), bag.read_revision))
            .collect();
        let counts_revision = shell.core.workspaces[&team].thread_counts.revision();
        (epoch, request, cursors, counts_revision)
    };
    let cursor_owner = crate::realtime::activity_counts::CursorOwner::capture(
        &state.peek().core,
        &transport,
        &session,
    );
    let result = api::fetch_threads_view(
        &transport,
        &client,
        &session,
        30,
        request.cursor.clone(),
        false,
    )
    .await;
    let applied = {
        let mut shell = state.write();
        if shell.core.account_epoch != epoch
            || shell.core.active_team.as_ref() != Some(&team)
            || !shell
                .core
                .transport
                .as_ref()
                .is_some_and(|active| std::sync::Arc::ptr_eq(active, &transport))
            || !shell.core.threads_view.matches_scope(epoch, &team)
            || shell.core.threads_view.load_seq != request.sequence
        {
            return;
        }
        if !cursor_owner
            .as_ref()
            .is_some_and(|owner| owner.current(&shell.core))
        {
            shell.core.threads_view.fail(&request);
            return;
        }
        match result {
            Ok(mut page) => {
                let counts = page.threads_state.take();
                let keys: Vec<_> = page
                    .threads
                    .iter()
                    .filter_map(|item| {
                        Some((
                            team.clone(),
                            item.channel()?.clone(),
                            item.root_ts()?.clone(),
                        ))
                    })
                    .collect();
                let applied = shell
                    .core
                    .apply_thread_feed_page(&team, &request, page, &cursors);
                if applied {
                    reconcile_cursors(&mut shell.core, &keys);
                }
                if applied && let Some(counts) = counts {
                    shell
                        .core
                        .workspaces
                        .get_mut(&team)
                        .unwrap()
                        .thread_counts
                        .apply_snapshot(counts, counts_revision);
                }
                applied
            }
            Err(error) => {
                shell.core.threads_view.fail(&request);
                shell.report_failure(&error, "Threads need a connection", || {
                    "Could not load threads. Please try again.".into()
                });
                false
            }
        }
    };
    if applied {
        crate::bootstrap::persist_workspace(&state, &team);
        crate::bootstrap::hydrate_current_surface(state).await;
    }
}

// Use the cursor retained by the revision-aware merge, never the stale page's
// last_read when a socket mark arrived while it was in flight.
fn reconcile_cursors(
    core: &mut super_platinum_core::CoreAppState,
    keys: &[super_platinum_core::domain::ThreadKey],
) {
    for key in keys {
        if let Some(cursor) = core.thread_cursor(key).map(str::to_owned) {
            crate::realtime::activity_counts::mark_thread_read(
                core, &key.0, &key.1, &key.2, &cursor,
            );
        }
    }
}

pub(crate) fn fixture(shell: &mut ShellState, name: &str) {
    crate::thread_unread::list_fixture(shell);
    let epoch = shell.core.account_epoch;
    let team = shell.core.active_team.clone().unwrap();
    let items = std::mem::take(&mut shell.core.threads_view.items);
    let feed = &mut shell.core.threads_view;
    feed.ensure_scope(epoch, &team);
    feed.items = items;
    if let Some(item) = feed.items.first_mut() {
        item.root_msg.reply_count = Some(147);
    }
    feed.loaded = true;
    feed.has_more = true;
    feed.next_cursor = Some("1719800403.000000".into());
    match name {
        "thread-feed-empty" => {
            feed.items.clear();
            feed.has_more = false;
        }
        "thread-feed-error" => feed.failed = true,
        "thread-feed-loading" => feed.loading = true,
        "thread-feed-updated" => feed.dirty = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fetched_thread_cursor_cannot_override_an_intervening_mark_in_activity() {
        let mut core = super_platinum_core::CoreAppState::new(Default::default());
        core.active_team = Some("T".into());
        core.activity.ensure_scope(core.account_epoch, "T");
        core.threads_view.ensure_scope(core.account_epoch, "T");
        let key = ("T".into(), "C".into(), "1.0".into());
        for ts in ["2.0", "4.0"] {
            core.activity.upsert(
                serde_json::from_value(serde_json::json!({
                    "key":ts,"feed_ts":ts,"is_unread":true,
                    "item":{"type":"at_user","message":{"channel":"C","thread_ts":"1.0","ts":ts}}
                }))
                .unwrap(),
            );
        }
        let request = core.threads_view.begin(false).unwrap();
        core.apply_thread_mark(&key, "2.0");
        let page = serde_json::from_value(serde_json::json!({"threads":[{
            "root_msg":{"channel":"C","ts":"1.0","last_read":"4.0"},
            "latest_replies":[{"ts":"2.0"},{"ts":"4.0"}]
        }]}))
        .unwrap();
        assert!(core.apply_thread_feed_page("T", &request, page, &Default::default()));
        reconcile_cursors(&mut core, &[key]);
        assert_eq!(core.activity.unread_count(), 1);
        assert!(
            core.activity
                .items
                .iter()
                .find(|item| item.key == "4.0")
                .unwrap()
                .is_unread
        );
    }

    #[test]
    fn switching_workspace_releases_a_pending_feed_request() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture(&mut shell, "thread-feed");
        let old_team = shell.core.active_team.clone().unwrap();
        let request = shell.core.threads_view.begin(false).unwrap();
        let other = shell
            .workspaces
            .iter()
            .position(|w| w.id != old_team)
            .unwrap();
        let other_id = shell.workspaces[other].id.clone();
        let mut workspace = shell.core.workspaces[&old_team].clone();
        workspace.team_id = other_id.clone();
        shell.core.workspaces.insert(other_id, workspace);
        assert!(shell.select_workspace(other));
        assert!(!shell.core.threads_view.loading);
        assert!(shell.core.threads_view.items.is_empty());
        assert_ne!(shell.core.threads_view.load_seq, request.sequence);
    }
}
