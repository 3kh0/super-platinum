//! Native Activity loading. Fetching or paging never marks notifications read.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use super_platinum_core::slack::api;

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
    let (epoch, unread_target) = {
        let shell = state.peek();
        (
            shell.core.account_epoch,
            shell
                .core
                .workspaces
                .get(&team)
                .and_then(|w| w.activity_unread_count)
                .unwrap_or(0),
        )
    };
    // Preserve the initial unread-fill behavior, then let the user page older
    // notifications on demand instead of silently stopping at the first batch.
    let mut page_index = 0;
    loop {
        let request = {
            let mut shell = state.write();
            if !current(&shell, epoch, &team, &transport) {
                return;
            }
            shell.core.activity.ensure_scope(epoch, &team);
            let Some(request) = shell.core.activity.begin(append || page_index > 0) else {
                return;
            };
            request
        };
        let result = api::fetch_activity_feed(
            &transport,
            &client,
            &session,
            20,
            request.cursor.clone(),
            false,
        )
        .await;
        let continue_loading = {
            let mut shell = state.write();
            if !current(&shell, epoch, &team, &transport) || !shell.core.activity.accepts(&request)
            {
                // A replaced transport can keep the same account/workspace.
                // Release only this request so the next refresh can recover.
                shell.core.activity.fail(&request);
                return;
            }
            match result {
                Ok(page) => {
                    shell.core.activity.apply_page(&request, page);
                    page_index += 1;
                    !append
                        && page_index < 5
                        && shell.main_view == MainView::Activity
                        && shell.core.activity.next_cursor.is_some()
                        && shell.core.activity.unread_count() < unread_target
                }
                Err(error) => {
                    shell.core.activity.fail(&request);
                    shell.report_failure(&error, "Activity needs a connection", || {
                        "Could not load activity. Please try again.".into()
                    });
                    false
                }
            }
        };
        if !continue_loading {
            break;
        }
    }
    crate::bootstrap::hydrate_current_surface(state).await;
}

fn current(
    shell: &ShellState,
    epoch: u64,
    team: &str,
    transport: &std::sync::Arc<super_platinum_core::slack::Transport>,
) -> bool {
    shell.core.account_epoch == epoch
        && shell.core.active_team.as_deref() == Some(team)
        && shell
            .core
            .transport
            .as_ref()
            .is_some_and(|active| std::sync::Arc::ptr_eq(active, transport))
}

pub(crate) fn fixture(shell: &mut ShellState, name: &str) {
    shell.main_view = MainView::Activity;
    let feed = &mut shell.core.activity;
    feed.loaded = true;
    feed.next_cursor = Some("older-notifications".into());
    match name {
        "activity-loading" => {
            feed.items.clear();
            feed.loaded = false;
            feed.loading = true;
            feed.next_cursor = None;
        }
        "activity-error" => {
            feed.items.clear();
            feed.loaded = false;
            feed.failed = true;
            feed.next_cursor = None;
        }
        "activity-more-error" => feed.failed = true,
        "activity-empty" => {
            feed.items.clear();
            feed.next_cursor = None;
        }
        _ => {}
    }
}
