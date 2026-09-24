use super::*;

/// The media half of a huddle: an Amazon Chime SDK `Meeting` + `Attendee` pair.
///
/// `rooms.join` hands these back under `call.free_willy` ("free_willy" is the
/// `media_backend_type` Slack reports for Chime-backed rooms). They are passed
/// to `amazon-chime-sdk-js` untouched, so they stay raw JSON rather than a
/// struct that would silently drop whatever field the next SDK wants.
///
/// `attendee.JoinToken` is a per-attendee secret, so `Debug` never prints the
/// payload and nothing here is `Serialize`: the only way out is
/// [`ChimeCredentials::bridge_payload`], which feeds the WebView.
#[derive(Clone, Deserialize)]
pub struct ChimeCredentials {
    pub meeting: Value,
    pub attendee: Value,
}

impl std::fmt::Debug for ChimeCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChimeCredentials")
            .field("meeting_id", &self.meeting.get("MeetingId"))
            .field("attendee_id", &self.attendee_id())
            .finish_non_exhaustive()
    }
}

impl ChimeCredentials {
    /// Chime's id for this seat; `rooms.leave` names it as `attendee_id`.
    pub fn attendee_id(&self) -> Option<&str> {
        self.attendee.get("AttendeeId").and_then(Value::as_str)
    }

    /// Both objects in the shape `MeetingSessionConfiguration` accepts.
    ///
    /// Slack sends `MeetingFeatures: null`, which older SDK builds choke on
    /// when they lower-case property names; dropping it is what every working
    /// third-party client does, and the server has already applied it anyway.
    pub fn bridge_payload(&self) -> Value {
        let mut meeting = self.meeting.clone();
        if let Some(object) = meeting.as_object_mut()
            && object.get("MeetingFeatures").is_some_and(Value::is_null)
        {
            object.remove("MeetingFeatures");
        }
        serde_json::json!({ "meeting": meeting, "attendee": self.attendee })
    }
}

/// The parts of a `rooms.join` response the client acts on.
#[derive(Debug, Clone, Deserialize)]
pub struct RoomJoinResponse {
    pub call: JoinedCall,
    #[serde(default)]
    pub huddle: Option<Room>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JoinedCall {
    /// The room id (`R…`) — the same id `sh_room_*` frames carry as `room.id`.
    pub call_id: String,
    /// Absent for non-Chime backends, which this client cannot carry.
    #[serde(default)]
    pub free_willy: Option<ChimeCredentials>,
}

/// `huddle_invite`: somebody rang this user into a huddle.
///
/// The frame also carries a `free_willy` credential pair ("quick join"). It is
/// deliberately not read: accepting goes through `rooms.join` with the room id
/// instead, so no attendee secret sits in memory for an invite that may never
/// be answered, and a stale pair can never be what a join connects with.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HuddleInvite {
    pub channel_id: ChannelId,
    /// The room id (`R…`).
    pub call_id: String,
    #[serde(default)]
    pub sender_user_id: Option<UserId>,
}

/// The Slack user behind a Chime attendee.
///
/// Slack mints `ExternalUserId` as `<team>-<room>-<user>`, with an optional
/// fourth part for a second device, and screen shares ride as a separate
/// attendee suffixed `#content`. The real client accepts exactly three or four
/// parts and reads the third; anything else is not a Slack participant.
pub fn huddle_user_from_external_id(external: &str) -> Option<&str> {
    let base = external.split('#').next().unwrap_or(external);
    let parts: Vec<&str> = base.split('-').collect();
    matches!(parts.len(), 3 | 4)
        .then(|| parts[2])
        .filter(|user| !user.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_ids_resolve_to_the_slack_user() {
        assert_eq!(
            huddle_user_from_external_id("T0266FRGM-R08SV11N3V3-U072PTA5BNG"),
            Some("U072PTA5BNG")
        );
        assert_eq!(
            huddle_user_from_external_id("T1-R1-U1-2"),
            Some("U1"),
            "a second device still belongs to its user"
        );
        assert_eq!(huddle_user_from_external_id("T1-R1-U1#content"), Some("U1"));
        assert_eq!(huddle_user_from_external_id("recorder"), None);
        assert_eq!(huddle_user_from_external_id("a-b-c-d-e"), None);
    }

    #[test]
    fn join_response_reads_room_and_credentials() {
        let body = serde_json::json!({
            "ok": true,
            "call": {
                "call_id": "R1",
                "free_willy": {
                    "meeting": { "MeetingId": "m-1", "MeetingFeatures": null, "MediaRegion": "us-east-1" },
                    "attendee": { "AttendeeId": "a-1", "ExternalUserId": "T1-R1-U1", "JoinToken": "secret-token" }
                }
            },
            "huddle": { "id": "R1", "channels": ["C1"], "participants": ["U1"] },
            "canvas": { "root_thread_ts": "1.0" }
        });
        let response: RoomJoinResponse = serde_json::from_value(body).expect("join response");
        assert_eq!(response.call.call_id, "R1");
        let credentials = response.call.free_willy.expect("chime credentials");
        assert_eq!(credentials.attendee_id(), Some("a-1"));
        let payload = credentials.bridge_payload();
        assert!(payload["meeting"].get("MeetingFeatures").is_none());
        assert_eq!(payload["meeting"]["MediaRegion"], "us-east-1");
        assert_eq!(
            response.huddle.and_then(|room| room.channel().cloned()),
            Some("C1".into())
        );
    }

    #[test]
    fn credentials_debug_never_prints_the_join_token() {
        let credentials = ChimeCredentials {
            meeting: serde_json::json!({ "MeetingId": "m-1" }),
            attendee: serde_json::json!({ "AttendeeId": "a-1", "JoinToken": "secret-token" }),
        };
        let printed = format!("{credentials:?}");
        assert!(printed.contains("a-1"));
        assert!(!printed.contains("secret-token"));
    }
}
