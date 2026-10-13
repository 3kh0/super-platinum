use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::thread_history::Direction;

pub(super) fn page_control(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    direction: Direction,
) -> Element {
    if snapshot.thread_history.next(direction).is_none() {
        return rsx! {};
    }
    let loading = snapshot.thread_history.loading == Some(direction);
    let failed = snapshot.thread_history.failed == Some(direction);
    let label = match (direction, loading, failed) {
        (_, true, _) => "Loading replies…",
        (Direction::Earlier, _, true) => "Retry earlier replies",
        (Direction::Newer, _, true) => "Retry newer replies",
        (Direction::Earlier, _, _) => "Load earlier replies",
        (Direction::Newer, _, _) => "Load newer replies",
    };
    rsx! {
        div {class:"thread-page-control", "data-thread-page":if direction==Direction::Earlier {"earlier"}else{"newer"},
            button {
                disabled:snapshot.thread_loading || snapshot.thread_history.loading.is_some(),
                onclick:move |_| {spawn(crate::bootstrap::load_thread_page(state,direction));},
                "{label}"
            }
            if failed {span {role:"status","Replies could not load. Try again."}}
        }
    }
}
