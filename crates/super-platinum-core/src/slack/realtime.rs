use std::time::Duration;

use serde_json::Value;
use tokio::sync::{mpsc, watch};

use super::transport::Health;

use super::events::{RawEvent, RtEvent};
use super::models::{Message as SlackMessage, TeamId};

/// How often the client pings flannel.
const PING_EVERY: Duration = Duration::from_secs(15);

/// How long the socket may stay silent before it is declared dead. Three ping
/// rounds: long enough that a slow network is not mistaken for a dropped one.
const SILENCE_LIMIT: Duration = Duration::from_secs(50);

#[derive(Debug, Clone)]
pub enum RtUpdate {
    Connected {
        generation: u64,
        connection: Connection,
    },
    Event {
        generation: u64,
        // Boxed: `RtEvent` is ~1KB, which would otherwise set the size of every
        // `RtUpdate` sent over the realtime channel, including keepalives.
        event: Box<RtEvent>,
    },
    Disconnected {
        generation: u64,
    },
}

#[derive(Debug, Clone)]
pub struct Connection {
    tx: mpsc::Sender<String>,
}

impl Connection {
    pub fn send(&self, frame: String) {
        let _ = self.tx.try_send(frame);
    }

    pub fn from_sender(tx: mpsc::Sender<String>) -> Self {
        Self { tx }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectParams {
    pub team: TeamId,
    pub ws_url: String,
    pub d_cookie: String,
    pub user_agent: String,
}

pub fn flannel_url(token: &str, team_id: &str) -> String {
    format!("wss://wss-primary.slack.com/?token={token}&flannel=3&gateway_server={team_id}-1")
}

pub fn user_typing_frame(channel: &str) -> String {
    format!(
        r#"{{"type":"user_typing","channel":{}}}"#,
        serde_json::json!(channel)
    )
}

pub fn presence_query_frame(ids: &[String]) -> String {
    serde_json::json!({ "type": "presence_query", "ids": ids }).to_string()
}

/// Standing presence subscription. Captured from the real client over CDP: the
/// first frame it sends after a connect is `presence_sub` for its own user id,
/// and flannel answers with a `presence_change` naming the ids under `users`.
/// Unlike `presence_query` this keeps pushing, so the badge stays live.
pub fn presence_sub_frame(ids: &[String]) -> String {
    serde_json::json!({ "type": "presence_sub", "ids": ids }).to_string()
}

/// Spawn a renderer-independent realtime supervisor.
///
/// The receiver closes when its consumer is dropped. Each connection attempt
/// receives a monotonically increasing generation so reducers can reject stale
/// events after a reconnect.
pub fn connect(
    params: ConnectParams,
    health: Option<watch::Receiver<Health>>,
) -> mpsc::Receiver<(TeamId, RtUpdate)> {
    let (output, receiver) = mpsc::channel(64);
    tokio::spawn(run_supervisor(params, health, output));
    receiver
}

async fn run_supervisor(
    params: ConnectParams,
    mut health: Option<watch::Receiver<Health>>,
    output: mpsc::Sender<(TeamId, RtUpdate)>,
) {
    let mut backoff = Duration::from_secs(1);
    let mut generation = 0;
    loop {
        // Never dial into a link the shell has already declared gone. Waiting
        // for it to be confirmed back means the socket returns as soon as the
        // network does, rather than sitting out a backoff that grew to half a
        // minute while nothing could possibly connect.
        if let Some(health) = health.as_mut()
            && *health.borrow_and_update() == Health::Offline
        {
            let _ = health.wait_for(|health| *health != Health::Offline).await;
            backoff = Duration::from_secs(1);
        }
        generation += 1;
        match run_connection(&params, generation, &health, &output).await {
            Ok(()) => backoff = Duration::from_secs(1),
            Err(e) => {
                tracing::warn!(team = %params.team, error = %e, "flannel connection ended");
            }
        }
        if output
            .send((params.team.clone(), RtUpdate::Disconnected { generation }))
            .await
            .is_err()
        {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn run_connection(
    params: &ConnectParams,
    generation: u64,
    health: &Option<watch::Receiver<Health>>,
    output: &mpsc::Sender<(TeamId, RtUpdate)>,
) -> Result<(), String> {
    let http = wreq::Client::builder()
        .emulation(wreq_util::Emulation::Chrome140)
        .build()
        .map_err(|e| format!("client build: {e}"))?;

    let response = http
        .websocket(&params.ws_url)
        .header("User-Agent", params.user_agent.as_str())
        .header("Cookie", format!("d={}", params.d_cookie))
        .send()
        .await
        .map_err(|e| format!("ws handshake: {e}"))?;
    let mut socket = response
        .into_websocket()
        .await
        .map_err(|e| format!("ws upgrade: {e}"))?;

    let (tx, mut rx) = mpsc::channel::<String>(64);
    if output
        .send((
            params.team.clone(),
            RtUpdate::Connected {
                generation,
                connection: Connection { tx },
            },
        ))
        .await
        .is_err()
    {
        return Ok(());
    }

    let mut ping_id: u64 = 0;
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // A link that goes away under an established socket does not error: writes
    // keep landing in the kernel buffer and reads simply never return. Slack
    // answers every ping, so silence past a few of them is the only honest
    // evidence the connection is gone.
    let mut last_seen = tokio::time::Instant::now();
    // The shell notices a dropped link in seconds; the socket on its own would
    // take the silence limit to work it out. Watching that verdict is what
    // turns a minute of believing in a dead connection into a few seconds.
    let mut link = health.clone();

    loop {
        tokio::select! {
            () = link_dropped(&mut link) => return Err("link dropped".into()),
            incoming = socket.recv() => match incoming {
                Some(Ok(wreq::ws::message::Message::Text(text))) => {
                    last_seen = tokio::time::Instant::now();
                    trace_frame(text.as_str());
                    if let Some(event) = parse_event(text.as_str())
                        && output
                            .send((
                                params.team.clone(),
                                RtUpdate::Event {
                                    generation,
                                    event: Box::new(event),
                                },
                            ))
                            .await
                            .is_err()
                    {
                        return Ok(());
                    }
                }
                Some(Ok(wreq::ws::message::Message::Close(_))) | None => return Ok(()),
                Some(Ok(_)) => last_seen = tokio::time::Instant::now(),
                Some(Err(e)) => return Err(format!("recv: {e}")),
            },
            outbound = rx.recv() => match outbound {
                Some(frame) => {
                    if let Err(e) = socket.send(wreq::ws::message::Message::text(frame)).await {
                        return Err(format!("send: {e}"));
                    }
                }
                None => return Ok(()),
            },
            _ = ping.tick() => {
                if last_seen.elapsed() >= SILENCE_LIMIT {
                    return Err(format!(
                        "no frame in {}s; treating the socket as dead",
                        SILENCE_LIMIT.as_secs()
                    ));
                }
                ping_id += 1;
                let frame = format!(r#"{{"type":"ping","id":{ping_id}}}"#);
                if let Err(e) = socket.send(wreq::ws::message::Message::text(frame)).await {
                    return Err(format!("ping: {e}"));
                }
            }
        }
    }
}

/// Prints the shape of every frame the socket delivers when
/// `SUPER_PLATINUM_RT_TRACE` is set: the event type, and the channel and
/// timestamp it names. Only structural fields — never message text, tokens, or
/// cookies — so the trace can be pasted into a bug report as-is.
fn trace_frame(text: &str) {
    static TRACING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !TRACING.get_or_init(|| std::env::var_os("SUPER_PLATINUM_RT_TRACE").is_some()) {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        eprintln!("rt: unparsable frame ({} bytes)", text.len());
        return;
    };
    let field = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_owned()
    };
    let kind = field(&value, "type");
    let item = value.get("item").unwrap_or(&value);
    eprintln!(
        "rt: {kind} channel={} ts={} subtype={} parsed={}",
        field(item, "channel"),
        field(item, "ts"),
        field(&value, "subtype"),
        parse_event(text).is_some(),
    );
}

/// Resolves when the shell declares the link gone, or never when nothing is
/// watching it.
async fn link_dropped(health: &mut Option<watch::Receiver<Health>>) {
    match health {
        Some(health) => {
            let _ = health.wait_for(|health| *health == Health::Offline).await;
        }
        None => std::future::pending().await,
    }
}

pub fn parse_event(text: &str) -> Option<RtEvent> {
    let value: Value = serde_json::from_str(text).ok()?;
    let kind = value.get("type").and_then(Value::as_str)?;
    match kind {
        "message" => {
            let channel = value.get("channel").and_then(Value::as_str)?.to_owned();
            match value.get("subtype").and_then(Value::as_str) {
                Some("message_changed") => {
                    let nested = value.get("message")?.clone();
                    let mut message: SlackMessage = serde_json::from_value(nested).ok()?;
                    message.channel.get_or_insert(channel.clone());
                    Some(RtEvent::MessageChanged { channel, message })
                }
                Some("message_replied") => {
                    let nested = value.get("message")?.clone();
                    let mut message: SlackMessage = serde_json::from_value(nested).ok()?;
                    message.channel.get_or_insert(channel);
                    Some(RtEvent::Message(message))
                }
                Some("message_deleted") => {
                    let deleted_ts = value.get("deleted_ts").and_then(Value::as_str)?.to_owned();
                    Some(RtEvent::MessageDeleted {
                        channel,
                        deleted_ts,
                    })
                }
                _ => {
                    let mut message: SlackMessage = serde_json::from_value(value).ok()?;
                    message.channel.get_or_insert(channel);
                    Some(RtEvent::Message(message))
                }
            }
        }
        "user_typing" => Some(RtEvent::UserTyping {
            channel: value.get("channel").and_then(Value::as_str)?.to_owned(),
            user: value.get("user").and_then(Value::as_str)?.to_owned(),
        }),
        "presence_change" => {
            let mut users: Vec<String> = value
                .get("users")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            if let Some(user) = value.get("user").and_then(Value::as_str) {
                users.push(user.to_owned());
            }
            if users.is_empty() {
                return None;
            }
            Some(RtEvent::PresenceChange {
                users,
                presence: value
                    .get("presence")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
        }
        "user_change" => serde_json::from_value(value.get("user")?.clone())
            .ok()
            .map(RtEvent::UserChanged),
        "dnd_updated" | "dnd_updated_user" => Some(RtEvent::DndUpdated {
            user: value.get("user").and_then(Value::as_str)?.to_owned(),
            dnd: value
                .get("dnd_status")
                .cloned()
                .and_then(|status| serde_json::from_value(status).ok())
                .unwrap_or_default(),
        }),
        "reaction_added" => parse_reaction_event(value, true),
        "reaction_removed" => parse_reaction_event(value, false),
        "channel_marked" | "im_marked" | "group_marked" | "mpim_marked" => {
            Some(RtEvent::ChannelMarked {
                channel: value.get("channel").and_then(Value::as_str)?.to_owned(),
                ts: value.get("ts").and_then(Value::as_str)?.to_owned(),
                unread_count: value
                    .get("unread_count_display")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32),
                mention_count: value
                    .get("mention_count_display")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32),
            })
        }
        "activity" => {
            let entry = value.get("entry")?.clone();
            let item: super::models::ActivityItem = serde_json::from_value(entry).ok()?;
            Some(RtEvent::ActivityUpdated(item))
        }
        "sh_room_join" | "sh_room_leave" | "sh_room_update" => {
            let room: super::models::Room =
                serde_json::from_value(value.get("room")?.clone()).ok()?;
            let user = value
                .get("user")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            Some(match kind {
                "sh_room_join" => RtEvent::RoomJoin { room, user },
                "sh_room_leave" => RtEvent::RoomLeave { room, user },
                _ => RtEvent::RoomUpdate { room },
            })
        }
        _ => {
            let raw: RawEvent = serde_json::from_value(value).ok()?;
            Some(RtEvent::Unknown(raw))
        }
    }
}

fn parse_reaction_event(value: Value, added: bool) -> Option<RtEvent> {
    let item = value.get("item")?;
    if item.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let channel = item.get("channel").and_then(Value::as_str)?.to_owned();
    let ts = item.get("ts").and_then(Value::as_str)?.to_owned();
    let user = value
        .get("user")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let reaction = value
        .get("reaction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if reaction.is_empty() {
        return None;
    }
    Some(if added {
        RtEvent::ReactionAdded {
            channel,
            ts,
            user,
            reaction,
        }
    } else {
        RtEvent::ReactionRemoved {
            channel,
            ts,
            user,
            reaction,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_message() {
        let frame = r#"{"type":"message","channel":"C1","user":"U1","ts":"1.2","text":"hi"}"#;
        match parse_event(frame) {
            Some(RtEvent::Message(m)) => {
                assert_eq!(m.channel.as_deref(), Some("C1"));
                assert_eq!(m.text.as_deref(), Some("hi"));
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn parses_message_changed_and_deleted() {
        let changed = r#"{"type":"message","subtype":"message_changed","channel":"C1","message":{"ts":"1.2","text":"edited"}}"#;
        match parse_event(changed) {
            Some(RtEvent::MessageChanged { channel, message }) => {
                assert_eq!(channel, "C1");
                assert_eq!(message.text.as_deref(), Some("edited"));
                assert_eq!(message.channel.as_deref(), Some("C1"));
            }
            other => panic!("expected MessageChanged, got {other:?}"),
        }

        let deleted =
            r#"{"type":"message","subtype":"message_deleted","channel":"C1","deleted_ts":"1.2"}"#;
        match parse_event(deleted) {
            Some(RtEvent::MessageDeleted {
                channel,
                deleted_ts,
            }) => {
                assert_eq!(channel, "C1");
                assert_eq!(deleted_ts, "1.2");
            }
            other => panic!("expected MessageDeleted, got {other:?}"),
        }
    }

    #[test]
    fn parses_message_replied_as_nested_message() {
        let replied = r#"{"type":"message","subtype":"message_replied","channel":"C1","message":{"type":"message","user":"U1","text":"actual","ts":"1.2"}}"#;
        match parse_event(replied) {
            Some(RtEvent::Message(message)) => {
                assert_eq!(message.user.as_deref(), Some("U1"));
                assert_eq!(message.text.as_deref(), Some("actual"));
                assert_eq!(message.channel.as_deref(), Some("C1"));
                assert_ne!(message.subtype.as_deref(), Some("message_replied"));
            }
            other => panic!("expected nested Message, got {other:?}"),
        }
    }

    #[test]
    fn parses_typing() {
        let frame = r#"{"type":"user_typing","channel":"C1","user":"U9"}"#;
        assert!(matches!(
            parse_event(frame),
            Some(RtEvent::UserTyping { .. })
        ));
    }

    #[test]
    fn parses_user_change() {
        let frame = r#"{"type":"user_change","user":{"id":"U1","profile":{"status_text":"Reviewing","status_emoji":":ship:"}}}"#;
        match parse_event(frame) {
            Some(RtEvent::UserChanged(user)) => {
                assert_eq!(user.id, "U1");
                let profile = user.profile.expect("profile");
                assert_eq!(profile.status_text.as_deref(), Some("Reviewing"));
                assert_eq!(profile.status_emoji.as_deref(), Some(":ship:"));
            }
            other => panic!("expected UserChanged, got {other:?}"),
        }
    }

    #[test]
    fn parses_batched_and_single_presence_change() {
        // Flannel answers a `presence_query` with a batched `users` array; the
        // classic single-`user` shape still shows up on some frames.
        let batched = r#"{"type":"presence_change","users":["U1","U2"],"presence":"active"}"#;
        match parse_event(batched) {
            Some(RtEvent::PresenceChange { users, presence }) => {
                assert_eq!(users, vec!["U1".to_owned(), "U2".to_owned()]);
                assert_eq!(presence, "active");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        let single = r#"{"type":"presence_change","user":"U3","presence":"away"}"#;
        match parse_event(single) {
            Some(RtEvent::PresenceChange { users, presence }) => {
                assert_eq!(users, vec!["U3".to_owned()]);
                assert_eq!(presence, "away");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        let empty = r#"{"type":"presence_change","presence":"away"}"#;
        assert!(parse_event(empty).is_none());
    }

    #[test]
    fn parses_dnd_updated_for_self_and_others() {
        let own = r#"{"type":"dnd_updated","user":"U0","dnd_status":{"dnd_enabled":true,"next_dnd_start_ts":10,"next_dnd_end_ts":20,"snooze_enabled":true,"snooze_endtime":99}}"#;
        match parse_event(own) {
            Some(RtEvent::DndUpdated { user, dnd }) => {
                assert_eq!(user, "U0");
                assert!(dnd.snooze_enabled);
                assert_eq!(dnd.snooze_endtime, Some(99));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        // Other members arrive without the snooze half of the payload.
        let other_user =
            r#"{"type":"dnd_updated_user","user":"U1","dnd_status":{"dnd_enabled":false}}"#;
        match parse_event(other_user) {
            Some(RtEvent::DndUpdated { user, dnd }) => {
                assert_eq!(user, "U1");
                assert!(!dnd.dnd_enabled);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_reaction_events_for_messages() {
        let added = r#"{"type":"reaction_added","user":"U1","reaction":"thumbsup","item":{"type":"message","channel":"C1","ts":"1.2"}}"#;
        match parse_event(added) {
            Some(RtEvent::ReactionAdded {
                channel,
                ts,
                user,
                reaction,
            }) => {
                assert_eq!(channel, "C1");
                assert_eq!(ts, "1.2");
                assert_eq!(user, "U1");
                assert_eq!(reaction, "thumbsup");
            }
            other => panic!("expected ReactionAdded, got {other:?}"),
        }

        let removed = r#"{"type":"reaction_removed","user":"U1","reaction":"eyes","item":{"type":"message","channel":"C1","ts":"1.2"}}"#;
        assert!(matches!(
            parse_event(removed),
            Some(RtEvent::ReactionRemoved { reaction, .. }) if reaction == "eyes"
        ));
    }

    #[test]
    fn parses_activity_updated_entry() {
        let frame = r#"{"type":"activity","subtype":"activity_updated","key":"dm-D1","entry":{"is_unread":true,"feed_ts":"1783834090.99","key":"dm-D1","item":{"type":"dm","bundle_info":{"payload":{"dm_entry":{"latest_message":{"ts":"1783834090.38","channel":"D1"}}}}}}}"#;
        match parse_event(frame) {
            Some(RtEvent::ActivityUpdated(item)) => {
                assert_eq!(item.key, "dm-D1");
                assert!(item.is_unread);
                assert_eq!(item.channel(), Some("D1"));
                assert_eq!(item.ts(), Some("1783834090.38"));
            }
            other => panic!("expected ActivityUpdated, got {other:?}"),
        }
    }

    #[test]
    fn parses_mark_events_for_all_conversation_kinds() {
        let frame = r#"{"type":"im_marked","channel":"D1","ts":"1783834090.000100","dm_count":3,"unread_count_display":0,"mention_count_display":0}"#;
        match parse_event(frame) {
            Some(RtEvent::ChannelMarked {
                channel,
                ts,
                unread_count,
                mention_count,
            }) => {
                assert_eq!(channel, "D1");
                assert_eq!(ts, "1783834090.000100");
                assert_eq!(unread_count, Some(0));
                assert_eq!(mention_count, Some(0));
            }
            other => panic!("expected ChannelMarked, got {other:?}"),
        }
        for kind in ["channel_marked", "group_marked", "mpim_marked"] {
            let frame = format!(r#"{{"type":"{kind}","channel":"C1","ts":"1.000001"}}"#);
            assert!(
                matches!(parse_event(&frame), Some(RtEvent::ChannelMarked { .. })),
                "{kind} should parse as ChannelMarked"
            );
        }
    }

    #[test]
    fn parses_room_join_and_leave_and_update() {
        let join = r#"{"type":"sh_room_join","user":"U08TBE25U82","huddle":{"channel_id":"C0P5NE354"},"room":{"id":"R0BHHSL2011","call_family":"huddle","channels":["C0P5NE354"],"created_by":"U08TBE25U82","has_ended":false,"huddle_link":"https://app.slack.com/huddle/E09/C0P5NE354","participants":["U08TBE25U82"],"media_backend_type":"free_willy"}}"#;
        match parse_event(join) {
            Some(RtEvent::RoomJoin { room, user }) => {
                assert_eq!(room.id, "R0BHHSL2011");
                assert_eq!(room.channel().map(String::as_str), Some("C0P5NE354"));
                assert_eq!(room.participants, vec!["U08TBE25U82".to_owned()]);
                assert!(room.is_active());
                assert_eq!(user, "U08TBE25U82");
            }
            other => panic!("expected RoomJoin, got {other:?}"),
        }

        let leave = r#"{"type":"sh_room_leave","user":"U1","room":{"id":"R1","channels":["C1"],"participants":[],"has_ended":false}}"#;
        assert!(matches!(
            parse_event(leave),
            Some(RtEvent::RoomLeave { room, user }) if room.id == "R1" && user == "U1"
        ));

        let update = r#"{"type":"sh_room_update","user":"U1","room":{"id":"R1","channels":["C1"],"participants":[],"has_ended":true}}"#;
        match parse_event(update) {
            Some(RtEvent::RoomUpdate { room }) => {
                assert!(room.has_ended);
                assert!(!room.is_active());
            }
            other => panic!("expected RoomUpdate, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_is_unknown_not_none() {
        let frame = r#"{"type":"pref_change","name":"x"}"#;
        assert!(matches!(parse_event(frame), Some(RtEvent::Unknown(_))));
    }

    #[test]
    fn non_event_frames_are_none() {
        assert!(parse_event("not json").is_none());
        assert!(parse_event(r#"{"reply_to":1,"ok":true}"#).is_none());
    }

    #[test]
    fn flannel_url_contains_token_and_gateway() {
        let url = flannel_url("xoxc-abc", "T123");
        assert!(url.contains("token=xoxc-abc"));
        assert!(url.contains("gateway_server=T123-1"));
        assert!(url.starts_with("wss://"));
    }

    #[test]
    fn user_typing_frame_contains_channel() {
        assert_eq!(
            user_typing_frame("C1"),
            r#"{"type":"user_typing","channel":"C1"}"#
        );
    }

    #[test]
    fn presence_sub_frame_contains_ids() {
        let frame = presence_sub_frame(&["U1".into()]);
        let value: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["type"], "presence_sub");
        assert_eq!(value["ids"], serde_json::json!(["U1"]));
    }

    #[test]
    fn presence_query_frame_contains_ids() {
        let frame = presence_query_frame(&["U1".into(), "U2".into()]);
        let value: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["type"], "presence_query");
        assert_eq!(value["ids"], serde_json::json!(["U1", "U2"]));
    }
}
