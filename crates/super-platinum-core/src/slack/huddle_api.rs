//! Slack huddle ("rooms") API surface.
//!
//! A huddle is two systems. Slack owns the room: who is in it, the channel it
//! belongs to, the `sh_room_*` realtime frames. The audio is an Amazon Chime
//! SDK meeting (`media_backend_type: "free_willy"`), which the client joins
//! directly with the credential pair `rooms.join` returns; no media passes
//! through Slack. Mute is purely a Chime operation — the official client makes
//! no Slack call for it.
//!
//! Method names, arguments, and `_x_reason` tags below are the ones the official
//! web client sends (read from its bundle, build 132707, Chime SDK 3.32.0).
//! Two things it does that this client deliberately does not:
//!
//! - It reuses the `free_willy` pair from a `huddle_invite` frame ("quick
//!   join"). Accepting here always goes through `rooms.join` with the room id.
//! - "End huddle for all" (`huddles.external.end`) is never called. Leaving is
//!   `rooms.leave`, which only ever removes this attendee.

use std::time::Duration;

use serde_json::Value;

use crate::config::WorkspaceSession;

use super::Error;
use super::client::{PreparedRequest, SlackClient};
use super::models::{ChannelId, RoomJoinResponse};
use super::transport::Transport;

/// Chime's own region locator; the official client asks it before starting a
/// huddle and sends the answer as `regions`. Government workspaces use a
/// different host, which this client does not support.
const NEAREST_MEDIA_REGION_URL: &str = "https://nearest-media-region.l.chime.aws";
const REGION_LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);

/// Errors `rooms.leave` answers when this attendee is already gone — the room
/// ended, or the server dropped the seat first. The official client logs them
/// as benign; a leave that finds nothing to leave has still done its job.
const BENIGN_LEAVE_ERRORS: [&str; 4] = [
    "channel_not_found",
    "room_not_found",
    "invalid_channel_id",
    "attendee_not_found",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinArgs {
    pub channel: ChannelId,
    /// The room being answered (`R…`), when joining from an invite.
    pub room: Option<String>,
    /// Media region for a **new** huddle; empty when one is already running,
    /// since the room's region was fixed by whoever started it.
    pub regions: String,
}

/// `rooms.join` — start a huddle in a channel, or join the one running there.
///
/// Registers this user as a visible participant. `multidevice` stays false:
/// true is the official client's explicit "use both devices" choice, and
/// false moves the seat here from any other device instead of doubling it.
pub fn rooms_join(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: JoinArgs,
) -> PreparedRequest {
    let mut fields = vec![("channel_id", args.channel)];
    if let Some(room) = args.room {
        fields.push(("id", room));
    }
    fields.push(("regions", args.regions));
    fields.push(("multidevice", "false".to_owned()));
    fields.push(("_x_reason", "calls-api/joinRoom".to_owned()));
    client.rest_form(workspace, "rooms.join", fields)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveArgs {
    pub channel: ChannelId,
    /// The room id (`R…`) from `call.call_id`.
    pub call: String,
    /// Chime's `Attendee.AttendeeId` for this seat.
    pub attendee: String,
}

/// `rooms.leave` — remove this attendee from the room. Never ends the huddle
/// for anyone else.
pub fn rooms_leave(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: LeaveArgs,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "rooms.leave",
        vec![
            ("channel_id", args.channel),
            ("call_id", args.call),
            ("attendee_id", args.attendee),
            ("reason", "user_initiated".to_owned()),
            ("_x_reason", "calls-api/leaveRoom".to_owned()),
        ],
    )
}

/// `screenhero.rooms.info` — a room's current state. ("Screenhero" is the old
/// name of Slack's calls subsystem; there is no `rooms.info`.)
pub fn rooms_info(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    room: String,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "screenhero.rooms.info",
        vec![
            ("room", room),
            ("_x_reason", "all-calls-store/conditional-fetch".to_owned()),
        ],
    )
}

/// `rooms.inviteResponse` with `response=decline`. Accepting is not a response
/// at all — it is a `rooms.join` naming the room.
pub fn decline_invite_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    room: String,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "rooms.inviteResponse",
        vec![
            ("response", "decline".to_owned()),
            ("channel_id", channel),
            ("room_id", room),
            ("_x_reason", "respond-to-huddle-invite".to_owned()),
        ],
    )
}

pub async fn join(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: JoinArgs,
) -> Result<RoomJoinResponse, Error> {
    let value = transport
        .execute(rooms_join(client, workspace, args))
        .await?;
    serde_json::from_value(value).map_err(|e| Error::Transport(format!("decode rooms.join: {e}")))
}

pub async fn leave(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: LeaveArgs,
) -> Result<(), Error> {
    match transport
        .execute(rooms_leave(client, workspace, args))
        .await
    {
        Ok(_) => Ok(()),
        Err(Error::Api(code)) if BENIGN_LEAVE_ERRORS.contains(&code.as_str()) => Ok(()),
        Err(error) => Err(error),
    }
}

pub async fn decline_invite(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    room: String,
) -> Result<(), Error> {
    transport
        .execute(decline_invite_request(client, workspace, channel, room))
        .await
        .map(|_| ())
}

/// The Chime media region closest to this machine, or `""` when the locator
/// does not answer quickly — Slack then picks one itself, which is exactly
/// what the official client falls back to.
pub async fn nearest_media_region(transport: &Transport, user_agent: &str) -> String {
    let lookup = async {
        let response = transport
            .http()
            .get(NEAREST_MEDIA_REGION_URL)
            .timeout(REGION_LOOKUP_TIMEOUT)
            .header("User-Agent", user_agent)
            .send()
            .await
            .ok()?;
        let body: Value = response.json().await.ok()?;
        region_from_locator(&body)
    };
    lookup.await.unwrap_or_default()
}

fn region_from_locator(body: &Value) -> Option<String> {
    body.get("region")
        .and_then(Value::as_str)
        .filter(|region| {
            !region.is_empty()
                && region
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;

    fn workspace() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T123".into(),
            enterprise_id: Some("E123".into()),
            user_id: "U080A3QP42C".into(),
            name: "Hack Club".into(),
            url: "https://hackclub.slack.com".into(),
            token: "xoxc-test-token".into(),
        }
    }

    fn form_fields(req: &PreparedRequest) -> &Vec<(String, String)> {
        match &req.body {
            RequestBody::Form(fields) => fields,
            other => panic!("expected form body, got {other:?}"),
        }
    }

    fn has(req: &PreparedRequest, key: &str, value: &str) -> bool {
        form_fields(req).contains(&(key.into(), value.into()))
    }

    #[test]
    fn rooms_join_starts_by_channel_with_a_region() {
        let request = rooms_join(
            &SlackClient::default(),
            &workspace(),
            JoinArgs {
                channel: "C123".into(),
                room: None,
                regions: "us-east-1".into(),
            },
        );
        assert!(request.url.contains("/api/rooms.join?"));
        assert!(has(&request, "channel_id", "C123"));
        assert!(has(&request, "regions", "us-east-1"));
        assert!(has(&request, "multidevice", "false"));
        assert!(!form_fields(&request).iter().any(|(key, _)| key == "id"));
        assert!(has(&request, "token", "xoxc-test-token"));
        assert!(!request.redacted_debug().contains("xoxc-test-token"));
    }

    #[test]
    fn rooms_join_answers_an_invite_by_room_id() {
        let request = rooms_join(
            &SlackClient::default(),
            &workspace(),
            JoinArgs {
                channel: "C123".into(),
                room: Some("R9".into()),
                regions: String::new(),
            },
        );
        assert!(has(&request, "id", "R9"));
        assert!(has(&request, "regions", ""));
    }

    #[test]
    fn joining_and_leaving_are_never_retried() {
        let join = rooms_join(
            &SlackClient::default(),
            &workspace(),
            JoinArgs {
                channel: "C1".into(),
                room: None,
                regions: String::new(),
            },
        );
        let leave = rooms_leave(
            &SlackClient::default(),
            &workspace(),
            LeaveArgs {
                channel: "C1".into(),
                call: "R1".into(),
                attendee: "a-1".into(),
            },
        );
        assert!(!join.retry_safe());
        assert!(!leave.retry_safe());
    }

    #[test]
    fn rooms_leave_names_the_seat_it_gives_up() {
        let request = rooms_leave(
            &SlackClient::default(),
            &workspace(),
            LeaveArgs {
                channel: "C123".into(),
                call: "R1".into(),
                attendee: "a-1".into(),
            },
        );
        assert!(request.url.contains("/api/rooms.leave?"));
        assert!(has(&request, "channel_id", "C123"));
        assert!(has(&request, "call_id", "R1"));
        assert!(has(&request, "attendee_id", "a-1"));
        assert!(has(&request, "reason", "user_initiated"));
    }

    #[test]
    fn room_info_uses_the_screenhero_method() {
        let request = rooms_info(&SlackClient::default(), &workspace(), "R123".into());
        assert!(request.url.contains("/api/screenhero.rooms.info?"));
        assert!(has(&request, "room", "R123"));
    }

    #[test]
    fn declining_an_invite_names_channel_and_room() {
        let request = decline_invite_request(
            &SlackClient::default(),
            &workspace(),
            "C1".into(),
            "R1".into(),
        );
        assert!(request.url.contains("/api/rooms.inviteResponse?"));
        assert!(has(&request, "response", "decline"));
        assert!(has(&request, "channel_id", "C1"));
        assert!(has(&request, "room_id", "R1"));
    }

    #[test]
    fn region_locator_accepts_only_region_names() {
        assert_eq!(
            region_from_locator(&serde_json::json!({"region": "us-east-1"})),
            Some("us-east-1".into())
        );
        assert_eq!(
            region_from_locator(&serde_json::json!({"region": ""})),
            None
        );
        assert_eq!(
            region_from_locator(&serde_json::json!({"region": "us east&x=1"})),
            None
        );
        assert_eq!(region_from_locator(&serde_json::json!({})), None);
    }
}
