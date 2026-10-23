//! Exact closed fixture for selected emoji facts in canonical channel/thread bags.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
pub(crate) mod check;
pub(crate) mod fixture;

pub(crate) fn enabled() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && std::env::var("SUPER_PLATINUM_CHANNEL_THREAD_EMOJI_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-thread-emoji" | "channel-thread-emoji-narrow")
        )
}
pub(crate) fn context_enabled(shell: &ShellState) -> bool {
    fixture::context_enabled(shell)
}
