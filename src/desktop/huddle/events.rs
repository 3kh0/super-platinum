//! Generation-guarded IPC decoding. Never accepts streams or credentials.
use crate::state::{HuddleCaption, HuddleDevices, HuddleReaction, HuddleRect, ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::huddle::{HuddleVideoTile, MediaEvent};

pub(super) fn media_event(value: &Value) -> Option<(u64, MediaEvent)> {
    let generation = value.get("generation")?.as_u64()?;
    let event = match value.get("type")?.as_str()? {
        "started" => MediaEvent::Started,
        "connecting" => MediaEvent::Connecting {
            reconnecting: value.get("reconnecting").and_then(Value::as_bool) == Some(true),
        },
        "stopped" => MediaEvent::Stopped {
            status: value.get("status")?.as_str()?.to_owned(),
        },
        "camera" => MediaEvent::Camera {
            enabled: value.get("enabled")?.as_bool()?,
            pending: value.get("pending")?.as_bool()?,
        },
        "share" => MediaEvent::ScreenShare {
            enabled: value.get("enabled")?.as_bool()?,
            pending: value.get("pending")?.as_bool()?,
        },
        "tiles" => MediaEvent::Tiles(
            value
                .get("tiles")?
                .as_array()?
                .iter()
                .filter_map(|tile| {
                    Some(HuddleVideoTile {
                        id: u32::try_from(tile.get("id")?.as_u64()?).ok()?,
                        user: tile.get("user").and_then(Value::as_str).map(str::to_owned),
                        local: tile.get("local")?.as_bool()?,
                        content: tile.get("content")?.as_bool()?,
                    })
                })
                .collect(),
        ),
        "muted" => MediaEvent::Muted(value.get("muted")?.as_bool()?),
        "failed" => MediaEvent::Failed(
            value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_owned(),
        ),
        "roster" => MediaEvent::Roster(
            value
                .get("attendees")?
                .as_array()?
                .iter()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    Some((
                        row.first()?.as_str()?.to_owned(),
                        row.get(1)?.as_bool()?,
                        row.get(2)?.as_bool()?,
                    ))
                })
                .collect(),
        ),
        _ => return None,
    };
    Some((generation, event))
}

pub(super) fn ui_event(state: &mut Signal<ShellState>, value: &Value) -> bool {
    let Some(generation) = value.get("generation").and_then(Value::as_u64) else {
        return false;
    };
    if !state.read().core.huddle.is_current(generation)
        || state.read().huddle_ui.generation != generation
    {
        return true;
    }
    let mut shell = state.write();
    match value.get("type").and_then(Value::as_str) {
        Some("capabilities") => {
            shell.huddle_ui.devices.screen_supported =
                value.get("screen_supported").and_then(Value::as_bool) == Some(true);
            shell.huddle_ui.devices.output_supported =
                value.get("output_supported").and_then(Value::as_bool) == Some(true);
        }
        Some("control-error") => shell.show_toast(
            value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("Could not change huddle settings"),
        ),
        Some("devices") => {
            if let Ok(devices) = serde_json::from_value::<HuddleDevices>(value.clone()) {
                shell.huddle_ui.devices = devices;
            }
        }
        Some("device-pending") => {
            shell.huddle_ui.device_pending =
                value.get("pending").and_then(Value::as_bool) == Some(true)
        }
        Some("device-selected") => {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            match value.get("kind").and_then(Value::as_str) {
                Some("microphone") => shell.huddle_ui.microphone = id,
                Some("camera") => shell.huddle_ui.camera = id,
                Some("speaker") => shell.huddle_ui.speaker = id,
                _ => {}
            }
        }
        Some("geometry") => {
            if let Ok(rect) = serde_json::from_value::<HuddleRect>(value["rect"].clone()) {
                if rect.valid() {
                    shell.huddle_ui.rect = Some(rect);
                }
            }
        }
        Some("background") => {
            shell.huddle_ui.background = value
                .get("effect")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .into()
        }
        Some("caption") => {
            if let Ok(caption) = serde_json::from_value::<HuddleCaption>(value.clone()) {
                if shell.huddle_ui.captions_on {
                    shell.huddle_ui.caption(caption);
                }
            }
        }
        Some("reaction") => {
            if let Ok(reaction) = serde_json::from_value::<HuddleReaction>(value.clone()) {
                shell.huddle_ui.reactions.push(reaction);
                let excess = shell.huddle_ui.reactions.len().saturating_sub(5);
                shell.huddle_ui.reactions.drain(..excess);
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn malformed_tiles_are_skipped_and_generation_is_required() {
        assert!(
            media_event(&json!({"type": "camera", "enabled": true, "pending": false})).is_none()
        );
        let (generation, event) = media_event(&json!({
            "type": "tiles", "generation": 4,
            "tiles": [
                {"id": 8, "user": "U1", "local": false, "content": true},
                {"id": -1, "local": false, "content": false},
                {"id": 4294967296_u64, "local": false, "content": false},
                {"id": 9}
            ]
        }))
        .unwrap();
        assert_eq!(generation, 4);
        let MediaEvent::Tiles(tiles) = event else {
            panic!("expected tiles")
        };
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].id, 8);
        assert!(tiles[0].content);
    }
}
