//! Huddles: Slack's room calls, with the audio carried by an Amazon Chime SDK
//! session that runs inside the main WebView.
//!
//! Slack owns the seat (`rooms.join` / `rooms.leave`) and the roster the rest
//! of the app shows (`sh_room_*`). Chime owns the media. The renderer-neutral
//! state machine lives in `super_platinum_core::huddle`; this module wires it
//! to Slack calls and to the bridge in `bridge.js`.
//!
//! The SDK (1.1 MB) is served from `/huddle/chime-sdk.js` on the app's own
//! origin, and only loaded the first time a call starts.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use serde_json::{Value, json};
use super_platinum_core::huddle::{MediaEvent, Teardown};
use super_platinum_core::slack::huddle_api::{self, JoinArgs, LeaveArgs};
use tokio::sync::mpsc;

use crate::bootstrap::credentials;
use crate::state::ShellState;

const BRIDGE_JS: &str = include_str!("bridge.js");
const CHIME_SDK: &[u8] = include_bytes!("../../../assets/huddle/chime-sdk.min.js");
/// The asset-handler name: `dioxus://index.html/huddle/…` routes here.
pub const ASSET_ROUTE: &str = "huddle";
/// Slack stops ringing after about half a minute, and does not always send a
/// cancel when it does.
pub const INVITE_RING: Duration = Duration::from_secs(45);

type Commands = (
    mpsc::UnboundedSender<Value>,
    Mutex<Option<mpsc::UnboundedReceiver<Value>>>,
);

/// Commands queue here from the first call on, so one sent before the bridge
/// task has started is delivered rather than lost.
fn commands() -> &'static Commands {
    static COMMANDS: OnceLock<Commands> = OnceLock::new();
    COMMANDS.get_or_init(|| {
        let (sender, receiver) = mpsc::unbounded_channel();
        (sender, Mutex::new(Some(receiver)))
    })
}

fn send(command: Value) {
    // The receiver lives as long as the app; a send can only fail at exit.
    let _ = commands().0.send(command);
}

/// Answers the WebView's request for the Chime SDK.
pub fn serve_asset(
    request: dioxus::desktop::AssetRequest,
    responder: dioxus::desktop::wry::RequestAsyncResponder,
) {
    let (status, body): (u16, &'static [u8]) = if request.uri().path() == "/huddle/chime-sdk.js" {
        (200, CHIME_SDK)
    } else {
        (404, b"")
    };
    let response = dioxus::desktop::wry::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/javascript; charset=utf-8")
        .header("Cache-Control", "no-cache")
        .body(std::borrow::Cow::Borrowed(body))
        .expect("static huddle asset response is valid");
    responder.respond(response);
}

/// Runs the media bridge for the life of the window: forwards queued commands
/// into the WebView and folds Chime events back into the shell state.
pub async fn bridge(mut state: Signal<ShellState>) {
    let Some(mut queue) = commands().1.lock().ok().and_then(|mut slot| slot.take()) else {
        return;
    };
    let mut eval = dioxus::document::eval(BRIDGE_JS);
    enum Next {
        Command(Option<Value>),
        Event(Result<Value, dioxus::document::EvalError>),
    }
    loop {
        let next = tokio::select! {
            command = queue.recv() => Next::Command(command),
            event = eval.recv::<Value>() => Next::Event(event),
        };
        match next {
            Next::Command(Some(command)) => {
                if let Err(error) = eval.send(command) {
                    eprintln!("super-platinum: huddle bridge rejected a command: {error}");
                }
            }
            Next::Command(None) => return,
            Next::Event(Ok(event)) => {
                let Some((generation, event)) = media_event(&event) else {
                    continue;
                };
                let teardown = state.write().core.huddle.apply_media(generation, event);
                if let Some(teardown) = teardown {
                    dioxus::prelude::spawn(finish(state, teardown));
                }
            }
            Next::Event(Err(error)) => {
                eprintln!("super-platinum: huddle bridge stopped: {error}");
                return;
            }
        }
    }
}

fn media_event(value: &Value) -> Option<(u64, MediaEvent)> {
    let generation = value.get("generation")?.as_u64()?;
    let event = match value.get("type")?.as_str()? {
        "started" => MediaEvent::Started,
        "connecting" => MediaEvent::Connecting {
            reconnecting: value.get("reconnecting").and_then(Value::as_bool) == Some(true),
        },
        "stopped" => MediaEvent::Stopped {
            status: value.get("status")?.as_str()?.to_owned(),
        },
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

/// Starts or joins the huddle in `channel` on the active workspace.
pub async fn join(state: Signal<ShellState>, channel: String) {
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    join_in(state, team, channel, None).await;
}

/// Answers a ring by joining the room it named.
pub async fn accept_invite(mut state: Signal<ShellState>, team: String, channel: String) {
    let Some(pending) = state.write().core.huddle.take_invite(&team, &channel) else {
        return;
    };
    join_in(state, team, channel, Some(pending.invite.call_id)).await;
}

pub async fn decline_invite(mut state: Signal<ShellState>, team: String, channel: String) {
    let Some(pending) = state.write().core.huddle.take_invite(&team, &channel) else {
        return;
    };
    let Some((transport, client, workspace)) = session_for(&state, &team) else {
        return;
    };
    if let Err(error) = huddle_api::decline_invite(
        &transport,
        &client,
        &workspace,
        channel,
        pending.invite.call_id,
    )
    .await
    {
        // The caller sees the ring time out instead; nothing for the reader to do.
        eprintln!("super-platinum: declining huddle invite failed: {error}");
    }
}

async fn join_in(
    mut state: Signal<ShellState>,
    team: String,
    channel: String,
    room: Option<String>,
) {
    let Some((transport, client, workspace)) = session_for(&state, &team) else {
        state
            .write()
            .show_toast("Huddles need a connection to Slack");
        return;
    };
    let begun = state
        .write()
        .core
        .huddle
        .begin_join(team.clone(), channel.clone());
    let Some((generation, replaced)) = begun else {
        return;
    };
    if let Some(replaced) = replaced {
        dioxus::prelude::spawn(finish(state, replaced));
    }

    // Only a new huddle picks a region: a running one was placed by whoever
    // started it, and the official client sends "" to join it.
    let existing = room.is_some()
        || state
            .read()
            .core
            .workspaces
            .get(&team)
            .is_some_and(|workspace| workspace.active_huddle(&channel).is_some());
    let regions = if existing {
        String::new()
    } else {
        media_region(&mut state, &transport).await
    };

    let joined = huddle_api::join(
        &transport,
        &client,
        &workspace,
        JoinArgs {
            channel: channel.clone(),
            room,
            regions,
        },
    )
    .await;
    let response = match joined {
        Ok(response) => response,
        Err(error) => {
            let mut shell = state.write();
            shell.core.huddle.join_failed(generation);
            shell.report_failure(&error, "Huddles need a connection to Slack", || {
                format!("Could not join huddle: {error}")
            });
            return;
        }
    };
    let call = response.call.call_id.clone();
    let credentials = response.call.free_willy.clone();
    let Some(attendee) = credentials
        .as_ref()
        .and_then(|credentials| credentials.attendee_id())
        .map(str::to_owned)
    else {
        let mut shell = state.write();
        shell.core.huddle.join_failed(generation);
        shell.show_toast("This huddle uses a call type Super Platinum cannot join");
        return;
    };
    let credentials = credentials.expect("attendee id implies credentials");

    let mut shell = state.write();
    if !shell
        .core
        .huddle
        .joined(generation, call.clone(), attendee.clone())
    {
        // The reader hung up or moved to another huddle while Slack was
        // answering: give the seat straight back rather than sit in a room
        // nobody here is listening to.
        drop(shell);
        let _ = huddle_api::leave(
            &transport,
            &client,
            &workspace,
            LeaveArgs {
                channel,
                call,
                attendee,
            },
        )
        .await;
        return;
    }
    if let Some(room) = response.huddle
        && let Some(workspace) = shell.core.workspaces.get_mut(&team)
    {
        workspace.apply_room(room);
    }
    let muted = shell
        .core
        .huddle
        .call
        .as_ref()
        .is_some_and(|call| call.muted);
    drop(shell);

    let mut command = credentials.bridge_payload();
    command["type"] = json!("join");
    command["generation"] = json!(generation);
    command["muted"] = json!(muted);
    send(command);
}

/// Hangs up the current call.
pub fn leave(mut state: Signal<ShellState>) {
    let teardown = state.write().core.huddle.leave();
    if let Some(teardown) = teardown {
        dioxus::prelude::spawn(finish(state, teardown));
    }
}

pub fn toggle_mute(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    let Some(muted) = shell.core.huddle.toggle_mute() else {
        return;
    };
    let generation = shell.core.huddle.call.as_ref().map(|call| call.generation);
    drop(shell);
    send(json!({ "type": "mute", "generation": generation, "muted": muted }));
}

/// Tears a call down on both sides: stops its media session, gives the seat
/// back to Slack, and says why when the call ended on its own.
pub async fn finish(mut state: Signal<ShellState>, teardown: Teardown) {
    let Teardown { call, reason } = teardown;
    // Named by generation, so it can never stop a call started after it.
    send(json!({ "type": "leave", "generation": call.generation }));
    if let Some(reason) = reason {
        state.write().show_toast(reason.message());
    }
    let (Some(room), Some(attendee)) = (call.room, call.attendee) else {
        return;
    };
    let Some((transport, client, workspace)) = session_for(&state, &call.team) else {
        return;
    };
    if let Err(error) = huddle_api::leave(
        &transport,
        &client,
        &workspace,
        LeaveArgs {
            channel: call.channel,
            call: room,
            attendee,
        },
    )
    .await
    {
        // The media session is already closed, and Slack drops a seat whose
        // media went away on its own; this only makes it prompt.
        eprintln!("super-platinum: rooms.leave failed: {error}");
    }
}

async fn media_region(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
) -> String {
    if let Some(region) = state.read().core.huddle.region.clone() {
        return region;
    }
    let user_agent = super_platinum_core::slack::xparams::Identity::from_capture().user_agent;
    let region = huddle_api::nearest_media_region(transport, &user_agent).await;
    if !region.is_empty() {
        state.write().core.huddle.region = Some(region.clone());
    }
    region
}

fn session_for(
    state: &Signal<ShellState>,
    team: &str,
) -> Option<(
    std::sync::Arc<super_platinum_core::slack::Transport>,
    super_platinum_core::slack::SlackClient,
    super_platinum_core::config::WorkspaceSession,
)> {
    let (transport, client, workspaces) = credentials(state)?;
    let workspace = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)?;
    Some((transport, client, workspace))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_events_translate_to_media_events() {
        assert_eq!(
            media_event(&json!({"generation": 3, "type": "started"})),
            Some((3, MediaEvent::Started))
        );
        assert_eq!(
            media_event(&json!({"generation": 3, "type": "stopped", "status": "MeetingEnded"})),
            Some((
                3,
                MediaEvent::Stopped {
                    status: "MeetingEnded".into()
                }
            ))
        );
        assert_eq!(
            media_event(
                &json!({"generation": 1, "type": "roster", "attendees": [["T-R-U1", true, false], ["bad"]]})
            ),
            Some((1, MediaEvent::Roster(vec![("T-R-U1".into(), true, false)])))
        );
        assert_eq!(media_event(&json!({"type": "started"})), None);
        assert_eq!(media_event(&json!({"generation": 1, "type": "nope"})), None);
    }

    #[test]
    fn the_sdk_asset_is_the_vendored_bundle() {
        let text = std::str::from_utf8(CHIME_SDK).expect("bundle is utf-8");
        assert!(text.contains("amazon-chime-sdk-js"));
        assert!(text.contains("var ChimeSDK="));
    }
}
