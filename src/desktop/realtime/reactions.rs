//! Reactions change message content, not sidebar ordering or channel row metadata.
//! The reducer still updates every canonical copy (including pins and threads).
use crate::state::ShellState;
use super_platinum_core::slack::events::RtEvent;

pub(super) fn channel(event: &RtEvent) -> Option<&str> {
    match event {
        RtEvent::ReactionAdded { channel, .. } | RtEvent::ReactionRemoved { channel, .. } => {
            Some(channel)
        }
        _ => None,
    }
}

pub(super) fn refresh(shell: &mut ShellState, team: &str, generation: u64, channel: &str) {
    if shell.core.active_team.as_deref() != Some(team)
        || shell.core.active_channel.as_deref() != Some(channel)
        || shell
            .core
            .workspaces
            .get(team)
            .is_none_or(|w| w.rt_generation != generation)
    {
        return;
    }
    // A pending navigation or synthetic override has not established the usual
    // canonical row/index contract. Let the ordinary refresh establish it.
    if !shell.channel_window_is_canonical()
        || shell.core.pending_scroll_to.is_some()
        || shell
            .channels
            .get(shell.active_channel)
            .is_none_or(|c| c.id != channel)
    {
        shell.refresh_from_core();
        return;
    }
    // Keep canonical thread synchronization, including its measurement-receipt
    // generation: reaction pills can change row height. Project it first so the
    // channel projector annotates unread dividers on both completed surfaces.
    shell.refresh_thread_from_core();
    shell.reproject_channel_window();
    if shell.messages.len() != shell.timeline_end - shell.timeline_start {
        // The indexed projector fails closed on canonical source-position drift.
        shell.refresh_from_core();
        return;
    }
}

#[cfg(test)]
mod tests;
