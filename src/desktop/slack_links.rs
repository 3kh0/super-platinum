//! Typed Slack navigation. Link ownership is resolved before changing surfaces;
//! a delayed lookup may never steal navigation back from a newer user action.
use std::sync::Arc;

use dioxus::prelude::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        Transport, api,
        models::{Channel, GenericInfoPage},
    },
};

use crate::state::{ChannelOpen, ChannelTab, MainView, Overlay, ShellState};

mod guarded;
mod parse;
pub(crate) use guarded::{NavigationGuard, ReadLifecycle, open_channel_guarded};
mod user;
pub(crate) mod user_check;
pub(crate) mod user_fixture;
use parse::{Link, WorkspaceRef};

const UNAVAILABLE: &str = "This Slack workspace is not signed in here.";
const UNSUPPORTED: &str = "This Slack link is not supported yet.";

/// Classify routing without opening a browser or changing the visible workspace.
pub(crate) fn target_team(shell: &ShellState, input: &str) -> Result<Option<String>, &'static str> {
    let link = parse::parse(input).map_err(|_| "This link is invalid or unsupported.")?;
    match link {
        Link::External { .. } => Ok(None),
        Link::Channel { workspace, .. } => resolve_team(shell, &workspace).map(Some),
        Link::Workspace { team } => resolve_team(shell, &WorkspaceRef::TeamId(team)).map(Some),
        Link::User { .. } | Link::UnhandledSlack { .. } => Err(UNSUPPORTED),
    }
}

/// Cross-workspace app-action navigation would invalidate the action's modal
/// ownership. Ordinary links can switch workspaces; buttons cannot yet do so.
pub(crate) fn button_url_allowed(shell: &ShellState, input: &str) -> Result<(), &'static str> {
    if let Some(team) = target_team(shell, input)?
        && shell.core.active_team.as_ref() != Some(&team)
    {
        return Err("App buttons cannot open another workspace yet.");
    }
    Ok(())
}

fn resolve_team(shell: &ShellState, target: &WorkspaceRef) -> Result<String, &'static str> {
    let session = shell.core.session.as_ref().ok_or(UNAVAILABLE)?;
    let mut matches = session.workspaces.iter().filter(|(id, session)| {
        session.team_id == **id
            && shell
                .core
                .workspaces
                .get(*id)
                .is_some_and(|workspace| workspace.self_user_id == session.user_id)
            && shell
                .workspaces
                .iter()
                .any(|workspace| workspace.id == **id)
            && match target {
                WorkspaceRef::TeamId(team) => *id == team,
                WorkspaceRef::Host(host) => url::Url::parse(&session.url)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                    .is_some_and(|candidate| canonical_host(&candidate) == canonical_host(host)),
            }
    });
    let team = matches
        .next()
        .map(|(id, _)| id.clone())
        .ok_or(UNAVAILABLE)?;
    if matches.next().is_some() {
        return Err("This Slack link does not identify a unique signed-in workspace.");
    }
    Ok(team)
}

fn canonical_host(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    // Match core api::forward's own permalink writer. Ownership still requires
    // one unique signed-in session after normalization; no workspace guessing.
    host.strip_suffix(".enterprise.slack.com")
        .map(|prefix| format!("{prefix}.slack.com"))
        .unwrap_or(host)
}

pub(crate) type FailureFeedback = Option<std::rc::Rc<dyn Fn(&ShellState, &str)>>;

fn failure(shell: &mut ShellState, feedback: &FailureFeedback, message: &str) {
    shell.show_toast(message);
    if let Some(feedback) = feedback {
        feedback(shell, message);
    }
}

fn report_failure(
    shell: &mut ShellState,
    feedback: &FailureFeedback,
    error: &super_platinum_core::slack::Error,
    offline: &str,
    detail: &str,
) {
    use super_platinum_core::error::NetworkFailure;
    shell.report_failure(error, offline, || detail.into());
    if let Some(feedback) = feedback {
        feedback(shell, if error.is_offline() { offline } else { detail });
    }
}

pub(crate) fn open(state: Signal<ShellState>, input: &str) {
    open_with_feedback(state, input, None);
}

pub(crate) fn open_with_feedback(
    mut state: Signal<ShellState>,
    input: &str,
    feedback: FailureFeedback,
) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::SlackLink(input.into()),
    ) {
        return;
    }
    let link = match parse::parse(input) {
        Ok(link) => link,
        Err(_) => {
            failure(
                &mut state.write(),
                &feedback,
                "This link is invalid or unsupported.",
            );
            return;
        }
    };
    if let Link::External { url } = link {
        if crate::media::open_external(&url).is_err() {
            failure(&mut state.write(), &feedback, "Could not open this link.");
        }
        return;
    }
    if let Link::User { team, user } = link {
        let intent = {
            let mut shell = state.write();
            match user::Intent::begin(&mut shell, team, user) {
                Ok(intent) => intent,
                Err(message) => {
                    failure(&mut shell, &feedback, message);
                    return;
                }
            }
        };
        if let Some(intent) = intent {
            dioxus::core::spawn_forever(user::navigate(state, intent, feedback));
        }
        return;
    }
    let target = match &link {
        Link::Channel { workspace, .. } => workspace.clone(),
        Link::Workspace { team } => WorkspaceRef::TeamId(team.clone()),
        _ => {
            failure(&mut state.write(), &feedback, UNSUPPORTED);
            return;
        }
    };
    let visit = {
        let mut shell = state.write();
        match Visit::begin(&mut shell, &target, matches!(link, Link::Workspace { .. })) {
            Ok(visit) => visit,
            Err(message) => {
                failure(&mut shell, &feedback, message);
                return;
            }
        }
    };
    dioxus::core::spawn_forever(async move {
        if !visit.current(&state.peek()) {
            return;
        }
        match link {
            Link::Workspace { .. } => {
                crate::bootstrap::refresh_selected_channel(state).await;
                if visit.current(&state.peek()) {
                    crate::bootstrap::hydrate_current_surface(state).await;
                }
            }
            Link::Channel {
                channel,
                message,
                thread,
                ..
            } => {
                navigate(state, visit, channel, message, thread, feedback).await;
            }
            _ => {}
        }
    });
}

struct Visit {
    epoch: u64,
    session: WorkspaceSession,
    transport: Option<Arc<Transport>>,
    channel: u64,
    thread: u64,
    profile: u64,
    link: u64,
    palette: u64,
    search: u64,
    shortcut: u64,
    overlay: Option<Overlay>,
    modal: Option<(u64, Option<String>)>,
    view: MainView,
    tab: ChannelTab,
}

impl Visit {
    fn begin(
        shell: &mut ShellState,
        target: &WorkspaceRef,
        reset_workspace: bool,
    ) -> Result<Self, &'static str> {
        let team = resolve_team(shell, target)?;
        let index = shell
            .workspaces
            .iter()
            .position(|workspace| workspace.id == team)
            .ok_or(UNAVAILABLE)?;
        if (reset_workspace || shell.core.active_team.as_ref() != Some(&team))
            && !shell.select_workspace(index)
        {
            return Err(UNAVAILABLE);
        }
        // A workspace link completes navigation at selection; unlike channel
        // and user links it has no later select_target commit to close Canvas.
        // Keep this after selection succeeds so departure refusals stay open.
        if reset_workspace {
            shell.overlay = None;
        }
        // Reserve only a link intent: missing metadata must not cancel a
        // foreground history or paging request if this navigation later fails.
        shell.slack_link_generation = shell.slack_link_generation.wrapping_add(1);
        Self::capture(shell)
    }

    fn capture(shell: &ShellState) -> Result<Self, &'static str> {
        let team = shell.core.active_team.as_ref().ok_or(UNAVAILABLE)?;
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(team))
            .cloned()
            .ok_or(UNAVAILABLE)?;
        Ok(Self {
            epoch: shell.core.account_epoch,
            session,
            transport: shell.core.transport.clone(),
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            profile: shell.profile_generation,
            link: shell.slack_link_generation,
            palette: shell.palette_generation,
            search: shell.search_open_generation,
            shortcut: shell.shortcut_generation,
            overlay: shell.overlay,
            modal: modal_revision(shell),
            view: shell.main_view,
            tab: shell.channel_tab,
        })
    }

    fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.session.team_id)
            && shell
                .core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| w.self_user_id == self.session.user_id)
            && shell
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(&self.session.team_id))
                .is_some_and(|s| s.user_id == self.session.user_id && s.token == self.session.token)
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.profile_generation == self.profile
            && shell.slack_link_generation == self.link
            && shell.palette_generation == self.palette
            && shell.search_open_generation == self.search
            && shell.shortcut_generation == self.shortcut
            && shell.overlay == self.overlay
            && modal_revision(shell) == self.modal
            && shell.main_view == self.view
            && shell.channel_tab == self.tab
    }

    fn resolve(&self, shell: &mut ShellState, id: &str, channel: Channel, revision: u64) -> bool {
        if !self.current(shell) || channel.id != id {
            return false;
        }
        let Some(workspace) = shell.core.workspaces.get_mut(&self.session.team_id) else {
            return false;
        };
        workspace.apply_channel_refresh_since(
            GenericInfoPage {
                channels: vec![channel],
                ..Default::default()
            },
            revision,
        );
        true
    }
}

fn modal_revision(shell: &ShellState) -> Option<(u64, Option<String>)> {
    let modal = &shell.app_modal.as_ref()?.modal;
    Some((
        modal.revision(),
        modal.stack().last().map(|view| view.id.clone()),
    ))
}

fn select_target(shell: &mut ShellState, channel: &str, anchor: Option<&str>) -> bool {
    if crate::canvas_create::departure(shell) {
        return false;
    }
    if let Some(anchor) = anchor {
        return shell.open_search_result(channel, anchor);
    }
    if !shell.channels.iter().any(|row| row.id == channel) {
        let Some(workspace) = shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))
        else {
            return false;
        };
        let Some(info) = workspace.channels.get(channel) else {
            return false;
        };
        shell
            .channels
            .push(crate::channel_vm::channel_vm(workspace, info, &shell.media));
    }
    let Some(index) = shell.channels.iter().position(|row| row.id == channel) else {
        return false;
    };
    shell.select_channel(index, ChannelOpen::Global);
    shell.overlay = None;
    true
}

async fn navigate(
    mut state: Signal<ShellState>,
    visit: Visit,
    channel: String,
    message: Option<String>,
    thread: Option<String>,
    feedback: FailureFeedback,
) {
    let (known, revision) = {
        let shell = state.peek();
        if !visit.current(&shell) {
            return;
        }
        let workspace = &shell.core.workspaces[&visit.session.team_id];
        (
            workspace.channels.contains_key(&channel),
            workspace.channel_lifecycle.revision(),
        )
    };
    if !known {
        let Some(transport) = visit.transport.as_ref() else {
            failure(
                &mut state.write(),
                &feedback,
                "Sign in to open this conversation.",
            );
            return;
        };
        let client = state.peek().core.client.clone();
        let posting = crate::bootstrap::posting_ingress::capture(&state, transport, &visit.session);
        let result =
            api::fetch_conversation_info(transport, &client, &visit.session, channel.clone()).await;
        if !visit.current(&state.peek()) {
            return;
        }
        if let Ok(info) = &result
            && info.id == channel
        {
            posting.channels(&state, std::slice::from_ref(info));
        }
        let mut shell = state.write();
        match result {
            Ok(info) => {
                if !visit.resolve(&mut shell, &channel, info, revision) {
                    failure(&mut shell, &feedback, "This conversation is unavailable.");
                    return;
                }
            }
            Err(error) => {
                report_failure(
                    &mut shell,
                    &feedback,
                    &error,
                    "Connect to Slack to open this conversation.",
                    "Could not open this conversation. It may no longer be accessible.",
                );
                return;
            }
        }
    }
    let next = {
        let mut shell = state.write();
        if !visit.current(&shell) {
            return;
        }
        if !select_target(
            &mut shell,
            &channel,
            thread.as_deref().or(message.as_deref()),
        ) {
            failure(&mut shell, &feedback, "This conversation is unavailable.");
            return;
        }
        let Ok(next) = Visit::capture(&shell) else {
            return;
        };
        next
    };
    crate::bootstrap::refresh_selected_channel(state).await;
    if next.current(&state.peek()) {
        if let Some(root) = thread {
            crate::bootstrap::open_thread_at(state, channel, root, message).await;
        }
    }
}

#[cfg(test)]
mod tests;
