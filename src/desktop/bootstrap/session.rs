use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api;
use super_platinum_core::slack::realtime::{self, ConnectParams};

use super::common::{credentials, persist_workspace, refresh_history};
use crate::state::ShellState;

mod channel_metadata;
pub(crate) use channel_metadata::Owner as ReadOwner;
mod surface_metadata;
use std::sync::Arc;
pub(crate) use surface_metadata::Hydration as SurfaceHydration;
use surface_metadata::{
    hydrate_emojis as hydrate_surface_emojis, hydrate_users as hydrate_surface_users,
};

pub async fn refresh(mut state: Signal<ShellState>) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, mut workspaces)) = credentials(&state) else {
        return;
    };

    let refresh_epoch = state.peek().core.account_epoch;
    let active_team = state.read().core.active_team.clone();
    workspaces.sort_by_key(|session| Some(&session.team_id) != active_team.as_ref());
    let mut later_owner = workspaces
        .iter()
        .find(|session| Some(&session.team_id) == active_team.as_ref())
        .and_then(|session| {
            channel_metadata::Owner::capture(&state.peek().core, &transport, session)
        })
        .map(Arc::new);
    for workspace_session in workspaces {
        let team = workspace_session.team_id.clone();
        let warm = state
            .peek()
            .core
            .workspaces
            .get(&team)
            .is_some_and(|w| !w.channels.is_empty());
        let counts_owner = (state.peek().core.account_epoch == refresh_epoch)
            .then(|| {
                crate::realtime::activity_counts::CountsOwner::capture(
                    &state.peek().core,
                    &transport,
                    &workspace_session,
                )
            })
            .flatten();
        let boot_read = {
            let mut shell = state.write();
            if shell.core.account_epoch != refresh_epoch {
                return;
            }
            super_platinum_core::state::posting_metadata::authority::BootRead::begin_full(
                &mut shell.core,
                &transport,
                &workspace_session,
            )
        };
        let Ok(boot_read) = boot_read else {
            continue;
        };
        let Ok(request) = boot_read.request(&state.peek().core) else {
            continue;
        };
        match transport.execute(request).await {
            Ok(raw) => {
                let mut shell = state.write();
                let boot_mode =
                    crate::directory_member_fetches::automatic::mode::Boot::capture(&raw);
                match boot_read.apply(&mut shell.core, raw, true) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        shell.loading = false;
                        shell.report_failure(&error, "", || {
                            format!("Workspace refresh failed: {error}")
                        });
                        continue;
                    }
                }
                let _ = boot_mode.accepted(&mut shell, &team);
                shell.core.screen = super_platinum_core::state::Screen::Main;
                shell.loading = false;
                shell.refresh_from_core();
            }
            Err(error) => {
                let mut shell = state.write();
                if !boot_read.failed(&mut shell.core) {
                    continue;
                }
                shell.loading = false;
                shell.report_failure(&error, "", || format!("Workspace refresh failed: {error}"));
                continue;
            }
        }

        if state.peek().core.account_epoch != refresh_epoch {
            return;
        }
        let Some(owner) =
            channel_metadata::Owner::capture(&state.peek().core, &transport, &workspace_session)
                .map(Arc::new)
        else {
            return;
        };

        if active_team.as_ref() == Some(&team) {
            later_owner = Some(owner.clone());
        }
        // Accepted boot may already have a current socket on a warm refresh.
        // Cold boot is deferred by the scheduler until Connected is accepted.
        crate::app_home::autocomplete::startup::schedule(state, &team);
        crate::app_home::channel_fetches::request_initialization(&mut state.write(), &team, warm);
        if let Some(owner) = &counts_owner {
            owner.refresh_badge_rules(state);
        }
        let params = ConnectParams {
            team: team.clone(),
            ws_url: realtime::flannel_url(&workspace_session.token, &team),
            d_cookie: state
                .read()
                .core
                .session
                .as_ref()
                .map(|session| session.d_cookie.clone())
                .unwrap_or_default(),
            user_agent: super_platinum_core::slack::xparams::Identity::from_capture().user_agent,
        };
        dioxus::prelude::spawn(crate::realtime::worker(state, params));

        let active_channel = {
            let shell = state.read();
            (shell.core.active_team.as_ref() == Some(&team))
                .then(|| shell.core.active_channel.clone())
                .flatten()
        };
        if let Some(channel) = active_channel {
            refresh_history(
                &mut state,
                &transport,
                &client,
                &workspace_session,
                &team,
                channel,
            )
            .await;
            hydrate_surface_users(
                &mut state,
                &transport,
                &client,
                &workspace_session,
                &team,
                owner.clone(),
            )
            .await;
            if !owner.current(&state.peek().core) {
                return;
            }
            spawn_owned_media(state, transport.clone(), owner.clone());
        }

        if !owner.current(&state.peek().core) {
            return;
        }
        crate::usergroups::hydrate(state, &team).await;
        if !owner.current(&state.peek().core) {
            return;
        }
        // Capture revisions immediately before the HTTP request, while retaining
        // the owner from before boot/history awaits. New socket/local read state wins.
        if let Some(read) = counts_owner.and_then(|owner| owner.begin(&state.peek().core)) {
            if let Ok(counts) = api::fetch_counts(&transport, &client, &workspace_session).await {
                let mut shell = state.write();
                if read.apply(&mut shell.core, counts) {
                    shell.refresh_dock_badge_for(&team);
                }
            }
        }
        if !owner.current(&state.peek().core) {
            return;
        }
        refresh_channel_metadata(&mut state, &transport, &client, &workspace_session, &team).await;
        if !owner.current(&state.peek().core) {
            return;
        }
        let dnd = api::fetch_dnd_info(&transport, &client, &workspace_session).await;
        if !owner.current(&state.peek().core) {
            return;
        }
        if let Ok(dnd) = dnd
            && let Some(workspace) = state.write().core.workspaces.get_mut(&team)
        {
            workspace.self_dnd = dnd;
        }
        let posting =
            crate::bootstrap::posting_ingress::capture(&state, &transport, &workspace_session);
        if let Ok(dms) = api::fetch_sidebar_dms(&transport, &client, &workspace_session).await {
            if !owner.current(&state.peek().core) {
                return;
            }
            posting.channels(&state, &dms.all_channels());
            if let Some(workspace) = state.write().core.workspaces.get_mut(&team) {
                workspace.apply_sidebar_dms(dms);
            }
        }
        if !owner.current(&state.peek().core) {
            return;
        }
        let sections = api::fetch_channel_sections(&transport, &client, &workspace_session).await;
        if !owner.current(&state.peek().core) {
            return;
        }
        if let Ok(sections) = sections
            && let Some(workspace) = state.write().core.workspaces.get_mut(&team)
        {
            workspace.apply_channel_sections(sections);
        }
        state.write().refresh_from_core();
        hydrate_surface_users(
            &mut state,
            &transport,
            &client,
            &workspace_session,
            &team,
            owner.clone(),
        )
        .await;
        if !owner.current(&state.peek().core) {
            return;
        }
        spawn_owned_media(state, transport.clone(), owner.clone());

        persist_workspace(&state, &team);
    }
    // Metadata only unless Later is visible; active history takes priority.
    if let Some(owner) = later_owner {
        dioxus::prelude::spawn(async move {
            if owner.current(&state.peek().core) {
                crate::later::load(state, Default::default(), false).await;
            }
        });
    }
}

pub(crate) async fn hydrate_current_surface(mut state: Signal<ShellState>) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    let Some(owner) =
        channel_metadata::Owner::capture(&state.peek().core, &transport, &workspace_session)
            .map(Arc::new)
    else {
        return;
    };
    hydrate_surface_with(state, owner.clone(), |stage| {
        let owner = owner.clone();
        let transport = &transport;
        let client = &client;
        let workspace_session = &workspace_session;
        let team = &team;
        async move {
            match stage {
                SurfaceStage::Activity => {
                    hydrate_activity_messages(&mut state, transport, client, workspace_session)
                        .await
                }
                SurfaceStage::Emoji => {
                    hydrate_surface_emojis(
                        &mut state,
                        transport,
                        client,
                        workspace_session,
                        team,
                        owner,
                    )
                    .await
                }
                SurfaceStage::Channels => {
                    hydrate_surface_channels(&mut state, transport, client, workspace_session, team)
                        .await
                }
                SurfaceStage::Users => {
                    hydrate_surface_users(
                        &mut state,
                        transport,
                        client,
                        workspace_session,
                        team,
                        owner,
                    )
                    .await
                }
                SurfaceStage::Avatars => register_surface_avatars(&state, team),
                SurfaceStage::Media => refresh_media(state, transport.clone()).await,
            }
        }
    })
    .await;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurfaceStage {
    Activity,
    Emoji,
    Channels,
    Users,
    Avatars,
    Media,
}
async fn hydrate_surface_with<F, Fut>(
    state: Signal<ShellState>,
    owner: Arc<channel_metadata::Owner>,
    mut stage: F,
) where
    F: FnMut(SurfaceStage) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for next in [
        SurfaceStage::Activity,
        SurfaceStage::Emoji,
        SurfaceStage::Channels,
        SurfaceStage::Users,
        SurfaceStage::Avatars,
        SurfaceStage::Media,
    ] {
        if !owner.current(&state.peek().core) {
            return;
        }
        stage(next).await;
    }
}

/// Every Slack message currently projected onto a visible surface. Emoji, user,
/// and channel hydration all need the same set.
fn visit_surface_messages(
    shell: &ShellState,
    team: &str,
    visit: &mut impl FnMut(&super_platinum_core::slack::models::Message),
) {
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return;
    };
    if let Some(messages) = shell
        .core
        .active_channel
        .as_ref()
        .and_then(|channel| workspace.messages.get(channel))
    {
        for message in &messages.messages {
            visit(message);
        }
    }
    if let Some(messages) = shell.thread_root.as_ref().and_then(|root| {
        shell.core.active_channel.as_ref().and_then(|channel| {
            shell
                .core
                .threads
                .get(&(team.to_owned(), channel.clone(), root.clone()))
        })
    }) {
        for message in &messages.messages {
            visit(message);
        }
    }
    for message in shell.core.activity.hydrated.values() {
        visit(message);
    }
    for item in &shell.core.threads_view.items {
        for message in std::iter::once(&item.root_msg)
            .chain(&item.unread_replies)
            .chain(&item.latest_replies)
        {
            visit(message);
        }
    }
}

/// Unfurl footers and `#channel` chips print a raw id until the conversation is
/// known, so pull metadata for the ones the visible messages point at.
/// Bring cached conversation metadata up to date, the way Slack's client does:
/// one conditional `conversations.genericInfo` per batch, which only returns
/// the conversations whose `updated` stamp moved (renamed, archived, gone
/// dormant). The sidebar's filters read exactly those fields.
async fn refresh_channel_metadata(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    team: &str,
) {
    let Some(owner) =
        channel_metadata::Owner::capture(&state.peek().core, transport, workspace_session)
    else {
        return;
    };
    let batches = state
        .read()
        .core
        .workspaces
        .get(team)
        .map(|workspace| workspace.channels_to_refresh())
        .unwrap_or_default();
    for batch in batches {
        let Some(read) = owner.begin(&state.peek().core, batch.keys()) else {
            return;
        };
        if !owner.current(&state.peek().core) {
            return;
        }
        let posting =
            crate::bootstrap::posting_ingress::capture(state, transport, workspace_session)
                .for_channels(batch.keys());
        match api::fetch_generic_info(transport, client, workspace_session, &batch).await {
            Ok(page) => {
                if !read.accepts(&state.peek().core, &page.channels) {
                    return;
                }
                posting.channels(state, &page.channels);
                read.refresh(&mut state.write().core, page);
            }
            Err(error) => {
                if owner.current(&state.peek().core) {
                    eprintln!("super-platinum: conversations.genericInfo failed: {error}");
                }
                break;
            }
        }
    }
}

async fn hydrate_surface_channels(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    team: &str,
) {
    let Some(owner) =
        channel_metadata::Owner::capture(&state.peek().core, transport, workspace_session)
    else {
        return;
    };
    let channels = {
        let shell = state.read();
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return;
        };
        let mut ids = std::collections::HashSet::new();
        visit_surface_messages(&shell, team, &mut |message| {
            super_platinum_core::state::collect_message_channel_ids(message, &mut ids);
        });
        ids.retain(|id| {
            !workspace
                .channels
                .get(id)
                .is_some_and(|channel| channel.name.is_some())
                && !shell
                    .core
                    .channel_hydrated
                    .contains(&(team.to_owned(), id.clone()))
        });
        ids.into_iter().collect::<Vec<_>>()
    };
    if channels.is_empty() {
        return;
    }
    for chunk in channels.chunks(50) {
        // Another surface may have claimed a later batch while this worker
        // awaited an earlier one. Reserve only IDs still unclaimed now.
        let chunk: Vec<_> = {
            let shell = state.peek();
            chunk
                .iter()
                .filter(|id| {
                    !shell
                        .core
                        .channel_hydrated
                        .contains(&(team.to_owned(), (*id).clone()))
                })
                .cloned()
                .collect()
        };
        if chunk.is_empty() {
            continue;
        }
        let Some(read) = owner.begin(&state.peek().core, &chunk) else {
            return;
        };
        state.write().core.channel_hydrated.extend(
            chunk
                .iter()
                .map(|channel| (team.to_owned(), channel.clone())),
        );
        if !owner.current(&state.peek().core) {
            return;
        }
        let posting =
            crate::bootstrap::posting_ingress::capture(state, transport, workspace_session)
                .for_channels(&chunk);
        match api::fetch_channels_info(transport, client, workspace_session, chunk.to_vec()).await {
            Ok(channels) => {
                if !read.accepts(&state.peek().core, &channels) {
                    read.retry_hydration(&mut state.write().core);
                    return;
                }
                posting.channels(state, &channels);
                let mut shell = state.write();
                // Missing rows are not successful hydration. Permit a later surface
                // visit to retry, including a response containing no channel rows.
                if read.hydrate(&mut shell.core, channels) {
                    shell.refresh_from_core();
                }
            }
            Err(error) => {
                if owner.current(&state.peek().core) {
                    read.retry_hydration(&mut state.write().core);
                    eprintln!("super-platinum: unfurl channel hydration failed: {error}");
                }
            }
        }
    }
}

fn register_surface_avatars(state: &Signal<ShellState>, team: &str) {
    let shell = state.read();
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return;
    };
    for item in &shell.core.activity.items {
        let user = item.author().or_else(|| {
            let channel = item.channel()?;
            let message = item
                .preview_ts()
                .and_then(|ts| {
                    shell
                        .core
                        .activity
                        .hydrated
                        .get(&(channel.to_owned(), ts.to_owned()))
                })
                .or_else(|| {
                    item.ts().and_then(|ts| {
                        shell
                            .core
                            .activity
                            .hydrated
                            .get(&(channel.to_owned(), ts.to_owned()))
                    })
                })?;
            message.user.as_deref()
        });
        let Some((user, url)) = user.and_then(|user| Some((user, workspace.avatar_url(user)?)))
        else {
            continue;
        };
        shell.media.register_avatar(user, &url);
    }
    for item in &shell.core.threads_view.items {
        for message in std::iter::once(&item.root_msg)
            .chain(&item.unread_replies)
            .chain(&item.latest_replies)
        {
            let Some((user, url)) = message
                .user
                .as_deref()
                .and_then(|user| Some((user, workspace.avatar_url(user)?)))
            else {
                continue;
            };
            shell.media.register_avatar(user, &url);
        }
    }
}

async fn hydrate_activity_messages(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
) {
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
                if shell
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
        let page =
            match api::fetch_messages_list(transport, client, workspace_session, chunk.to_vec())
                .await
            {
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
        for (channel, data) in page.messages_data {
            for message in data.messages {
                if let Some(ts) = message.ts.clone() {
                    shell
                        .core
                        .activity
                        .hydrated
                        .insert((channel.clone(), ts), message);
                }
            }
        }
    }
}

pub async fn sign_in(mut state: Signal<ShellState>) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::SignIn,
    ) {
        return;
    }
    let media = state.read().media.clone();
    state.write().loading = true;
    let Ok(executable) = std::env::current_exe() else {
        state
            .write()
            .show_toast("Could not locate the Super Platinum executable");
        return;
    };
    let status = tokio::process::Command::new(executable)
        .env("SUPER_PLATINUM_AUTH", "1")
        .env("SUPER_PLATINUM_AUTH_ADD", "1")
        .status()
        .await;
    match status {
        Ok(status) if status.success() => {
            let loaded = super::startup::load_environment(media).await;
            install_loaded(state, loaded);
            super::startup::restore_transcripts(state).await;
            crate::messaging::upload_journal::load(state);
            refresh(state).await;
        }
        Ok(_) => {
            let mut shell = state.write();
            shell.loading = false;
            shell.show_toast("Slack sign-in was not completed");
        }
        Err(error) => {
            let mut shell = state.write();
            shell.loading = false;
            shell.show_toast(format!("Could not start Slack sign-in: {error}"));
        }
    }
}

pub async fn switch_account(mut state: Signal<ShellState>, account_id: String) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::SwitchAccount(account_id.clone()),
    ) {
        return;
    }
    let result = tokio::task::spawn_blocking(move || {
        super_platinum_core::config::set_active_account(&account_id)
    })
    .await;
    match result {
        Ok(Ok(())) => reload_account(state).await,
        Ok(Err(error)) => state
            .write()
            .show_toast(format!("Account switch failed: {error}")),
        Err(error) => state
            .write()
            .show_toast(format!("Account switch stopped: {error}")),
    }
}

pub async fn remove_account(mut state: Signal<ShellState>, account_id: String) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::RemoveAccount(account_id.clone()),
    ) {
        return;
    }
    let result = tokio::task::spawn_blocking(move || {
        super_platinum_core::config::remove_account(&account_id)
    })
    .await;
    match result {
        Ok(Ok(_)) => reload_account(state).await,
        Ok(Err(error)) => state
            .write()
            .show_toast(format!("Account removal failed: {error}")),
        Err(error) => state
            .write()
            .show_toast(format!("Account removal stopped: {error}")),
    }
}

async fn reload_account(mut state: Signal<ShellState>) {
    let media = state.read().media.clone();
    let loaded = super::startup::load_environment(media).await;
    install_loaded(state, loaded);
    super::startup::restore_transcripts(state).await;
    crate::messaging::upload_journal::load(state);
    refresh(state).await;
}

fn spawn_owned_media(
    state: Signal<ShellState>,
    transport: Arc<super_platinum_core::slack::Transport>,
    owner: Arc<channel_metadata::Owner>,
) {
    let work = refresh_media(state, transport);
    dioxus::prelude::spawn(async move {
        if owner.current(&state.peek().core) {
            work.await;
        }
    });
}
pub(super) fn refresh_media(
    state: Signal<ShellState>,
    transport: Arc<super_platinum_core::slack::Transport>,
) -> impl std::future::Future<Output = ()> {
    crate::media::load(state, transport)
}

pub(crate) fn install_loaded(mut state: Signal<ShellState>, mut loaded: ShellState) {
    let mut previous = state.write();
    // A newly loaded account/configuration must acquire posting authority from
    // its own actual boot request. Warm caches and carried UI receipts cannot
    // transfer a prior account's boot/config binding.
    for workspace in loaded.core.workspaces.values_mut() {
        workspace.posting_metadata.invalidate();
    }
    let _release = crate::performance::emoji_retirement::scope(14, 1, 0);
    crate::read_preferences::settings::carry_pending(&mut previous, &mut loaded);
    crate::canvas_create::carry_pending(&mut previous, &mut loaded);
    crate::canvas::comments::carry_pending(&mut previous, &mut loaded);
    crate::slack_lists::comments::carry_pending(&mut previous, &mut loaded);
    crate::channel_tabs::carry(&mut previous, &mut loaded);
    crate::slack_lists::completion::carry_pending(&mut previous, &mut loaded);
    crate::slack_lists::reminder::carry_pending(&mut previous, &mut loaded);
    crate::file_share::carry_pending(&mut previous, &mut loaded);
    crate::profile_edit::recording::carry_pending(&mut previous, &mut loaded);
    crate::app_home::workflows::navigation_history::carry(&mut previous, &mut loaded);
    crate::app_home::about::management::carry_pending(&mut previous, &mut loaded);
    crate::app_home::channel_fetches::carry(&mut previous, &mut loaded);
    crate::directory_member_fetches::carry_into_fresh_shell(&mut previous, &mut loaded);
    crate::bootstrap::search_file_pages::saved::registry::carry(&mut previous, &mut loaded);
    previous.media.retire();
    *previous = loaded;
}
