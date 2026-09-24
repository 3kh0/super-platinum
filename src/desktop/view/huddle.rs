//! Huddle chrome: the conversation header's huddle control, and the dock at
//! the foot of the list column that carries the live call and any ring.

use dioxus::prelude::*;
use super_platinum_core::huddle::{HuddleCall, HuddlePhase, PendingInvite};
use super_platinum_core::slack::models::Room;
use super_platinum_core::state::Workspace;

use crate::icons::{Icon, icon};
use crate::state::ShellState;

/// How many faces the dock shows before collapsing the rest into a count.
const DOCK_FACES: usize = 6;

/// The header control for the open conversation's huddle: start one, join the
/// one running, or show that this is the call the reader is in.
pub(crate) fn huddle_header_button(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    channel_id: &str,
    room: Option<&Room>,
) -> Element {
    let team = snapshot.core.active_team.clone().unwrap_or_default();
    let in_call = snapshot.core.huddle.is_in(&team, channel_id);
    let count = room.map(|room| room.participants.len()).unwrap_or(0);
    let channel = channel_id.to_owned();
    if in_call {
        return rsx! {
            button {
                class: "huddle-button active",
                title: "Leave huddle",
                "aria-label": "Leave huddle",
                onclick: move |_| crate::huddle::leave(state),
                {icon(Icon::Headphones, "icon sm")}
                span { "In huddle" }
            }
        };
    }
    if room.is_some() {
        return rsx! {
            button {
                class: "huddle-button live",
                title: "Join huddle",
                onclick: move |_| {
                    spawn(crate::huddle::join(state, channel.clone()));
                },
                {icon(Icon::Headphones, "icon sm")}
                if count > 0 {
                    span { "Join · {count}" }
                } else {
                    span { "Join" }
                }
            }
        };
    }
    rsx! {
        button {
            class: "huddle-button idle",
            title: "Start huddle",
            "aria-label": "Start huddle",
            onclick: move |_| {
                spawn(crate::huddle::join(state, channel.clone()));
            },
            {icon(Icon::Headphones, "icon sm")}
        }
    }
}

/// The live call and pending rings, pinned to the bottom of whichever list
/// column the current surface shows. Renders nothing when there is neither.
pub(crate) fn huddle_dock(state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let huddle = &snapshot.core.huddle;
    if huddle.call.is_none() && huddle.invites.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { class: "huddle-dock", "aria-label": "Huddle",
            for pending in huddle.invites.iter() {
                {invite_row(state, snapshot, pending)}
            }
            if let Some(call) = huddle.call.as_ref() {
                {call_panel(state, snapshot, call)}
            }
        }
    }
}

fn invite_row(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    pending: &PendingInvite,
) -> Element {
    let workspace = snapshot.core.workspaces.get(&pending.team);
    let caller = pending
        .invite
        .sender_user_id
        .as_deref()
        .zip(workspace)
        .map(|(user, workspace)| workspace.display_name(user))
        .unwrap_or_else(|| "Someone".into());
    // A 1:1 DM ring names its caller already; anywhere else, say where.
    let place = workspace
        .filter(|workspace| {
            !workspace
                .channels
                .get(&pending.invite.channel_id)
                .is_some_and(|channel| channel.is_im)
        })
        .map(|workspace| conversation_label(workspace, &pending.invite.channel_id))
        .unwrap_or_default();
    let (accept_team, accept_channel) = (pending.team.clone(), pending.invite.channel_id.clone());
    let (decline_team, decline_channel) = (accept_team.clone(), accept_channel.clone());
    rsx! {
        div { class: "huddle-invite", role: "alert",
            div { class: "huddle-invite-copy",
                strong { "{caller}" }
                span { " is inviting you to a huddle" }
                if !place.is_empty() {
                    span { class: "huddle-invite-place", " · {place}" }
                }
            }
            div { class: "huddle-invite-actions",
                button {
                    class: "huddle-text-btn",
                    onclick: move |_| {
                        spawn(crate::huddle::decline_invite(state, decline_team.clone(), decline_channel.clone()));
                    },
                    "Decline"
                }
                button {
                    class: "huddle-text-btn accept",
                    onclick: move |_| {
                        spawn(crate::huddle::accept_invite(state, accept_team.clone(), accept_channel.clone()));
                    },
                    {icon(Icon::Headphones, "icon sm")}
                    "Join"
                }
            }
        }
    }
}

fn call_panel(mut state: Signal<ShellState>, snapshot: &ShellState, call: &HuddleCall) -> Element {
    let workspace = snapshot.core.workspaces.get(&call.team);
    let place = workspace
        .map(|workspace| conversation_label(workspace, &call.channel))
        .unwrap_or_default();
    let status = match call.phase {
        HuddlePhase::Joining | HuddlePhase::Connecting => "Connecting…".to_owned(),
        HuddlePhase::Reconnecting => "Reconnecting…".to_owned(),
        HuddlePhase::Connected => call
            .connected_at
            .map(|at| elapsed_label(at.elapsed().as_secs()))
            .unwrap_or_default(),
    };
    let live = call.phase == HuddlePhase::Connected;
    // Chime's roster is who can actually be heard; before the session is up,
    // Slack's room roster is the best answer to "who is in there".
    let people: Vec<(String, bool, bool)> = if call.participants.is_empty() {
        workspace
            .and_then(|workspace| workspace.active_huddle(&call.channel))
            .map(|room| {
                room.participants
                    .iter()
                    .map(|user| (user.clone(), false, false))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        call.participants
            .iter()
            .map(|row| (row.user.clone(), row.speaking, row.muted))
            .collect()
    };
    let overflow = people.len().saturating_sub(DOCK_FACES);
    let muted = call.muted;
    let channel = call.channel.clone();
    let team = call.team.clone();
    rsx! {
        div { class: if live { "huddle-call live" } else { "huddle-call" },
            div { class: "huddle-call-head",
                button {
                    class: "huddle-call-place",
                    title: "Open conversation",
                    onclick: move |_| {
                        let index = {
                            let shell = state.read();
                            (shell.core.active_team.as_deref() == Some(team.as_str()))
                                .then(|| shell.channels.iter().position(|item| item.id == channel))
                                .flatten()
                        };
                        if let Some(index) = index {
                            state.write().select_channel(index, crate::state::ChannelOpen::Global);
                            spawn(crate::bootstrap::refresh_selected_channel(state));
                        }
                    },
                    {icon(Icon::Headphones, "icon sm")}
                    span { class: "huddle-call-name", "{place}" }
                }
                span { class: "huddle-call-status", "{status}" }
            }
            div { class: "huddle-call-body",
                div { class: "huddle-faces",
                    for (user, speaking, user_muted) in people.iter().take(DOCK_FACES) {
                        {face(snapshot, workspace, user, *speaking, *user_muted)}
                    }
                    if overflow > 0 {
                        span { class: "huddle-face more", "+{overflow}" }
                    }
                }
                div { class: "huddle-controls",
                    button {
                        class: if muted { "huddle-control muted" } else { "huddle-control" },
                        title: if muted { "Unmute microphone" } else { "Mute microphone" },
                        "aria-label": if muted { "Unmute microphone" } else { "Mute microphone" },
                        "aria-pressed": if muted { "true" } else { "false" },
                        onclick: move |_| crate::huddle::toggle_mute(state),
                        if muted {
                            {icon(Icon::MicOff, "icon")}
                        } else {
                            {icon(Icon::Mic, "icon")}
                        }
                    }
                    button {
                        class: "huddle-control leave",
                        title: "Leave huddle",
                        "aria-label": "Leave huddle",
                        onclick: move |_| crate::huddle::leave(state),
                        {icon(Icon::CallEnd, "icon")}
                    }
                }
            }
        }
    }
}

fn face(
    snapshot: &ShellState,
    workspace: Option<&Workspace>,
    user: &str,
    speaking: bool,
    muted: bool,
) -> Element {
    let name = workspace
        .map(|workspace| workspace.display_name(user))
        .unwrap_or_else(|| user.to_owned());
    let initials = name
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    let avatar = workspace
        .and_then(|workspace| workspace.avatar_url(user))
        .map(|url| snapshot.media.register_avatar(user, &url))
        .filter(|avatar| snapshot.media.is_ready(avatar));
    let class = match (speaking, muted) {
        (true, _) => "huddle-face speaking",
        (false, true) => "huddle-face muted",
        _ => "huddle-face",
    };
    let label = if muted {
        format!("{name} (muted)")
    } else {
        name.clone()
    };
    rsx! {
        span { class: "{class}", title: "{label}",
            if let Some(avatar) = avatar {
                img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "{name}" }
            } else {
                "{initials}"
            }
            if muted {
                span { class: "huddle-face-badge", {icon(Icon::MicOff, "")} }
            }
        }
    }
}

/// `#channel` for channels, the person or group for DMs.
fn conversation_label(workspace: &Workspace, channel_id: &str) -> String {
    workspace
        .channels
        .get(channel_id)
        .map(|channel| {
            let name = super_platinum_core::state::channel_display_name(workspace, channel);
            if channel.is_im || channel.is_mpim {
                name
            } else {
                format!("#{name}")
            }
        })
        .unwrap_or_else(|| channel_id.to_owned())
}

fn elapsed_label(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::elapsed_label;

    #[test]
    fn elapsed_reads_like_a_call_timer() {
        assert_eq!(elapsed_label(0), "0:00");
        assert_eq!(elapsed_label(75), "1:15");
        assert_eq!(elapsed_label(3600 + 62), "1:01:02");
    }
}
