//! Renderer adapter only; each surface retains its own query and mutation authority.
use crate::{
    app_home::inputs::{channels, users as home},
    app_modal::users as modal,
    state::ShellState,
};
use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::block_actions::form::InputKey;

#[derive(Clone)]
pub(super) enum Ticket {
    Modal(modal::Ticket),
    Home(home::Ticket),
    HomeChannel(channels::Ticket),
}
impl Ticket {
    pub(super) fn capture(s: &ShellState, key: &InputKey, is_home: bool) -> Option<Self> {
        if is_home {
            home::Ticket::capture(s, key)
                .map(Self::Home)
                .or_else(|| channels::Ticket::capture(s, key).map(Self::HomeChannel))
        } else {
            modal::Ticket::capture(s, key).map(Self::Modal)
        }
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        match self {
            Self::Modal(t) => t.current(s),
            Self::Home(t) => t.current(s),
            Self::HomeChannel(t) => t.current(s),
        }
    }
    pub(super) fn generation(&self) -> u64 {
        match self {
            Self::Modal(t) => t.generation(),
            Self::Home(t) => t.generation(),
            Self::HomeChannel(t) => t.generation(),
        }
    }
    pub(super) fn after_edit(&self, s: &ShellState) -> Option<Self> {
        match self {
            Self::Modal(_) => self.current(s).then(|| self.clone()),
            Self::Home(t) => t.after_edit(s).map(Self::Home),
            Self::HomeChannel(t) => t.after_edit(s).map(Self::HomeChannel),
        }
    }
}
pub(super) fn display(s: &ShellState, key: &InputKey) -> Option<Vec<(String, String)>> {
    home::display(s, key).or_else(|| channels::display(s, key))
}
pub(super) fn unavailable(s: &ShellState, raw: &Value, is_home: bool) -> Option<&'static str> {
    if is_home {
        if matches!(
            raw["type"].as_str(),
            Some(
                "channels_select"
                    | "multi_channels_select"
                    | "conversations_select"
                    | "multi_conversations_select"
            )
        ) {
            return channels::unavailable(s, raw["type"] != "channels_select").map(|notice| {
                if matches!(raw["type"].as_str(), Some("conversations_select" | "multi_conversations_select")) {
                    "Conversation lookup is unavailable for this Home context. Your selection is retained."
                } else { notice }
            });
        }
        crate::app_home::context::Witness::capture(s)
            .is_err()
            .then_some(
                "People lookup is unavailable for this Home context. Your inputs are retained.",
            )
    } else {
        modal::unavailable(s, raw)
    }
}
pub(super) fn presentation(s: &ShellState, t: &Ticket) -> Option<modal::SearchView> {
    match t {
        Ticket::Modal(t) => modal::presentation(s, t),
        Ticket::Home(t) => home::presentation(s, t),
        Ticket::HomeChannel(t) => channels::presentation(s, t),
    }
}
pub(super) fn labels(s: &ShellState, t: &Ticket, ids: &[String]) -> Vec<(String, String)> {
    match t {
        Ticket::Modal(t) => modal::labels(s, t, ids),
        Ticket::Home(t) => home::labels(s, t, ids),
        Ticket::HomeChannel(t) => channels::labels(s, t, ids),
    }
}
pub(super) fn open(s: Signal<ShellState>, t: Ticket) {
    match t {
        Ticket::Modal(t) => modal::open(s, t),
        Ticket::Home(t) => home::open(s, t),
        Ticket::HomeChannel(t) => channels::open(s, t),
    }
}
pub(super) fn search(s: Signal<ShellState>, t: Ticket, q: String) {
    match t {
        Ticket::Modal(t) => modal::search(s, t, q),
        Ticket::Home(t) => home::search(s, t, q),
        Ticket::HomeChannel(t) => channels::search(s, t, q),
    }
}
pub(super) fn collapse(s: Signal<ShellState>, t: Ticket) {
    match t {
        Ticket::Modal(t) => modal::collapse(s, t),
        Ticket::Home(t) => home::collapse(s, t),
        Ticket::HomeChannel(t) => channels::collapse(s, t),
    }
}
pub(super) fn choose(
    s: Signal<ShellState>,
    t: Ticket,
    serial: u64,
    revision: u64,
    id: String,
) -> bool {
    match t {
        Ticket::Modal(t) => modal::choose(s, t, serial, revision, id),
        Ticket::Home(t) => home::choose(s, t, serial, revision, id),
        Ticket::HomeChannel(t) => channels::choose(s, t, serial, revision, id),
    }
}
pub(super) fn remove(s: Signal<ShellState>, t: Ticket, id: String) -> bool {
    match t {
        Ticket::Modal(t) => modal::remove(s, t, id),
        Ticket::Home(t) => home::remove(s, t, id),
        Ticket::HomeChannel(t) => channels::remove(s, t, id),
    }
}
