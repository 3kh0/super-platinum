//! Huddle controls at the shell's serial mutation boundary.
use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use serde_json::json;
use super_platinum_core::{huddle::HuddlePhase, slack::huddle_api};

pub fn toggle_camera(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    let Some(call) = shell.core.huddle.call.as_mut() else {
        return;
    };
    if call.phase != HuddlePhase::Connected || call.camera_pending {
        return;
    }
    call.camera_pending = true;
    super::send(
        json!({ "type": "camera", "generation": call.generation, "enabled": !call.camera_on }),
    );
}

pub fn select_device(state: Signal<ShellState>, kind: &str, id: String) {
    let shell = state.read();
    let Some(call) = shell.core.huddle.call.as_ref() else {
        return;
    };
    if call.phase != HuddlePhase::Connected || shell.huddle_ui.device_pending {
        return;
    }
    super::send(json!({ "type": "device", "generation": call.generation, "kind": kind, "id": id }));
}

pub fn background(state: Signal<ShellState>, effect: String) {
    let shell = state.read();
    let Some(call) = shell.core.huddle.call.as_ref() else {
        return;
    };
    super::send(json!({ "type": "background", "generation": call.generation, "effect": effect }));
}

pub fn reaction(state: Signal<ShellState>, emoji: &str) {
    let shell = state.read();
    let Some(call) = shell.core.huddle.call.as_ref() else {
        return;
    };
    if call.phase != HuddlePhase::Connected {
        return;
    }
    super::send(json!({ "type": "reaction", "generation": call.generation, "emoji": emoji }));
}

pub async fn toggle_captions(mut state: Signal<ShellState>) {
    let Some(call) = state.read().core.huddle.call.clone() else {
        return;
    };
    if call.phase != HuddlePhase::Connected || state.read().huddle_ui.captions_pending {
        return;
    }
    if state.read().huddle_ui.captions_on {
        state.write().huddle_ui.captions_on = false;
        return;
    }
    let Some(room) = call.room else {
        return;
    };
    let Some((transport, client, workspace)) = super::session_for(&state, &call.team) else {
        return;
    };
    state.write().huddle_ui.captions_pending = true;
    let result = transport
        .execute(huddle_api::start_captions_request(
            &client, &workspace, room,
        ))
        .await;
    let mut shell = state.write();
    if !shell.core.huddle.is_current(call.generation) {
        return;
    }
    shell.huddle_ui.captions_pending = false;
    match result {
        Ok(_) => {
            shell.huddle_ui.captions_on = true;
            shell.huddle_ui.visible = true;
        }
        Err(error) => shell.report_failure(&error, "Captions need a connection to Slack", || {
            format!("Could not enable live captions: {error}")
        }),
    }
}

pub async fn invite(mut state: Signal<ShellState>, user: String) {
    let Some(call) = state.read().core.huddle.call.clone() else {
        return;
    };
    if call.phase != HuddlePhase::Connected || state.read().huddle_ui.invite_pending {
        return;
    }
    let Some((transport, client, workspace)) = super::session_for(&state, &call.team) else {
        return;
    };
    state.write().huddle_ui.invite_pending = true;
    let result = transport
        .execute(huddle_api::notify_member_request(
            &client,
            &workspace,
            call.channel,
            user,
        ))
        .await;
    let mut shell = state.write();
    if !shell.core.huddle.is_current(call.generation) {
        return;
    }
    shell.huddle_ui.invite_pending = false;
    match result {
        Ok(_) => {
            shell.huddle_ui.invite = false;
            shell.show_toast("Huddle invitation sent");
        }
        Err(error) => shell.report_failure(
            &error,
            "Inviting someone needs a connection to Slack",
            || format!("Could not send huddle invitation: {error}"),
        ),
    }
}

pub async fn open_huddle_thread(mut state: Signal<ShellState>) {
    let Some(call) = state.read().core.huddle.call.clone() else {
        return;
    };
    let thread = call.thread.or_else(|| {
        let shell = state.read();
        let room = shell
            .core
            .workspaces
            .get(&call.team)?
            .active_huddle(&call.channel)?;
        let root = room
            .extra
            .get("thread_root_ts")
            .or_else(|| room.extra.get("canvas_thread_ts"))?
            .as_str()?;
        Some((call.channel.clone(), root.into()))
    });
    let Some((channel, root)) = thread else {
        state.write().show_toast("This huddle has no thread yet");
        return;
    };
    let missing = state
        .read()
        .core
        .workspaces
        .get(&call.team)
        .is_none_or(|w| !w.channels.contains_key(&channel));
    if missing {
        let Some((transport, client, workspace)) = super::session_for(&state, &call.team) else {
            return;
        };
        let loaded = super_platinum_core::slack::api::fetch_channels_info(
            &transport,
            &client,
            &workspace,
            vec![channel.clone()],
        )
        .await;
        let mut shell = state.write();
        if !shell.core.huddle.is_current(call.generation) {
            return;
        }
        match loaded {
            Ok(channels) => {
                if let Some(workspace) = shell.core.workspaces.get_mut(&call.team) {
                    for channel in channels {
                        workspace.channels.insert(channel.id.clone(), channel);
                    }
                }
            }
            Err(error) => {
                shell.report_failure(
                    &error,
                    "The huddle thread needs a connection to Slack",
                    || format!("Could not load huddle thread: {error}"),
                );
                return;
            }
        }
    }
    {
        let mut shell = state.write();
        if let Some(index) = shell.workspaces.iter().position(|w| w.id == call.team) {
            shell.select_workspace(index);
        }
        // The canvas thread may live in a separate channel. Project that
        // destination so the shared thread composer sends to the right place.
        shell.core.active_channel = Some(channel.clone());
        shell.refresh_from_core();
        let Some(index) = shell.channels.iter().position(|c| c.id == channel) else {
            shell.show_toast("The huddle thread channel is not available yet");
            return;
        };
        shell.select_channel(index, ChannelOpen::Global);
        shell.huddle_ui.expanded = false;
        shell.huddle_ui.visible = false;
    }
    crate::bootstrap::refresh_selected_channel(state).await;
    if !state.read().core.huddle.is_current(call.generation)
        || state.read().core.active_channel.as_deref() != Some(&channel)
    {
        return;
    }
    crate::bootstrap::open_thread(state, channel, root).await;
}
