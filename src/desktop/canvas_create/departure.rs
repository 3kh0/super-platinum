//! One current-dialog navigation intent. Durable content custody stays in Root;
//! this UI authority is discarded on replacement and never carried at startup.
use super::{Draft, Overlay, ShellState, adapter, controller, domain};
use crate::state::{CanvasTarget, MainView, SettingsSection};
use dioxus::prelude::*;
use super_platinum_core::{
    later::SavedKey,
    slack::models::{File, Message},
};

#[derive(Clone)]
pub(crate) enum Destination {
    Palette,
    Search {
        conversation: bool,
    },
    SearchQuery(String),
    Settings(SettingsSection),
    SelfMenu,
    Workspace(String),
    Surface(MainView),
    Canvas(CanvasTarget),
    File(Box<File>),
    SlackLink(String),
    AppHome(super_platinum_core::block_actions::home::navigation::Target),
    SignIn,
    SwitchAccount(String),
    RemoveAccount(String),
    Activity {
        key: String,
        channel: Option<String>,
        ts: Option<String>,
        thread: Option<String>,
    },
    Message {
        channel: String,
        ts: String,
    },
    Later(SavedKey),
    Pinned {
        channel: String,
        message: Box<Message>,
    },
}

#[derive(Clone)]
struct Origin {
    owner: domain::Owner,
    epoch: u64,
    visit: String,
    shortcut: u64,
    channel: u64,
    active_channel: Option<String>,
    thread: u64,
    view: MainView,
}
impl Origin {
    fn capture(shell: &ShellState, d: &Draft) -> Option<Self> {
        let owner = d.owner.clone()?;
        if controller::owner(shell).as_ref() != Some(&owner) {
            return None;
        }
        Some(Self {
            owner,
            epoch: shell.core.account_epoch,
            visit: d.visit.clone(),
            shortcut: shell.shortcut_generation,
            channel: shell.channel_generation,
            active_channel: shell.core.active_channel.clone(),
            thread: shell.thread_generation,
            view: shell.main_view,
        })
    }
    fn owner_current(&self, shell: &ShellState) -> bool {
        controller::owner(shell).as_ref() == Some(&self.owner)
            && shell.core.account_epoch == self.epoch
            && shell.shortcut_generation == self.shortcut
            && shell.channel_generation == self.channel
            && shell.core.active_channel == self.active_channel
            && shell.thread_generation == self.thread
            && shell.main_view == self.view
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owner_current(shell)
            && shell.overlay == Some(Overlay::CanvasCreate)
            && shell
                .canvas_create
                .as_ref()
                .is_some_and(|d| d.visit == self.visit && d.owner.as_ref() == Some(&self.owner))
    }
    fn closed(&self, shell: &ShellState) -> bool {
        self.owner_current(shell) && shell.overlay.is_none() && shell.canvas_create.is_none()
    }
}

#[derive(Clone)]
pub(super) struct Pending {
    origin: Origin,
    destination: Destination,
}
/// An affine, already-consumed intent; callers cannot construct or clone it.
pub(super) struct Continuation {
    origin: Origin,
    destination: Destination,
}

/// Returns true when the original route must stop. First explicit intent wins;
/// subsequent handlers cannot replace it while native capture/save is pending.
pub(crate) fn request(shell: &mut ShellState, destination: Destination) -> bool {
    if !controller::departure(shell) {
        return false;
    }
    let origin = shell
        .canvas_create
        .as_ref()
        .and_then(|d| Origin::capture(shell, d));
    if let (Some(origin), Some(d)) = (origin, shell.canvas_create.as_mut()) {
        if d.pending_departure.is_none() {
            d.pending_departure = Some(Pending {
                origin,
                destination,
            });
        }
    }
    true
}
pub(super) fn pending(shell: &ShellState, visit: &str) -> bool {
    shell
        .canvas_create
        .as_ref()
        .is_some_and(|d| d.visit == visit && d.pending_departure.is_some())
}
pub(super) fn take_saved(
    shell: &mut ShellState,
    visit: &str,
    editor: &super::editor::Editor,
    saved: &domain::Draft,
) -> Result<Option<Continuation>, ()> {
    if pending(shell, visit) {
        let snapshot = editor.snapshot();
        if !snapshot.saved
            || snapshot.failed
            || snapshot.capture.is_some()
            || snapshot.base != *saved
        {
            return Err(());
        }
    }
    Ok(take(shell, visit))
}
pub(super) fn take(shell: &mut ShellState, visit: &str) -> Option<Continuation> {
    let pending = shell
        .canvas_create
        .as_mut()
        .filter(|d| d.visit == visit)?
        .pending_departure
        .take()?;
    pending.origin.current(shell).then_some(Continuation {
        origin: pending.origin,
        destination: pending.destination,
    })
}
impl Continuation {
    pub(super) fn run(self, mut state: Signal<ShellState>) {
        if !self.origin.closed(&state.peek()) {
            return;
        }
        match self.destination {
            Destination::Palette => state.write().open_palette(),
            Destination::Search { conversation } => state.write().open_search(conversation),
            Destination::Settings(section) => {
                let mut s = state.write();
                s.open_preferences();
                s.settings_section = section;
            }
            Destination::SelfMenu => state.write().toggle_self_menu(),
            Destination::Workspace(team) => {
                let index = state.peek().workspaces.iter().position(|w| w.id == team);
                if let Some(index) = index {
                    if state.write().select_workspace(index) {
                        adapter::retain(crate::bootstrap::refresh_selected_channel(state));
                    }
                }
            }
            Destination::Canvas(target) => crate::canvas::open_target(state, target),
            Destination::File(file) => crate::files::open(state, *file),
            Destination::SlackLink(input) => crate::slack_links::open(state, &input),
            Destination::AppHome(target) => crate::app_home::navigation::open(state, target),
            Destination::Later(key) => crate::later::open(state, key),
            Destination::Pinned { channel, message } => crate::pins::open(state, channel, *message),
            Destination::Message { channel, ts } => {
                if state.write().open_search_result(&channel, &ts) {
                    adapter::retain(crate::bootstrap::refresh_selected_channel(state));
                }
            }
            Destination::Activity {
                key,
                channel,
                ts,
                thread,
            } => {
                let navigation = {
                    let mut shell = state.write();
                    // Metadata may refresh during a save; never reinterpret a
                    // retained row key as another channel/message/thread.
                    let same = shell.core.activity.items.iter().any(|i| {
                        i.key == key
                            && i.channel() == channel.as_deref()
                            && i.ts() == ts.as_deref()
                            && i.thread_ts() == thread.as_deref()
                    });
                    if !same {
                        return;
                    }
                    let opened = shell.select_activity_item(
                        key.clone(),
                        channel.as_deref(),
                        ts.as_deref(),
                        thread.as_deref(),
                    );
                    opened
                        .then(|| crate::activity_feed::Navigation::capture(&shell, &key))
                        .flatten()
                };
                if let Some(navigation) = navigation {
                    adapter::retain(navigation.run(state));
                }
            }
            // These entry points are async. Recheck before their first poll,
            // so a newer user action can revoke queued startup authority.
            destination => {
                let origin = self.origin;
                adapter::retain(async move {
                    if !origin.closed(&state.peek()) {
                        return;
                    }
                    match destination {
                        Destination::Surface(target) => {
                            crate::bootstrap::load_main_view(state, target).await
                        }
                        Destination::SignIn => crate::bootstrap::sign_in(state).await,
                        Destination::SwitchAccount(account) => {
                            crate::bootstrap::switch_account(state, account).await
                        }
                        Destination::RemoveAccount(account) => {
                            crate::bootstrap::remove_account(state, account).await
                        }
                        Destination::SearchQuery(query) => {
                            {
                                let mut shell = state.write();
                                shell.replace_search_query(query);
                                shell.overlay = Some(Overlay::Search);
                            }
                            crate::bootstrap::search(state).await;
                        }
                        _ => unreachable!("synchronous destinations consumed above"),
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
