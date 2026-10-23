use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::events::RtEvent;
use super_platinum_core::slack::realtime::{self, ConnectParams, RtUpdate};

use crate::state::ShellState;

pub(crate) mod activity_counts;
mod presence;
mod reactions;

pub async fn worker(mut state: Signal<ShellState>, params: ConnectParams) {
    // The supervisor watches the same link verdict the shell paints on the
    // rail, so a dead socket is dropped in seconds and the reconnect waits for
    // the network to be confirmed back instead of dialling into nothing.
    let Some((epoch, transport)) = worker_source(&state.peek().core, &params) else {
        return;
    };
    let captured = {
        let mut shell = state.write();
        crate::drafts::protected::production::socket::Worker::capture(&mut shell, params)
    };
    let (mut owned_socket, params) = match captured {
        Ok(value) => value,
        Err(_) => {
            state.write().managed_drafts.error =
                Some("Owned draft storage is not ready for this account. Input is retained.");
            return;
        }
    };
    let health = Some(transport.health_watch());
    let mut updates = realtime::connect(params, health);
    let mut notifications = crate::notification::NotificationTracker::default();
    #[cfg(target_os = "macos")]
    let attention_state = state;
    while let Some((team, update)) = updates.recv().await {
        let mut shell = state.write();
        // A replaced account may reuse the same team and socket generation.
        // Events belong to the transport that opened this worker, never the
        // credentials that happen to be current when a queued frame arrives.
        if !owned_socket.current(&shell)
            || shell.core.account_epoch != epoch
            || shell
                .core
                .transport
                .as_ref()
                .is_none_or(|current| !std::sync::Arc::ptr_eq(current, &transport))
        {
            return;
        }
        match update {
            RtUpdate::Connected {
                generation,
                connection,
            } => {
                if !owned_socket.can_accept(&shell, &team, generation) {
                    continue;
                }
                crate::file_share::connection(&mut shell, &team, generation);
                if shell.core.active_team.as_ref() == Some(&team) {
                    crate::performance::record(crate::performance::StartupStage::RealtimeConnected);
                }
                if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                    if generation > 1 {
                        workspace.posting_metadata.invalidate();
                        workspace.draft_presence.invalidate();
                    }
                    workspace.rt_generation = generation;
                    workspace.reset_channel_discovery();
                    let self_user = workspace.self_user_id.clone();
                    connection.send(super_platinum_core::slack::realtime::presence_sub_frame(
                        std::slice::from_ref(&self_user),
                    ));
                    workspace.rt =
                        super_platinum_core::state::RealtimeStatus::Connected(connection);
                }
                let accepted_socket = match owned_socket.accept(&mut shell, &team, generation) {
                    Ok(source) => source,
                    Err(_) => return,
                };
                let channel_work =
                    crate::app_home::channel_fetches::connected(&mut shell, &team, generation);
                crate::directory_member_fetches::automatic::connected(
                    &mut shell, &team, generation,
                );
                // Nothing replays the frames that arrived while the socket was
                // down, so whatever happened to the open conversation meanwhile
                // — a reaction above all, which no later frame mentions again —
                // would stay missing until it was left and reopened. Refetching
                // what is on screen is how the reader stops being the last to
                // know. The first connect needs none of this: the conversation
                // it opens fetches itself.
                let reconnected = generation > 1;
                let refresh_later = reconnected && shell.core.active_team.as_ref() == Some(&team);
                if refresh_later {
                    crate::slack_lists::invalidate_visible_for_reconnect(&mut shell);
                }
                let refresh_threads =
                    refresh_later && shell.main_view == crate::state::MainView::Threads;
                let refresh_drafts = refresh_later
                    && (shell.core.scheduled.loaded
                        || shell.main_view == crate::state::MainView::Scheduled);
                let later_filter = shell.core.later.filter;
                let thread = shell
                    .thread_root
                    .clone()
                    .zip(shell.core.active_channel.clone());
                drop(shell);
                crate::app_home::autocomplete::startup::schedule(state, &team);
                crate::app_home::channel_fetches::resume(state, channel_work);
                crate::directory_member_fetches::automatic::resume(state, &team);
                if owned_socket
                    .connected(state, accepted_socket, generation)
                    .await
                    .is_err()
                {
                    state.write().managed_drafts.error = Some(
                        "Draft continuity is unavailable; input and storage work are retained.",
                    );
                }
                if reconnected {
                    dioxus::prelude::spawn(crate::channel_discovery::reconcile(
                        state,
                        team.clone(),
                        generation,
                    ));
                    if refresh_later {
                        crate::slack_lists::refresh(state);
                        dioxus::prelude::spawn(refresh_open_metadata(state, generation));
                        dioxus::prelude::spawn(crate::later::load(state, later_filter, false));
                    }
                    if refresh_threads {
                        dioxus::prelude::spawn(crate::thread_feed::load(state, false));
                    }
                    if refresh_drafts {
                        dioxus::prelude::spawn(crate::scheduled::load(state, false));
                    }
                    dioxus::prelude::spawn(crate::bootstrap::refresh_selected_channel(state));
                    dioxus::prelude::spawn(crate::channel_resources::refresh(state));
                    if let Some((root, channel)) = thread {
                        dioxus::prelude::spawn(crate::bootstrap::open_thread(state, channel, root));
                    }
                }
            }
            RtUpdate::Event { generation, event } => {
                let owned_draft = match event.as_ref() {
                    RtEvent::Unknown(raw)
                        if matches!(
                            raw.kind.as_str(),
                            "draft_create" | "draft_update" | "draft_delete"
                        ) =>
                    {
                        Some(raw.clone())
                    }
                    _ => None,
                };
                crate::file_share::event(&mut shell, &team, generation, &event);
                let presence_only = matches!(event.as_ref(), RtEvent::PresenceChange { .. });
                let reaction_channel = reactions::channel(&event).map(str::to_owned);
                let list_refresh =
                    crate::slack_lists::events::prepare(&mut shell, &team, generation, &event);
                let file_read = crate::file_events::prepare(&mut shell, &team, generation, &event);
                let file_change =
                    matches!(event.as_ref(), RtEvent::Unknown(raw) if raw.kind == "file_change");
                let discovery =
                    crate::channel_discovery::prepare(&mut shell.core, &team, generation, &event);
                let needs_group_hydration = match event.as_ref() {
                    RtEvent::Message(message) | RtEvent::MessageChanged { message, .. } => {
                        let mut ids = std::collections::HashSet::new();
                        super_platinum_core::slack::models::collect_group_ids(message, &mut ids);
                        ids.iter().any(|id| {
                            shell.core.workspaces.get(&team).is_some_and(|w| {
                                w.usergroups.get(id).is_none_or(|g| g.handle.is_empty())
                            })
                        })
                    }
                    RtEvent::Unknown(raw) => raw.kind.starts_with("subteam_"),
                    _ => false,
                };
                let needs_user_hydration = match event.as_ref() {
                    RtEvent::Message(message) => message.user.as_deref().is_some_and(|user| {
                        shell
                            .core
                            .workspaces
                            .get(&team)
                            .is_none_or(|workspace| !workspace.users.contains_key(user))
                    }),
                    _ => false,
                };
                let self_profile_changed = matches!(
                    event.as_ref(),
                    RtEvent::UserChanged(user)
                        if shell.core.workspaces.get(&team)
                            .is_some_and(|workspace| user.id == workspace.self_user_id)
                );
                let prefs_changed = matches!(event.as_ref(), RtEvent::PreferencesChanged { .. });
                let locale_changed = matches!(event.as_ref(), RtEvent::PreferencesChanged { name, .. } if matches!(name.as_str(), "locale" | "time24"));
                let old_locale = locale_changed.then(|| {
                    shell.core.workspaces.get(&team).map(|workspace| {
                        (
                            workspace.locale.clone(),
                            workspace.notifications.user.get("time24").cloned(),
                        )
                    })
                });
                let notification = notifications.for_event_in_view(
                    &shell.core,
                    &team,
                    generation,
                    &event,
                    crate::notification::NotificationView {
                        focused: dioxus::desktop::window().window.is_focused()
                            && (shell.transcript_visible()
                                || (shell.detail_open() && shell.thread_root.is_some())),
                        thread_root: shell.thread_root.as_deref(),
                    },
                );
                #[cfg(target_os = "macos")]
                if notification.is_some() {
                    crate::notification::attention::request(
                        attention_state,
                        &shell.core,
                        &team,
                        generation,
                        &event,
                    );
                }
                // A room that ended takes this user's call with it.
                let huddle_teardown = match event.as_ref() {
                    RtEvent::RoomJoin { room, .. }
                    | RtEvent::RoomLeave { room, .. }
                    | RtEvent::RoomUpdate { room }
                        if shell
                            .core
                            .workspaces
                            .get(&team)
                            .is_some_and(|workspace| workspace.rt_generation == generation) =>
                    {
                        shell.core.huddle.apply_room(&team, room)
                    }
                    _ => None,
                };
                let arrival = match event.as_ref() {
                    RtEvent::Message(message)
                        if message.channel.as_deref() == shell.core.active_channel.as_deref() =>
                    {
                        message.ts.clone()
                    }
                    _ => None,
                };
                // Typing and thread totals are read directly from the workspace;
                // neither requires rebuilding channel or transcript projections.
                let bookmarks_channel =
                    crate::bookmarks::event_channel(&shell, &team, generation, &event);
                let pin_event = matches!(event.as_ref(),RtEvent::Unknown(raw) if matches!(raw.kind.as_str(),"pin_added"|"pin_removed"));
                let pins_generation = shell.core.pins.generation;
                let saved_event = matches!(event.as_ref(),RtEvent::Unknown(raw) if matches!(raw.kind.as_str(),"saved_added"|"saved_updated"|"saved_deleted"));
                let draft_event = matches!(event.as_ref(),RtEvent::Unknown(raw) if matches!(raw.kind.as_str(),"draft_create"|"draft_update"|"draft_delete"));
                let later_generation = shell.core.later.generation;
                let later_needs_reload = shell.core.later.loading || !shell.core.later.loaded;
                let thread_totals_event = matches!(event.as_ref(), RtEvent::GlobalThreadState(_));
                let activity_counts_event = matches!(
                    event.as_ref(),
                    RtEvent::BadgeCountsUpdated(_)
                        | RtEvent::LegacyBadgeCountsUpdated(_)
                        | RtEvent::CombinedBadgeCountsUpdated { .. }
                );
                let old_thread_totals = thread_totals_event
                    .then(|| {
                        shell
                            .core
                            .workspaces
                            .get(&team)
                            .and_then(|w| w.thread_counts.snapshot.clone())
                    })
                    .flatten();
                let typing_only = bookmarks_channel.is_some()
                    || thread_totals_event
                    || activity_counts_event
                    || matches!(event.as_ref(), RtEvent::UserTyping { .. })
                    || saved_event
                    || draft_event
                    || file_change;
                let metadata_revision = shell
                    .core
                    .workspaces
                    .get(&team)
                    .map(|w| w.channel_lifecycle.revision());
                let draft_revision = shell
                    .core
                    .workspaces
                    .get(&team)
                    .map(|w| w.draft_presence.revision());
                let thread_marked = matches!(event.as_ref(), RtEvent::ThreadMarked { .. });
                let transcript_visible = shell.reading_latest(crate::read_visibility::focused());
                let activity_names_revision =
                    crate::app_home::about::management::activity::refresh::policy_revision(&shell);
                let home_load = crate::app_home::realtime(&mut shell, &team, generation, &event);
                let observations_changed = crate::app_home::autocomplete::startup::relevant_event(
                    &shell, &team, generation, &event,
                );
                let modal_load = crate::app_modal::realtime(&mut shell, &team, generation, &event);
                let list_labels =
                    crate::slack_lists::labels::prepare(&shell, &team, generation, &event);
                let upload =
                    crate::messaging::upload_recovery::event(&shell, &team, generation, &event);
                let external_history_event = matches!(event.as_ref(), RtEvent::Unknown(raw)
                    if matches!(raw.kind.as_str(), "channel_updated" | "channel_joined" |
                        "group_joined" | "mpim_joined" | "channel_created" | "im_created" |
                        "channel_converted_to_shared" | "group_converted_to_shared"));
                let channel_event = crate::app_home::channel_fetches::before_event(
                    &shell, &team, generation, &event,
                );
                let member_event = matches!(event.as_ref(), RtEvent::UserChanged(_))
                    || matches!(event.as_ref(), RtEvent::Unknown(raw) if raw.kind == "user_invalidated");
                let member_event = member_event.then(|| event.clone());
                let group_work =
                    crate::usergroups::prepare_event(&mut shell, &team, generation, &event);
                let group_tracked = group_work.is_some();
                apply_in_view_with_groups(
                    &mut shell.core,
                    &team,
                    generation,
                    *event,
                    transcript_visible,
                    !group_tracked,
                );
                let channel_work =
                    crate::app_home::channel_fetches::after_event(&mut shell, channel_event);
                if let Some(event) = member_event {
                    crate::directory_member_fetches::automatic::event(
                        &mut shell, &team, generation, &event,
                    );
                }
                let draft_presence_changed = draft_revision
                    != shell
                        .core
                        .workspaces
                        .get(&team)
                        .map(|w| w.draft_presence.revision());
                if draft_presence_changed {
                    crate::app_home::about::management::activity::drafts_changed(&mut shell, &team);
                }
                if let Some((channel, ts)) = upload {
                    crate::messaging::upload_recovery::reconcile(
                        &mut shell,
                        &team,
                        &channel,
                        Some(std::slice::from_ref(&ts)),
                    );
                }
                if let Some(change) = list_labels {
                    change.apply(&mut shell);
                }
                if old_locale.is_some_and(|old| {
                    old != shell.core.workspaces.get(&team).map(|workspace| {
                        (
                            workspace.locale.clone(),
                            workspace.notifications.user.get("time24").cloned(),
                        )
                    })
                }) {
                    crate::slack_lists::locale_changed(&mut shell, &team);
                }
                if thread_marked {
                    crate::thread_unread::reconcile(&mut shell);
                }
                let thread_totals_changed = thread_totals_event
                    && shell
                        .core
                        .workspaces
                        .get(&team)
                        .and_then(|w| w.thread_counts.snapshot.as_ref())
                        != old_thread_totals.as_ref();
                let metadata_changed = shell
                    .core
                    .workspaces
                    .get(&team)
                    .map(|w| w.channel_lifecycle.revision())
                    != metadata_revision;
                let pins_changed = pin_event && shell.core.pins.generation != pins_generation;
                let pins_channel = shell
                    .core
                    .active_channel
                    .clone()
                    .filter(|_| shell.channel_tab == crate::state::ChannelTab::Pins);
                let later_changed = saved_event && shell.core.later.generation != later_generation;
                let later_filter = shell.core.later.filter;
                if let Some(arrival) = arrival {
                    let now = std::time::Instant::now();
                    shell.message_arrivals.insert(arrival, now);
                    shell.realtime_insert_started = Some(now);
                }
                if presence_only {
                    presence::refresh(&mut shell, &team);
                } else if let Some(channel) = reaction_channel {
                    reactions::refresh(&mut shell, &team, generation, &channel);
                } else if !typing_only {
                    shell.refresh_from_core();
                } else if activity_counts_event || thread_totals_event || saved_event {
                    shell.refresh_dock_badge_for(&team);
                }
                drop(shell);
                if observations_changed {
                    crate::app_home::autocomplete::startup::schedule(state, &team);
                }
                crate::app_home::about::management::activity::refresh::after_event(
                    state,
                    activity_names_revision,
                );
                // Queue while the accepted frame owner is still current, before
                // the unrelated owned-draft await below can switch accounts.
                crate::app_home::channel_fetches::resume(state, channel_work);
                if let Some(work) = group_work {
                    crate::usergroups::source::resume(state, work);
                }
                if external_history_event || draft_presence_changed {
                    crate::bootstrap::persist_workspace_metadata(&state, &team);
                }
                if let Some(raw) = owned_draft {
                    if owned_socket
                        .event(state, &team, generation, raw)
                        .await
                        .is_err()
                    {
                        state.write().managed_drafts.error =
                            Some("A draft update needs review. Your input is retained.");
                    }
                }
                if list_refresh {
                    crate::slack_lists::refresh(state);
                }
                if home_load {
                    dioxus::core::spawn_forever(crate::app_home::refresh(state));
                }
                if modal_load {
                    crate::app_modal::load(state);
                }
                if let Some(read) = file_read {
                    dioxus::core::spawn_forever(read.run(state));
                }
                if let Some(discovery) = discovery {
                    dioxus::prelude::spawn(discovery.run(state));
                }
                if let Some(channel) = bookmarks_channel {
                    dioxus::core::spawn_forever(crate::bookmarks::load(state, channel));
                }
                if pins_changed {
                    if let Some(channel) = pins_channel {
                        dioxus::core::spawn_forever(crate::pins::load(state, channel));
                    }
                }
                if later_changed {
                    if later_needs_reload {
                        dioxus::prelude::spawn(crate::later::load(state, later_filter, false));
                    } else {
                        dioxus::prelude::spawn(crate::later::hydrate_visible(state));
                    }
                }
                if prefs_changed
                    || metadata_changed
                    || thread_totals_changed
                    || activity_counts_event
                {
                    crate::bootstrap::persist_workspace(&state, &team);
                }
                if needs_user_hydration || self_profile_changed {
                    dioxus::prelude::spawn(crate::bootstrap::hydrate_current_surface(state));
                }
                if needs_group_hydration {
                    let team = team.clone();
                    dioxus::prelude::spawn(async move {
                        crate::usergroups::hydrate(state, &team).await;
                    });
                }
                if let Some(notification) = notification {
                    dioxus::prelude::spawn(crate::notification::show(notification));
                }
                if let Some(teardown) = huddle_teardown {
                    dioxus::prelude::spawn(crate::huddle::finish(state, teardown));
                }
            }
            RtUpdate::Disconnected { generation } => {
                owned_socket.disconnected(&mut shell, &team, generation);
                crate::file_share::connection(&mut shell, &team, generation);
                mark_disconnected(&mut shell, &team, generation);
            }
        }
    }
    owned_socket.retire(&mut state.write());
    let _ = owned_socket
        .close(super_platinum_core::drafts::store::protected::GapReason::Disconnect)
        .await;
}

/// Apply an owned disconnect without inventing a new accepted connection.
/// Closed native fixtures use this same transition to exercise pending readers.
pub(crate) fn mark_disconnected(shell: &mut ShellState, team: &str, generation: u64) -> bool {
    let Some(workspace) = shell
        .core
        .workspaces
        .get_mut(team)
        .filter(|workspace| generation >= workspace.rt_generation)
    else {
        return false;
    };
    workspace.rt = super_platinum_core::state::RealtimeStatus::Disconnected;
    workspace.posting_metadata.invalidate();
    crate::directory_member_fetches::automatic::disconnected(shell, team, generation);
    true
}

fn worker_source(
    core: &super_platinum_core::CoreAppState,
    params: &ConnectParams,
) -> Option<(u64, std::sync::Arc<super_platinum_core::slack::Transport>)> {
    let session = core.session.as_ref()?;
    let workspace = session.workspaces.get(&params.team)?;
    // The spawned future can first poll after the account was replaced.
    if params.d_cookie != session.d_cookie
        || params.ws_url != realtime::flannel_url(&workspace.token, &params.team)
    {
        return None;
    }
    Some((core.account_epoch, core.transport.clone()?))
}

async fn refresh_open_metadata(mut state: Signal<ShellState>, generation: u64) {
    let Some(ctx) = crate::scheduled::Context::get(&state) else {
        return;
    };
    let Some((channel, revision)) = ({
        let shell = state.peek();
        shell.core.active_channel.clone().zip(
            shell
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .map(|w| w.channel_lifecycle.revision()),
        )
    }) else {
        return;
    };
    let posting = crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session)
        .for_channels([&channel]);
    let result = super_platinum_core::slack::api::fetch_conversation_details(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        &channel,
    )
    .await;
    if !ctx.current(&state.peek())
        || state
            .peek()
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .is_none_or(|w| w.rt_generation != generation)
    {
        return;
    }
    if let Ok(info) = result {
        posting.channels(&state, std::slice::from_ref(&info));
        {
            let mut shell = state.write();
            if let Some(w) = shell.core.workspaces.get_mut(&ctx.session.team_id) {
                w.merge_channel_details(info, revision);
            }
            shell.refresh_from_core();
        }
        crate::bootstrap::persist_workspace(&state, &ctx.session.team_id);
    }
}

#[cfg(test)]
fn apply(
    core: &mut super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    event: RtEvent,
) {
    apply_in_view(core, team, generation, event, true);
}

pub(crate) fn apply_in_view(
    core: &mut super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    event: RtEvent,
    transcript_visible: bool,
) {
    apply_in_view_with_groups(core, team, generation, event, transcript_visible, true);
}

fn apply_in_view_with_groups(
    core: &mut super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    event: RtEvent,
    transcript_visible: bool,
    apply_groups: bool,
) {
    let Some(workspace) = core.workspaces.get(team) else {
        return;
    };
    if generation != workspace.rt_generation {
        return;
    }
    if let RtEvent::Unknown(raw) = &event
        && let Some(id) = super_platinum_core::file_actions::deleted_event(raw)
    {
        core.remove_file(team, id);
        return;
    }
    if core.active_team.as_deref() == Some(team)
        && core.threads_view.matches_scope(core.account_epoch, team)
    {
        if let RtEvent::GlobalThreadState(counts) = &event
            && workspace.thread_counts.snapshot.as_ref() != Some(counts)
        {
            core.threads_view.dirty = true;
        }
        core.threads_view.observe(&event);
    }
    if let RtEvent::Unknown(raw) = &event
        && let Some((channel, root, active)) =
            super_platinum_core::thread_subscriptions::from_event(raw)
    {
        let key = (team.into(), channel, root);
        core.thread_subscriptions
            .apply(core.account_epoch, &key, active);
        core.reconcile_thread_subscription(&key);
    }
    super_platinum_core::state::posting_metadata::authority::observe_event(
        core, team, generation, &event,
    );
    let workspace = core.workspaces.get_mut(team).unwrap();
    workspace.apply_channel_event(&event);
    match event {
        RtEvent::DesktopNotification(_) => {}
        RtEvent::PreferencesChanged { name, value } => {
            if name == "locale" {
                workspace.locale = value.as_str().map(str::to_owned);
            }
            workspace.notifications.apply_change(&name, value.clone());
            workspace
                .emoji_preferences
                .apply_change(&name, value.clone());
            workspace
                .read_preferences
                .apply_change(&name, value.clone());
            workspace
                .clear_unreads_preference
                .apply_change(&name, value.clone());
            // Sidebar and notifications share the same mute preference.
            if name == "all_notifications_prefs" {
                workspace.sidebar.sync_notification_mutes(
                    &workspace.notifications,
                    super_platinum_core::state::now_secs(),
                );
            }
            if name == "vip_users" {
                workspace.apply_vip_snapshot(
                    value
                        .as_str()
                        .unwrap_or_default()
                        .split(',')
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned)
                        .collect(),
                );
            }
        }
        RtEvent::Message(message) => {
            let Some(channel) = message.channel.clone() else {
                return;
            };
            if let Some(user) = message.user.as_deref() {
                workspace.clear_typing_user(&channel, user);
            }
            let thread_root = message
                .thread_ts
                .clone()
                .filter(|root| message.ts.as_deref() != Some(root));
            if let Some(root) = thread_root {
                let bag = core
                    .threads
                    .entry((team.to_owned(), channel.clone(), root.clone()))
                    .or_default();
                bag.merge_update(super_platinum_core::state::visible_message(message.clone()));
                bag.recount_thread_unreads(&root);
                if message.subtype.as_deref() != Some("thread_broadcast") {
                    return;
                }
            }
            let group_ping = message.user.as_deref() != Some(&workspace.self_user_id)
                && workspace.usergroups.values().any(|group| {
                    group.includes(&workspace.self_user_id) && group.mentioned_in(&message)
                });
            let viewing = transcript_visible
                && core.active_team.as_deref() == Some(team)
                && core.active_channel.as_deref() == Some(channel.as_str());
            workspace.count_incoming_message(&channel, &message, viewing, group_ping);
            // Merged, not replaced. A frame that re-sends a message we already
            // hold is a partial: `message_replied` carries the parent's text and
            // reply counts but no `reactions` at all, so overwriting the stored
            // copy with it wiped every pill off a message the moment anyone
            // replied to it. Slack merges these frames field by field, and so
            // must we; a wholesale replacement is what `conversations.history`
            // is for, where the payload really is the whole message.
            workspace
                .messages
                .entry(channel)
                .or_default()
                .merge_update(super_platinum_core::state::visible_message(message));
        }
        RtEvent::MessageChanged {
            channel, message, ..
        } => {
            if core.later.matches_scope(core.account_epoch, team) {
                core.later.update_message(&channel, &message);
            }
            workspace
                .messages
                .entry(channel.clone())
                .or_default()
                .merge_update(message.clone());
            for ((thread_team, thread_channel, _), messages) in &mut core.threads {
                if thread_team == team && thread_channel == &channel {
                    messages.merge_update(message.clone());
                }
            }
            core.update_pinned_message(team, &channel, &message);
        }
        RtEvent::MessageDeleted {
            channel,
            deleted_ts,
        } => {
            if core.pins.matches_scope(core.account_epoch, team) {
                core.pins.delete_message(&channel, &deleted_ts);
            }
            if core.later.matches_scope(core.account_epoch, team) {
                core.later.delete_message(&channel, &deleted_ts);
            }
            // Retain the event even when this channel or row has not loaded:
            // an upload verification page already on the wire may contain it.
            workspace
                .messages
                .entry(channel.clone())
                .or_default()
                .record_remote_deletion(&deleted_ts);
            for ((thread_team, thread_channel, _), messages) in &mut core.threads {
                if thread_team == team && thread_channel == &channel {
                    messages.record_remote_deletion(&deleted_ts);
                }
            }
        }
        RtEvent::UserTyping { channel, user } => {
            workspace.set_typing(&channel, user, std::time::Instant::now())
        }
        RtEvent::PresenceChange { users, presence } => {
            let presence = super_platinum_core::state::Presence::from_slack(&presence);
            for user in users {
                workspace.set_presence(user, presence);
            }
        }
        RtEvent::UserChanged(user) => workspace.apply_user_update(user),
        RtEvent::DndUpdated { user, dnd } => {
            if user == workspace.self_user_id {
                workspace.self_dnd = dnd;
            }
        }
        RtEvent::ReactionAdded {
            channel,
            ts,
            user,
            reaction,
        } => apply_reaction(core, team, &channel, &ts, &user, &reaction, true),
        RtEvent::ReactionRemoved {
            channel,
            ts,
            user,
            reaction,
        } => apply_reaction(core, team, &channel, &ts, &user, &reaction, false),
        RtEvent::BadgeCountsUpdated(counts) => activity_counts::apply_live(core, team, counts),
        RtEvent::LegacyBadgeCountsUpdated(counts) => {
            if workspace.activity_inbox_enabled() == Some(false) {
                workspace
                    .dock_counts
                    .merge_channel_badges(&serde_json::json!(counts));
            }
        }
        RtEvent::CombinedBadgeCountsUpdated { activity, legacy } => {
            match workspace.activity_inbox_enabled() {
                Some(true) => activity_counts::apply_live(core, team, activity),
                Some(false) => {
                    workspace
                        .dock_counts
                        .merge_channel_badges(&serde_json::json!(legacy));
                }
                None => {}
            }
        }
        RtEvent::ActivityUpdated(item) => {
            if core.active_team.as_deref() == Some(team) {
                core.activity.ensure_scope(core.account_epoch, team);
                core.activity.upsert(item);
                core.activity.loaded = true;
            }
        }
        RtEvent::RoomJoin { room, .. }
        | RtEvent::RoomLeave { room, .. }
        | RtEvent::RoomUpdate { room } => {
            workspace.apply_room(room);
        }
        RtEvent::HuddleInvite(invite) => {
            core.huddle
                .add_invite(team.to_owned(), invite, std::time::Instant::now());
        }
        RtEvent::HuddleInviteCancel { channel } => {
            core.huddle.cancel_invite(team, &channel);
        }
        RtEvent::GlobalThreadState(counts) => workspace.thread_counts.apply_live(counts),
        RtEvent::ThreadMarked {
            channel,
            root_ts,
            last_read,
        } => {
            core.apply_thread_mark(&(team.into(), channel.clone(), root_ts.clone()), &last_read);
            activity_counts::mark_thread_read(core, team, &channel, &root_ts, &last_read);
        }
        RtEvent::ChannelMarked {
            channel,
            ts,
            unread_count,
            mention_count,
        } => {
            let unread_count = unread_count.unwrap_or(0);
            let mention_count = mention_count.unwrap_or(0);
            let messages = workspace.messages.entry(channel.clone()).or_default();
            messages.last_read = Some(ts.clone());
            messages.read_revision = messages.read_revision.wrapping_add(1);
            messages.unread_count = unread_count;
            messages.mention_count = mention_count;
            if mention_count == 0 {
                messages.vip_count = 0;
            }
            if let Some(channel) = workspace.channels.get_mut(&channel) {
                channel.last_read = Some(ts.clone());
                channel.unread_count = Some(unread_count);
                channel.unread_count_display = Some(unread_count);
                channel.mention_count = Some(mention_count);
                channel.has_unreads = unread_count > 0 || mention_count > 0;
            }
            activity_counts::mark_channel_read(core, team, &channel, &ts);
        }
        RtEvent::Unknown(raw) => {
            super_platinum_core::composer_emoji::changes::apply(workspace, &raw);
            // Canonical saved counts belong to every workspace, independently
            // of which account-scoped Later page is currently visible.
            if matches!(
                raw.kind.as_str(),
                "saved_added" | "saved_updated" | "saved_deleted"
            ) && let Some(counts) = raw.rest.get("counts")
            {
                workspace.dock_counts.apply_saved_counts(counts);
            }
            if matches!(
                raw.kind.as_str(),
                "draft_create" | "draft_update" | "draft_delete" | "draft_send"
            ) {
                let payload = serde_json::Value::Object(raw.rest.clone().into_iter().collect());
                let _ = workspace.draft_presence.realtime(team, &raw.kind, &payload);
                if core.active_team.as_deref() == Some(team) {
                    core.scheduled.ensure_scope(core.account_epoch, team);
                    core.scheduled.apply_event(&raw);
                }
                return;
            }
            if let Some(update) = super_platinum_core::pins::PinUpdate::from_event(&raw) {
                if core.active_team.as_deref() == Some(team) {
                    core.pins.ensure_scope(core.account_epoch, team);
                    if core.apply_pin_event(team, &update) {
                        if let Some(ts) = update.item.message.as_ref().and_then(|m| m.ts.as_ref()) {
                            core.update_pin_copies(team, &update.channel, ts, update.pinned);
                        }
                    }
                }
                return;
            }
            if apply_groups {
                super_platinum_core::slack::models::apply_group_event(
                    &mut workspace.usergroups,
                    &workspace.self_user_id,
                    &raw,
                );
            }
            if core.active_team.as_deref() == Some(team)
                && matches!(
                    raw.kind.as_str(),
                    "saved_added" | "saved_updated" | "saved_deleted"
                )
            {
                core.later.ensure_scope(core.account_epoch, team);
                core.later.apply_event(&raw);
            }
        }
    }
}

/// Puts a reaction on every copy of the message the app holds.
///
/// The transcript is not the only place a message is rendered from: a thread
/// bag holds its own copy of the root and of every reply, and that is what the
/// thread pane draws — the pane Activity opens beside its list included. Only
/// updating the channel meant a pill added while a thread was open never
/// appeared there, which is exactly where a reacted message is read from.
///
/// A channel with no transcript loaded is left alone rather than given an empty
/// one: the reaction is already on the server copy, and the history fetch that
/// opens the conversation brings it in.
pub(crate) fn apply_reaction(
    core: &mut super_platinum_core::CoreAppState,
    team: &str,
    channel: &str,
    ts: &str,
    user: &str,
    name: &str,
    added: bool,
) {
    if core.pins.matches_scope(core.account_epoch, team) {
        core.pins.apply_reaction(channel, ts, user, name, added);
    }
    if let Some(messages) = core
        .workspaces
        .get_mut(team)
        .and_then(|workspace| workspace.messages.get_mut(channel))
    {
        messages.apply_reaction(ts, user, name, added);
    }
    for ((thread_team, thread_channel, _), messages) in &mut core.threads {
        if thread_team == team && thread_channel == channel {
            messages.apply_reaction(ts, user, name, added);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod pin_file_ingress_tests;
