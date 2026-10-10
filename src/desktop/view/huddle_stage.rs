//! One stage and one set of controls for the app's single live huddle.
use crate::{
    icons::{Icon, icon},
    state::{HuddleDevice, ShellState},
};
use dioxus::prelude::*;
use super_platinum_core::huddle::{HuddleCall, HuddlePhase, HuddleVideoTile};

pub(super) fn controls(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    call: &HuddleCall,
) -> Element {
    let live = call.phase == HuddlePhase::Connected;
    let generation = call.generation;
    let ui = &snapshot.huddle_ui;
    rsx! {
        div { class: "huddle-controls",
            button { class: if call.muted { "huddle-control muted" } else { "huddle-control" },
                title: if call.muted { "Unmute microphone" } else { "Mute microphone" },
                "aria-label": if call.muted { "Unmute microphone" } else { "Mute microphone" },
                "aria-pressed": call.muted.to_string(),
                onclick: move |_| crate::huddle::toggle_mute(state),
                {icon(if call.muted { Icon::MicOff } else { Icon::Mic }, "icon")}
            }
            button { class: if call.camera_on { "huddle-control on" } else { "huddle-control" },
                title: if call.camera_on { "Turn camera off" } else { "Turn camera on" },
                "aria-label": if call.camera_on { "Turn camera off" } else { "Turn camera on" },
                "aria-pressed": call.camera_on.to_string(), "aria-busy": call.camera_pending.to_string(),
                disabled: !live || call.camera_pending || ui.device_pending,
                onclick: move |_| crate::huddle::toggle_camera(state),
                {icon(if call.camera_on { Icon::Videocam } else { Icon::VideocamOff }, "icon")}
            }
            button { class: if call.screen_sharing { "huddle-control on" } else { "huddle-control" },
                title: if call.screen_sharing { "Stop sharing screen" } else { "Share screen" },
                "aria-label": if call.screen_sharing { "Stop sharing screen" } else { "Share screen" },
                "aria-pressed": call.screen_sharing.to_string(), "aria-busy": call.share_pending.to_string(),
                disabled: !live || call.share_pending,
                "data-huddle-share": "true", "data-huddle-generation": "{generation}",
                {icon(Icon::ScreenShare, "icon")}
            }
            button { class: if ui.visible { "huddle-control on" } else { "huddle-control" },
                title: if ui.visible { "Hide huddle window" } else { "Show huddle window" },
                "aria-label": if ui.visible { "Hide huddle window" } else { "Show huddle window" },
                "aria-pressed": ui.visible.to_string(), disabled: !live,
                onclick: move |_| { let visible = state.read().huddle_ui.visible; state.write().huddle_ui.visible = !visible; },
                {icon(Icon::GridView, "icon")}
            }
            button { class: "huddle-control", title: "Huddle settings", "aria-label": "Huddle settings", disabled: !live,
                onclick: move |_| { let mut shell = state.write(); shell.huddle_ui.visible = true; shell.huddle_ui.settings = !shell.huddle_ui.settings; shell.huddle_ui.invite = false; },
                {icon(Icon::Settings, "icon")}
            }
            button { class: "huddle-control leave", title: "Leave huddle", "aria-label": "Leave huddle",
                onclick: move |_| crate::huddle::leave(state), {icon(Icon::CallEnd, "icon")}
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct StageEntry {
    key: String,
    user: Option<String>,
    tile: Option<HuddleVideoTile>,
}

fn entries(call: &HuddleCall, participants: &[String], local: &str) -> Vec<StageEntry> {
    let mut entries: Vec<_> = call
        .tiles
        .iter()
        .filter(|t| t.content)
        .map(|t| StageEntry {
            key: format!("tile-{}", t.id),
            user: t.user.clone(),
            tile: Some(t.clone()),
        })
        .collect();
    let mut placed = Vec::new();
    let mut people = participants.to_vec();
    for participant in &call.participants {
        if !people.contains(&participant.user) {
            people.push(participant.user.clone());
        }
    }
    if !local.is_empty() && !people.iter().any(|p| p == local) {
        people.push(local.into());
    }
    for user in people {
        let tile = call
            .tiles
            .iter()
            .find(|t| !t.content && t.user.as_deref() == Some(&user) && !placed.contains(&t.id));
        if let Some(tile) = tile {
            placed.push(tile.id);
        }
        entries.push(StageEntry {
            key: tile
                .map(|t| format!("tile-{}", t.id))
                .unwrap_or_else(|| format!("photo-{user}")),
            user: Some(user),
            tile: tile.cloned(),
        });
    }
    entries.extend(
        call.tiles
            .iter()
            .filter(|t| !t.content && !placed.contains(&t.id))
            .map(|t| StageEntry {
                key: format!("tile-{}", t.id),
                user: t.user.clone(),
                tile: Some(t.clone()),
            }),
    );
    entries
}

pub(super) fn stage(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let Some(call) = snapshot.core.huddle.call.as_ref() else {
        return rsx! {};
    };
    if !snapshot.huddle_ui.visible || call.phase == HuddlePhase::Joining {
        return rsx! {};
    }
    let workspace = snapshot.core.workspaces.get(&call.team);
    let participants = workspace
        .and_then(|w| w.active_huddle(&call.channel))
        .map(|room| room.participants.as_slice())
        .unwrap_or_default();
    let local = workspace
        .map(|w| w.self_user_id.as_str())
        .unwrap_or_default();
    let entries = entries(call, participants, local);
    let generation = call.generation;
    let ui = &snapshot.huddle_ui;
    let expanded = ui.expanded;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").is_ok();
    let place = workspace
        .map(|w| super::huddle::conversation_label(w, &call.channel))
        .unwrap_or_else(|| "Huddle".into());
    let style = ui
        .rect
        .map(|r| {
            format!(
                "left:{}px;top:{}px;width:{}px;height:{}px",
                r.left, r.top, r.width, r.height
            )
        })
        .unwrap_or_default();
    rsx! {
        section { key: "huddle-stage-{generation}", class: if expanded { "huddle-stage-window expanded" } else { "huddle-stage-window" },
            style: "{style}", "aria-label": "Huddle in {place}",
            "data-huddle-stage": "true", "data-huddle-generation": "{generation}",
            "data-huddle-expanded": expanded.to_string(),
            "data-huddle-fixture": if fixture { Some("true") } else { None },
            onmounted: move |_| { spawn(async move { let _ = document::eval("window.superPlatinumInitializeHuddleStage?.()").await; }); },
            header { class: "huddle-stage-header", "data-huddle-drag": "true",
                {icon(Icon::Headphones, "icon sm")}
                strong { "{place}" }
                span { class: "huddle-stage-status", "{entries.len()} tiles" }
                button { class: "huddle-control", title: if expanded { "Shrink huddle window" } else { "Expand huddle window" },
                    "aria-label": if expanded { "Shrink huddle window" } else { "Expand huddle window" },
                    onclick: move |_| state.write().huddle_ui.expanded = !expanded,
                    {icon(if expanded { Icon::Shrink } else { Icon::Expand }, "icon sm")}
                }
                button { class: "huddle-control", title: "Hide window (call continues)", "aria-label": "Hide huddle window",
                    onclick: move |_| state.write().huddle_ui.visible = false,
                    {icon(Icon::Close, "icon sm")}
                }
            }
            div { class: "huddle-stage-content",
                if ui.settings { {settings(state, snapshot)} }
                else if ui.invite { {invite_panel(state, snapshot, call)} }
                else {
                    div { class: "huddle-stage-grid",
                        for entry in entries.iter() {
                            {stage_tile(snapshot, call, entry, fixture)}
                        }
                    }
                    if ui.captions_on {
                        div { class: "huddle-captions", "aria-label": "Live captions", "aria-live": "polite",
                            if ui.captions.is_empty() { p { "Listening for captions…" } }
                            for caption in ui.captions.iter().rev().take(3).collect::<Vec<_>>().into_iter().rev() {
                                p { key: "{caption.id}",
                                    strong { {caption.user.as_deref().and_then(|user| workspace.map(|w| w.display_name(user))).unwrap_or_else(|| "Speaker".into())} ": " }
                                    span { class: if caption.partial { "partial" } else { "" }, "{caption.text}" }
                                }
                            }
                        }
                    }
                    if !ui.reactions.is_empty() {
                        div { class: "huddle-reactions", "aria-live": "polite",
                            for reaction in &ui.reactions {
                                span {
                                    {reaction.user.as_deref().and_then(|user| workspace.map(|w| w.display_name(user))).unwrap_or_else(|| "Someone".into())} " "
                                    "{super_platinum_core::state::emoji_glyph(&reaction.emoji)}"
                                }
                            }
                            button { title: "Clear reactions", "aria-label": "Clear huddle reactions", onclick: move |_| state.write().huddle_ui.reactions.clear(), {icon(Icon::Close, "icon sm")} }
                        }
                    }
                }
            }
            footer { class: "huddle-stage-footer",
                {controls(state, snapshot, call)}
                div { class: "huddle-stage-extra",
                    button { class: if ui.captions_on { "huddle-control on" } else { "huddle-control" },
                        title: "Live captions", "aria-label": "Toggle live captions", "aria-pressed": ui.captions_on.to_string(),
                        disabled: call.phase != HuddlePhase::Connected || ui.captions_pending,
                        onclick: move |_| { spawn(crate::huddle::toggle_captions(state)); }, {icon(Icon::Captions, "icon")}
                    }
                    button { class: "huddle-control", title: "Invite someone", "aria-label": "Invite someone to huddle",
                        disabled: call.phase != HuddlePhase::Connected,
                        onclick: move |_| { let mut shell = state.write(); shell.huddle_ui.invite = !shell.huddle_ui.invite; shell.huddle_ui.settings = false; }, {icon(Icon::PersonAdd, "icon")}
                    }
                    button { class: "huddle-control", title: "Open huddle thread", "aria-label": "Open huddle thread",
                        onclick: move |_| { spawn(crate::huddle::open_huddle_thread(state)); }, {icon(Icon::Message, "icon")}
                    }
                }
            }
            div { class: "huddle-reaction-picker", "aria-label": "Send a huddle reaction",
                for (emoji, label) in [("thumbsup", "Thumbs up"), ("heart", "Heart"), ("tada", "Celebrate"), ("eyes", "Eyes"), ("raised_hands", "Raised hands")] {
                    button { title: "{label}", "aria-label": "Send {label} reaction", disabled: call.phase != HuddlePhase::Connected,
                        onclick: move |_| crate::huddle::reaction(state, emoji),
                        "{super_platinum_core::state::emoji_glyph(emoji)}"
                    }
                }
            }
            if !expanded {
                for corner in ["top-left", "top-right", "bottom-left", "bottom-right"] {
                    div { class: "huddle-resize {corner}", "data-huddle-resize": corner }
                }
            }
        }
    }
}

fn stage_tile(
    snapshot: &ShellState,
    call: &HuddleCall,
    entry: &StageEntry,
    fixture: bool,
) -> Element {
    let workspace = snapshot.core.workspaces.get(&call.team);
    let name = entry
        .user
        .as_deref()
        .and_then(|u| workspace.map(|w| w.display_name(u)))
        .unwrap_or_else(|| "Participant".into());
    let participant = call
        .participants
        .iter()
        .find(|p| Some(p.user.as_str()) == entry.user.as_deref());
    let speaking = participant.is_some_and(|p| p.speaking);
    let muted = participant.is_some_and(|p| p.muted);
    let content = entry.tile.as_ref().is_some_and(|t| t.content);
    let label = if content {
        format!("{name}'s screen")
    } else {
        name.clone()
    };
    let avatar = entry
        .user
        .as_deref()
        .zip(workspace)
        .and_then(|(u, w)| {
            w.avatar_url(u)
                .map(|url| snapshot.media.register_avatar(u, &url))
        })
        .filter(|id| snapshot.media.is_ready(id));
    let pronouns = entry
        .user
        .as_deref()
        .zip(workspace)
        .and_then(|(u, w)| w.users.get(u))
        .and_then(|u| u.profile.as_ref())
        .and_then(|p| p.pronouns.as_deref())
        .unwrap_or_default();
    rsx! {
        figure { key: "{entry.key}", class: if content { "huddle-stage-tile content" } else if speaking { "huddle-stage-tile speaking" } else { "huddle-stage-tile" },
            if let Some(tile) = &entry.tile {
                video { autoplay: true, muted: true, playsinline: true,
                    class: if tile.local && !tile.content { "mirrored" } else { "" },
                    "aria-label": "{label}", "data-huddle-tile": "{tile.id}", "data-huddle-generation": "{call.generation}",
                    "data-huddle-demo": if fixture { Some(if tile.content { "share" } else { "camera" }) } else { None },
                }
            } else if let Some(avatar) = avatar {
                img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "{name}" }
            } else {
                div { class: "huddle-stage-initials", {name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())} }
            }
            figcaption {
                if content { {icon(Icon::ScreenShare, "icon sm")} }
                if muted && !content { {icon(Icon::MicOff, "icon sm")} }
                if speaking && !content { {icon(Icon::Mic, "icon sm")} }
                span { "{label}" }
                if !pronouns.is_empty() && !content { span { class: "huddle-pronouns", "({pronouns})" } }
            }
        }
    }
}

fn device_select(
    state: Signal<ShellState>,
    kind: &'static str,
    label: &'static str,
    selected: &str,
    devices: &[HuddleDevice],
    disabled: bool,
) -> Element {
    rsx! {
        label { class: "huddle-setting",
            span { "{label}" }
            select { value: "{selected}", disabled,
                onchange: move |event| crate::huddle::select_device(state, kind, event.value()),
                option { value: "", "System default" }
                for device in devices { option { value: "{device.id}", "{device.label}" } }
            }
        }
    }
}
fn settings(state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let ui = &snapshot.huddle_ui;
    rsx! {
        div { class: "huddle-settings huddle-device-settings",
            h2 { "Huddle settings" }
            {device_select(state, "microphone", "Microphone", &ui.microphone, &ui.devices.microphone, ui.device_pending)}
            {device_select(state, "camera", "Camera", &ui.camera, &ui.devices.camera, ui.device_pending)}
            {device_select(state, "speaker", "Speaker", &ui.speaker, &ui.devices.speaker, ui.device_pending || !ui.devices.output_supported)}
            if !ui.devices.output_supported { p { "Speaker output follows your system sound settings." } }
            label { class: "huddle-setting", span { "Camera background" }
                select { value: "{ui.background}", disabled: ui.device_pending,
                    onchange: move |event| crate::huddle::background(state, event.value()),
                    option { value: "none", "None" }
                    option { value: "blur", "Blur" }
                    option { value: "blue", "Blue" }
                    option { value: "gray", "Gray" }
                }
            }
            if ui.device_pending { p { role: "status", "Applying settings…" } }
            if !ui.devices.screen_supported { p { "Screen capture availability depends on your system WebView." } }
        }
    }
}
fn invite_panel(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    call: &HuddleCall,
) -> Element {
    let workspace = snapshot.core.workspaces.get(&call.team);
    let query = snapshot.huddle_ui.invite_query.to_lowercase();
    let mut people = workspace
        .map(|w| {
            w.users
                .values()
                .filter(|u| {
                    !u.deleted
                        && !u.is_bot
                        && u.id != w.self_user_id
                        && !call.participants.iter().any(|p| p.user == u.id)
                })
                .map(|u| (u.id.clone(), w.display_name(&u.id)))
                .filter(|(_, name)| name.to_lowercase().contains(&query))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    people.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    rsx! {
        div { class: "huddle-settings",
            h2 { "Invite to huddle" }
            input { placeholder: "Find a person", "aria-label": "Find a person to invite", value: "{snapshot.huddle_ui.invite_query}", oninput: move |event| state.write().huddle_ui.invite_query = event.value() }
            for (user, name) in people.into_iter().take(30) {
                button { class: "huddle-invite-person", disabled: snapshot.huddle_ui.invite_pending, onclick: move |_| { spawn(crate::huddle::invite(state, user.clone())); },
                    span { "{name}" } {icon(Icon::PersonAdd, "icon sm")}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shares_and_unmatched_cameras_survive_stage_projection() {
        let mut state = super_platinum_core::huddle::HuddleState::default();
        state.begin_join("T1".into(), "C1".into());
        let call = state.call.as_mut().unwrap();
        call.tiles = vec![
            HuddleVideoTile {
                id: 1,
                user: Some("U1".into()),
                local: false,
                content: false,
            },
            HuddleVideoTile {
                id: 2,
                user: Some("U1".into()),
                local: false,
                content: true,
            },
            HuddleVideoTile {
                id: 3,
                user: None,
                local: false,
                content: false,
            },
        ];
        let rows = entries(call, &["U1".into(), "U2".into()], "U0");
        assert_eq!(rows.len(), 5);
        assert!(rows[0].tile.as_ref().unwrap().content);
        assert!(
            rows.iter()
                .any(|r| r.user.as_deref() == Some("U2") && r.tile.is_none())
        );
        assert!(rows.last().unwrap().user.is_none());
    }
}
