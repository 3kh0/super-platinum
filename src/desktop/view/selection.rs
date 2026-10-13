//! Document-wide selection endings must release either virtual transcript.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionEvent {
    root_id: String,
    scope: String,
    token: u64,
    pinned: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Surface {
    Channel,
    Thread,
}
#[derive(Default)]
struct Owners(HashMap<Surface, (String, u64)>);
impl Owners {
    fn apply(&mut self, shell: &mut ShellState, event: &SelectionEvent) -> Option<Surface> {
        let surface = match event.root_id.as_str() {
            "message-timeline" => Surface::Channel,
            "thread-body" => Surface::Thread,
            _ => return None,
        };
        let scope = match surface {
            Surface::Channel => shell.timeline_scope(),
            Surface::Thread => shell.thread_scope(),
        };
        if scope != event.scope {
            return None;
        }
        let owner = (event.scope.clone(), event.token);
        if event.pinned {
            self.0.insert(surface, owner);
        } else if self.0.get(&surface) == Some(&owner) {
            self.0.remove(&surface);
        } else {
            return None;
        }
        match surface {
            Surface::Channel => {
                if event.pinned {
                    shell.pin_selection();
                } else {
                    shell.unpin_selection();
                }
            }
            Surface::Thread => shell.thread_window.selection_pinned = event.pinned,
        }
        Some(surface)
    }
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    channel_pending: Signal<bool>,
    channel_queued: Signal<bool>,
    thread_pending: Signal<bool>,
    thread_queued: Signal<bool>,
) {
    let mut bridge = document::eval(&format!(
        "{}(event => dioxus.send(event));",
        include_str!("../selection_lifecycle.js")
    ));
    spawn(super::selection_check::run(state));
    let mut owners = Owners::default();
    while let Ok(event) = bridge.recv::<SelectionEvent>().await {
        let changed = owners.apply(&mut state.write(), &event);
        if !event.pinned {
            match changed {
                Some(Surface::Channel) => {
                    super::composer::schedule_timeline_measure(
                        state,
                        channel_pending,
                        channel_queued,
                    );
                    spawn(super::composer::load_older_if_needed(state));
                }
                Some(Surface::Thread) => {
                    super::thread_window::schedule(state, thread_pending, thread_queued)
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_and_workspace_navigation_retire_selection_pin() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.pin_selection();
        let channel = if shell.active_channel == 0 { 1 } else { 0 };
        shell.select_channel(channel, crate::state::ChannelOpen::Global);
        assert!(!shell.selection_pinned);
        shell.pin_selection();
        assert!(shell.select_workspace(shell.active_workspace));
        assert!(!shell.selection_pinned);
    }
    #[test]
    fn old_scope_or_gesture_cannot_release_new_selection() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let mut owners = Owners::default();
        let mut event = SelectionEvent {
            root_id: "message-timeline".into(),
            scope: shell.timeline_scope(),
            token: 1,
            pinned: true,
        };
        owners.apply(&mut shell, &event);
        assert!(shell.selection_pinned);
        let mut old = event.clone();
        event.token = 2;
        owners.apply(&mut shell, &event);
        old.pinned = false;
        assert!(owners.apply(&mut shell, &old).is_none());
        assert!(shell.selection_pinned);
        old = event.clone();
        shell.channel_generation += 1;
        event.scope = shell.timeline_scope();
        event.token = 3;
        owners.apply(&mut shell, &event);
        old.pinned = false;
        assert!(owners.apply(&mut shell, &old).is_none());
        assert!(shell.selection_pinned);
        event.pinned = false;
        owners.apply(&mut shell, &event);
        assert!(!shell.selection_pinned);
    }
}
