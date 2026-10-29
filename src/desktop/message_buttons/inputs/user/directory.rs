//! HvtU's member search defaults are distinct from the ordinary List User field.
use super::*;
use crate::scheduled::Context;
use serde_json::Value;
use super_platinum_core::{
    slack::{Error, models::User},
    state::Workspace,
};
mod discovery;
pub(crate) mod read_scope;
pub(crate) use discovery::aggregate;
pub(crate) use discovery::prepare_conversation_page;
pub(crate) use discovery::{
    conversation_mask_matches, decode_channel_rows, decode_channels, decode_user_rows,
    decode_users, prepare_channels, prepare_channels_page, prepare_public_channels_page,
    prepare_users, prepare_users_page, public_channel_filter_matches,
};
pub(crate) mod fan_in;
pub(crate) use discovery::{mpim as decode_mpim, normalize_mpim_query};
mod continued;
mod policy;
use discovery::fetch;
pub(crate) use policy::eligible;
const LIMIT: usize = 24;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub id: String,
    pub label: String,
    pub canonical: Option<Value>,
}
pub(crate) fn candidates<'a>(
    workspace: &Workspace,
    users: impl Iterator<Item = (&'a User, bool)>,
    query: &str,
) -> Vec<Choice> {
    let query = query.trim().trim_start_matches('@').to_lowercase();
    let mut ranked: Vec<(bool, String, Choice)> = Vec::with_capacity(LIMIT + 1);
    for (user, remote) in users {
        if !eligible(workspace, user)
            || workspace
                .users
                .get(&user.id)
                .is_some_and(|u| !eligible(workspace, u))
        {
            continue;
        }
        let label = super_platinum_core::state::display_name(Some(user), &user.id);
        let folded = label.to_lowercase();
        let name = user.name.as_deref().unwrap_or_default().to_lowercase();
        let real = user.real_name.as_deref().unwrap_or_default().to_lowercase();
        if !remote
            && !query.is_empty()
            && ![&folded, &name, &real].iter().any(|s| s.contains(&query))
        {
            continue;
        }
        let candidate = (
            !query.is_empty() && folded != query && name != query,
            folded,
            Choice {
                id: user.id.clone(),
                label,
                canonical: None,
            },
        );
        if ranked.iter().any(|(_, _, c)| c.id == user.id) {
            continue;
        }
        let index = ranked.partition_point(|old| {
            (&old.0, &old.1, &old.2.id) <= (&candidate.0, &candidate.1, &candidate.2.id)
        });
        if index < LIMIT {
            ranked.insert(index, candidate);
            ranked.truncate(LIMIT);
        }
    }
    ranked.into_iter().map(|(_, _, c)| c).collect()
}
pub(super) fn cached(shell: &ShellState, source: &Source, query: &str) -> Vec<Choice> {
    shell
        .core
        .workspaces
        .get(&source.owner().team_id)
        .map_or_else(Vec::new, |w| {
            candidates(w, w.users.values().map(|u| (u, false)), query)
        })
}
pub(crate) fn owner_current(shell: &ShellState, ctx: &Context) -> bool {
    shell.core.account_epoch == ctx.epoch
        && shell
            .core
            .transport
            .as_ref()
            .is_some_and(|transport| std::sync::Arc::ptr_eq(transport, &ctx.transport))
        && shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&ctx.session.team_id))
            .is_some_and(|s| s.user_id == ctx.session.user_id && s.token == ctx.session.token)
}
pub(crate) fn context_current(shell: &ShellState, ctx: &Context) -> bool {
    owner_current(shell, ctx) && shell.core.active_team.as_ref() == Some(&ctx.session.team_id)
}
/// A shared account/team deadline; callers cannot mutate the directory cache.
pub(crate) fn cooling(shell: &ShellState, epoch: u64, team: &str) -> bool {
    shell
        .message_buttons
        .user_retry
        .get(&(epoch, team.into()))
        .is_some_and(|deadline| std::time::Instant::now() < *deadline)
}
pub(super) fn admit_query(
    shell: &mut ShellState,
    ctx: &Context,
    revision: u64,
    query_revision: u64,
) -> bool {
    if !context_current(shell, ctx) || !request_current(shell, revision, query_revision) {
        return false;
    }
    if shell
        .message_buttons
        .user_retry
        .get(&(ctx.epoch, ctx.session.team_id.clone()))
        .is_some_and(|deadline| std::time::Instant::now() < *deadline)
    {
        let p = shell.message_buttons.user_picker.as_mut().unwrap();
        p.loading = false;
        p.error =
            Some("Slack asked us to wait before searching again. Cached choices are shown.".into());
        return false;
    }
    true
}
pub(super) fn request_current(shell: &ShellState, revision: u64, query_revision: u64) -> bool {
    shell.message_buttons.user_picker.as_ref().is_some_and(|p| {
        p.revision == revision && p.query_revision == query_revision && current(shell, p)
    })
}
fn explicit_conversation_query(picker: &super::Picker) -> bool {
    super::is_conversation(&picker.metadata)
        && picker
            .filter
            .as_ref()
            .is_some_and(conversation_mask_matches)
}
fn ordinary_channel_query(picker: &super::Picker) -> bool {
    super::is_channel(&picker.metadata)
        && picker
            .filter
            .as_ref()
            .is_some_and(public_channel_filter_matches)
}
pub(crate) fn search(mut state: Signal<ShellState>, revision: u64, query: String) {
    let request = {
        let mut shell = state.write();
        let Some(p) = shell.message_buttons.user_picker.as_ref() else {
            return;
        };
        if p.revision != revision
            || !current(&shell, p)
            || (p.query == query && (p.loading || p.error.is_none()))
        {
            return;
        }
        let ordinary_channels = ordinary_channel_query(p);
        let explicit_conversation = explicit_conversation_query(p);
        let source = p.source.clone();
        let filter = p.filter.clone();
        let choices = if let Some(filter) = &filter {
            super::conversations::cached(&shell, &source, filter, &query)
        } else {
            cached(&shell, &source, &query)
        };
        let context = Context::capture(&shell);
        shell
            .message_buttons
            .user_retry
            .retain(|(epoch, _), _| *epoch == source.owner().account_epoch);
        let cooling = shell
            .message_buttons
            .user_retry
            .get(&(source.owner().account_epoch, source.owner().team_id.clone()))
            .is_some_and(|deadline| std::time::Instant::now() < *deadline);
        let p = shell.message_buttons.user_picker.as_mut().unwrap();
        p.query = query.clone();
        p.query_revision = next_id();
        p.candidates = choices;
        p.error = None;
        p.loading = !query.trim().is_empty() && context.is_some() && !cooling;
        if cooling {
            p.error = Some(
                "Slack asked us to wait before searching again. Cached choices are shown.".into(),
            );
        }
        if !p.loading {
            return;
        }
        (
            context.unwrap(),
            p.query_revision,
            filter,
            ordinary_channels,
            explicit_conversation,
        )
    };
    spawn(async move {
        let (context, query_revision, filter, ordinary_channels, explicit_conversation) = request;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if !admit_query(&mut state.write(), &context, revision, query_revision) {
            return;
        }
        if filter.is_none() {
            let context = std::sync::Arc::new(context);
            let reader = context.clone();
            continued::run_with(
                state,
                context,
                revision,
                query_revision,
                query,
                move |request, scope| {
                    let context = reader.clone();
                    async move { scope.transport(&context, request).await }
                },
            )
            .await;
            return;
        }
        let posting = crate::bootstrap::posting_ingress::capture(
            &state,
            &context.transport,
            &context.session,
        );
        if ordinary_channels {
            let Some(filter) = filter else { return };
            let context = std::sync::Arc::new(context);
            let reader = context.clone();
            continued::run_channels_with(
                state,
                context,
                revision,
                query_revision,
                query,
                filter,
                move |request, scope| {
                    let context = reader.clone();
                    async move { scope.transport(&context, request).await }
                },
                move |channels| posting.channels(&state, channels),
            )
            .await;
            return;
        }
        if explicit_conversation {
            let Some(filter) = filter else {
                return;
            };
            let context = std::sync::Arc::new(context);
            let reader = context.clone();
            continued::run_conversations_with(
                state,
                context,
                revision,
                query_revision,
                query,
                filter,
                move |_, request, scope, _| {
                    let context = reader.clone();
                    async move { scope.transport(&context, request).await }
                },
                move |channels| posting.channels(&state, channels),
            )
            .await;
            return;
        }
        let result = fetch(&context, &query, filter.as_ref()).await;
        let accepted = {
            let shell = state.peek();
            owner_current(&shell, &context) && request_current(&shell, revision, query_revision)
        };
        if accepted && let Ok((_, channels)) = &result {
            posting.channels(&state, channels);
        }
        finish_read(
            state,
            &context,
            revision,
            query_revision,
            &query,
            result,
            true,
        );
    });
}

fn finish_read(
    mut state: Signal<ShellState>,
    context: &Context,
    revision: u64,
    query_revision: u64,
    query: &str,
    result: Result<(Vec<User>, Vec<super_platinum_core::slack::models::Channel>), Error>,
    observe_rate: bool,
) {
    let mut shell = state.write();
    if !owner_current(&shell, &context) {
        return;
    }
    if observe_rate
        && let Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
            ..
        }) = &result
    {
        let epoch = shell.core.account_epoch;
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_secs((*seconds).max(1)))
            .unwrap_or_else(|| std::time::Instant::now() + std::time::Duration::from_secs(86400));
        remember_cooldown(&mut shell, epoch, &context.session.team_id, deadline);
    }
    if !request_current(&shell, revision, query_revision) {
        return;
    }
    match result {
        Ok((users, channels)) => {
            let agent_unavailable = users
                .iter()
                .any(|u| u.extra.get("is_agentforce_bot") == Some(&Value::Bool(true)));
            let p = shell.message_buttons.user_picker.as_ref().unwrap();
            let workspace = &shell.core.workspaces[&p.source.owner().team_id];
            let choices = if let Some(filter) = &p.filter {
                super::conversations::candidates(
                    workspace,
                    filter,
                    users
                        .iter()
                        .map(|u| (u, true))
                        .chain(workspace.users.values().map(|u| (u, false))),
                    channels
                        .iter()
                        .map(|c| (c, true))
                        .chain(workspace.channels.values().map(|c| (c, false))),
                    &query,
                )
            } else {
                candidates(
                    workspace,
                    users
                        .iter()
                        .map(|u| (u, true))
                        .chain(workspace.users.values().map(|u| (u, false))),
                    &query,
                )
            };
            let owner = p.source.owner().clone();
            super::super::channels::refresh_retained(&mut shell, &owner, &channels);
            super::super::channels::refresh_retained_users(&mut shell, &owner, &users);
            let p = shell.message_buttons.user_picker.as_mut().unwrap();
            if let Some(selection) = p.multiple.as_mut() {
                super::super::channels::refresh_users(
                    &mut selection.proofs,
                    &selection.selected,
                    &users,
                );
                super::super::channels::refresh(
                    &mut selection.proofs,
                    &selection.selected,
                    &channels,
                );
            }
            p.candidates = choices;
            p.agent_unavailable |=
                p.filter.as_ref().is_none_or(|f| f.allows("im")) && agent_unavailable;
            p.loading = false;
        }
        Err(error) => {
            let delay = match error {
                Error::RateLimited {
                    retry_after_secs: Some(s),
                    ..
                } => s.max(1),
                _ => 5,
            };
            let epoch = shell.core.account_epoch;
            let deadline = std::time::Instant::now()
                .checked_add(std::time::Duration::from_secs(delay))
                .unwrap_or_else(|| {
                    std::time::Instant::now() + std::time::Duration::from_secs(86400)
                });
            remember_cooldown(&mut shell, epoch, &context.session.team_id, deadline);
            let p = shell.message_buttons.user_picker.as_mut().unwrap();
            p.loading = false;
            p.error = Some("Could not search Slack. Cached choices are shown.".into());
        }
    }
}

pub(crate) fn remember_cooldown(
    shell: &mut ShellState,
    epoch: u64,
    team: &str,
    deadline: std::time::Instant,
) {
    shell
        .message_buttons
        .user_retry
        .entry((epoch, team.into()))
        .and_modify(|current| *current = (*current).max(deadline))
        .or_insert(deadline);
}

/// Shared read-only member discovery. Callers retain their own surface tickets.
pub(crate) async fn fetch_users(ctx: &Context, query: &str) -> Result<Vec<User>, Error> {
    fetch(ctx, query, None).await.map(|(users, _)| users)
}

/// Shared conversation discovery; no message-owned picker or write is involved.
pub(crate) async fn fetch_conversations(
    ctx: &Context,
    query: &str,
    filter: &super::conversations::Filter,
) -> Result<(Vec<User>, Vec<super_platinum_core::slack::models::Channel>), Error> {
    fetch(ctx, query, Some(filter)).await
}
