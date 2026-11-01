//! Existing Activity hydration with original native ownership fences.
use super::*;

pub(super) async fn hydrate(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
) {
    hydrate_with(*state, transport, workspace_session, |ids| {
        api::fetch_messages_list(transport, client, workspace_session, ids)
    })
    .await;
}

pub(super) async fn hydrate_with<F, Fut>(
    mut state: Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    mut fetch: F,
) where
    F: FnMut(Vec<(String, Vec<String>)>) -> Fut,
    Fut: std::future::Future<
            Output = Result<
                super_platinum_core::slack::models::MessagesListPage,
                super_platinum_core::slack::Error,
            >,
        >,
{
    let Some(owner) =
        channel_metadata::Owner::capture(&state.peek().core, transport, workspace_session)
    else {
        return;
    };
    let (epoch, sequence) = {
        let shell = state.peek();
        (shell.core.account_epoch, shell.core.activity.load_seq)
    };
    let current = |shell: &ShellState| {
        owner.current(&shell.core)
            && shell.core.account_epoch == epoch
            && shell.core.active_team.as_deref() == Some(workspace_session.team_id.as_str())
            && shell
                .core
                .activity
                .matches_scope(epoch, &workspace_session.team_id)
            && shell.core.activity.load_seq == sequence
            && shell
                .core
                .transport
                .as_deref()
                .is_some_and(|active| std::ptr::eq(active, transport))
    };
    let message_ids = {
        let shell = state.read();
        if !current(&shell) || shell.main_view != crate::state::MainView::Activity {
            return;
        }
        let mut groups = std::collections::HashMap::<String, Vec<String>>::new();
        for item in &shell.core.activity.items {
            let Some(channel) = item.channel() else {
                continue;
            };
            for ts in item.request_ts() {
                if shell.core.activity_message_was_deleted(channel, &ts)
                    || shell
                        .core
                        .activity
                        .hydrated
                        .contains_key(&(channel.to_owned(), ts.clone()))
                {
                    continue;
                }
                let bucket = groups.entry(channel.to_owned()).or_default();
                if !bucket.contains(&ts) {
                    bucket.push(ts);
                }
            }
        }
        groups.into_iter().collect::<Vec<_>>()
    };
    if message_ids.is_empty() {
        return;
    }

    // Slack rejects messages.list requests spanning too many conversations.
    // Activity pages can easily cover dozens, so hydrate them in bounded batches.
    for chunk in message_ids.chunks(20) {
        if !current(&state.peek()) {
            return;
        }
        let Some((read, files)) = ({
            let mut shell = state.write();
            shell
                .core
                .activity
                .begin_message_hydration(chunk)
                .map(|read| {
                    (
                        read,
                        shell
                            .core
                            .begin_file_display_read(&workspace_session.team_id),
                    )
                })
        }) else {
            continue;
        };
        let page = match fetch(chunk.to_vec()).await {
            Ok(page) => page,
            Err(error) => {
                eprintln!("super-platinum: activity message hydration batch failed: {error}");
                continue;
            }
        };
        let mut shell = state.write();
        if !current(&shell) {
            return;
        }
        if let Err(reason) =
            shell
                .core
                .apply_activity_message_page(&workspace_session.team_id, read, page, files)
        {
            eprintln!("super-platinum: activity message hydration response rejected: {reason}");
        }
    }
}

#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
