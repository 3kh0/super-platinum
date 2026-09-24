//! Renderer-neutral huddle call state.
//!
//! One call at a time, like Slack. The shell drives it from two directions:
//! Slack (`rooms.join` / `rooms.leave`, `sh_room_*` frames, invites) and the
//! media bridge (Chime session events). Every call carries a generation, and
//! every async result or media event names the generation it belongs to, so a
//! slow join or a late "stopped" from a call already left cannot touch the one
//! that replaced it.

use std::time::Instant;

use crate::slack::models::{
    ChannelId, HuddleInvite, Room, TeamId, UserId, huddle_user_from_external_id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HuddlePhase {
    /// `rooms.join` is in flight.
    Joining,
    /// Slack answered; the Chime session is starting.
    Connecting,
    Connected,
    /// Chime lost the media path and is retrying on its own.
    Reconnecting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HuddleParticipant {
    pub user: UserId,
    pub speaking: bool,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub struct HuddleCall {
    pub generation: u64,
    pub team: TeamId,
    pub channel: ChannelId,
    pub phase: HuddlePhase,
    /// The room id (`R…`), once `rooms.join` has answered.
    pub room: Option<String>,
    /// Chime's id for this seat; `rooms.leave` needs it.
    pub attendee: Option<String>,
    pub muted: bool,
    /// Who the media session can hear, as Chime reports it. Empty until the
    /// session starts; the Slack-side roster (`Room::participants`) fills in
    /// before then.
    pub participants: Vec<HuddleParticipant>,
    pub connected_at: Option<Instant>,
}

/// What the Chime session reported, translated off the bridge.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaEvent {
    Started,
    Connecting {
        reconnecting: bool,
    },
    /// The session is over. `status` is the SDK's `MeetingSessionStatusCode`
    /// name (`Left`, `MeetingEnded`, …), kept as a name so a renumbering in a
    /// later SDK cannot turn one reason into another.
    Stopped {
        status: String,
    },
    /// The local microphone state the session actually applied.
    Muted(bool),
    /// One entry per attendee: `(ExternalUserId, speaking, muted)`.
    Roster(Vec<(String, bool, bool)>),
    /// The session could not be set up at all (no microphone, bad
    /// credentials, SDK failed to load).
    Failed(String),
}

/// Why a call stopped. The shell turns this into copy, and a stop it did not
/// ask for still gets a `rooms.leave` so Slack never keeps a ghost seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HuddleEnd {
    Ended,
    MovedToAnotherDevice,
    Removed,
    Disconnected(String),
    Failed(String),
}

impl HuddleEnd {
    pub fn message(&self) -> String {
        match self {
            Self::Ended => "Huddle ended".into(),
            Self::MovedToAnotherDevice => "Huddle moved to another device".into(),
            Self::Removed => "You were removed from the huddle".into(),
            Self::Disconnected(status) => format!("Huddle disconnected ({status})"),
            Self::Failed(reason) => format!("Could not start huddle audio: {reason}"),
        }
    }

    fn from_status(status: &str) -> Self {
        match status {
            "MeetingEnded" | "AudioCallEnded" | "NoAttendeePresent" => Self::Ended,
            "AudioJoinedFromAnotherDevice" => Self::MovedToAnotherDevice,
            "AudioAttendeeRemoved" => Self::Removed,
            other => Self::Disconnected(other.to_owned()),
        }
    }
}

/// A call the shell has to tear down: stop the media session and, when the
/// room and seat are known, give the seat back with `rooms.leave`.
#[derive(Debug, Clone)]
pub struct Teardown {
    pub call: HuddleCall,
    /// Set when the call ended on its own; `None` when the user left.
    pub reason: Option<HuddleEnd>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingInvite {
    pub team: TeamId,
    pub invite: HuddleInvite,
    pub received_at: Instant,
}

#[derive(Debug, Default)]
pub struct HuddleState {
    pub call: Option<HuddleCall>,
    pub invites: Vec<PendingInvite>,
    generation: u64,
    /// Chime's nearest media region, looked up once per session and only when
    /// starting a huddle.
    pub region: Option<String>,
}

impl HuddleState {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether this generation is still the live call.
    pub fn is_current(&self, generation: u64) -> bool {
        self.call
            .as_ref()
            .is_some_and(|call| call.generation == generation)
    }

    pub fn is_in(&self, team: &str, channel: &str) -> bool {
        self.call
            .as_ref()
            .is_some_and(|call| call.team == team && call.channel == channel)
    }

    /// Starts a join. Returns the new generation, plus the call it replaces —
    /// Slack allows one huddle at a time, so switching is leave-then-join.
    /// `None` when this conversation's huddle is already joined or joining.
    pub fn begin_join(
        &mut self,
        team: TeamId,
        channel: ChannelId,
    ) -> Option<(u64, Option<Teardown>)> {
        if self.is_in(&team, &channel) {
            return None;
        }
        let replaced = self.leave();
        self.generation += 1;
        // Answering by joining settles any ring for this conversation.
        self.invites
            .retain(|pending| !(pending.team == team && pending.invite.channel_id == channel));
        self.call = Some(HuddleCall {
            generation: self.generation,
            team,
            channel,
            phase: HuddlePhase::Joining,
            room: None,
            attendee: None,
            muted: false,
            participants: Vec::new(),
            connected_at: None,
        });
        Some((self.generation, replaced))
    }

    /// `rooms.join` answered. False when the user has moved on, in which case
    /// the caller owns giving the fresh seat straight back.
    pub fn joined(&mut self, generation: u64, room: String, attendee: String) -> bool {
        let Some(call) = self
            .call
            .as_mut()
            .filter(|call| call.generation == generation)
        else {
            return false;
        };
        call.room = Some(room);
        call.attendee = Some(attendee);
        call.phase = HuddlePhase::Connecting;
        true
    }

    /// `rooms.join` failed; forget the attempt if it is still the live one.
    pub fn join_failed(&mut self, generation: u64) {
        if self.is_current(generation) {
            self.call = None;
        }
    }

    /// The user hung up.
    pub fn leave(&mut self) -> Option<Teardown> {
        self.call.take().map(|call| Teardown { call, reason: None })
    }

    /// Flips the requested mic state. The session confirms with
    /// [`MediaEvent::Muted`]; until then the control already shows the choice.
    pub fn toggle_mute(&mut self) -> Option<bool> {
        let call = self.call.as_mut()?;
        call.muted = !call.muted;
        Some(call.muted)
    }

    pub fn apply_media(&mut self, generation: u64, event: MediaEvent) -> Option<Teardown> {
        let call = self
            .call
            .as_mut()
            .filter(|call| call.generation == generation)?;
        match event {
            MediaEvent::Started => {
                call.phase = HuddlePhase::Connected;
                call.connected_at.get_or_insert_with(Instant::now);
            }
            MediaEvent::Connecting { reconnecting } => {
                if reconnecting {
                    call.phase = HuddlePhase::Reconnecting;
                } else if call.phase == HuddlePhase::Joining {
                    call.phase = HuddlePhase::Connecting;
                }
            }
            MediaEvent::Muted(muted) => call.muted = muted,
            MediaEvent::Roster(attendees) => call.participants = merge_roster(attendees),
            MediaEvent::Stopped { status } => {
                let call = self.call.take()?;
                return Some(Teardown {
                    call,
                    reason: Some(HuddleEnd::from_status(&status)),
                });
            }
            MediaEvent::Failed(reason) => {
                let call = self.call.take()?;
                return Some(Teardown {
                    call,
                    reason: Some(HuddleEnd::Failed(reason)),
                });
            }
        }
        None
    }

    /// A `sh_room_*` frame. Ends the call when its room is over, and drops any
    /// ring for a room that no longer exists.
    pub fn apply_room(&mut self, team: &str, room: &Room) -> Option<Teardown> {
        if !room.is_active() {
            self.invites
                .retain(|pending| !(pending.team == team && pending.invite.call_id == room.id));
        }
        let ends_call = !room.is_active()
            && self
                .call
                .as_ref()
                .is_some_and(|call| call.team == team && call.room.as_deref() == Some(&room.id));
        if !ends_call {
            return None;
        }
        self.call.take().map(|call| Teardown {
            call,
            reason: Some(HuddleEnd::Ended),
        })
    }

    /// Records a ring. Ignored when this user is already in that huddle; a
    /// second ring for the same conversation replaces the first.
    pub fn add_invite(&mut self, team: TeamId, invite: HuddleInvite, now: Instant) -> bool {
        let already_in = self.call.as_ref().is_some_and(|call| {
            call.team == team
                && (call.channel == invite.channel_id
                    || call.room.as_deref() == Some(&invite.call_id))
        });
        if already_in {
            return false;
        }
        self.invites.retain(|pending| {
            !(pending.team == team && pending.invite.channel_id == invite.channel_id)
        });
        self.invites.push(PendingInvite {
            team,
            invite,
            received_at: now,
        });
        true
    }

    pub fn cancel_invite(&mut self, team: &str, channel: &str) -> bool {
        let before = self.invites.len();
        self.invites
            .retain(|pending| !(pending.team == team && pending.invite.channel_id == channel));
        self.invites.len() != before
    }

    pub fn take_invite(&mut self, team: &str, channel: &str) -> Option<PendingInvite> {
        let index = self
            .invites
            .iter()
            .position(|pending| pending.team == team && pending.invite.channel_id == channel)?;
        Some(self.invites.remove(index))
    }

    /// Drops rings older than `max_age`. Slack stops ringing after about half
    /// a minute and does not always say so.
    pub fn expire_invites(&mut self, now: Instant, max_age: std::time::Duration) -> bool {
        let before = self.invites.len();
        self.invites
            .retain(|pending| now.duration_since(pending.received_at) < max_age);
        self.invites.len() != before
    }
}

/// One row per Slack user. A second device or a screen share is a separate
/// Chime attendee for the same person: they are speaking if any seat is, and
/// muted only if every seat is.
fn merge_roster(attendees: Vec<(String, bool, bool)>) -> Vec<HuddleParticipant> {
    let mut out: Vec<HuddleParticipant> = Vec::new();
    for (external, speaking, muted) in attendees {
        let Some(user) = huddle_user_from_external_id(&external) else {
            continue;
        };
        let is_content = external.contains('#');
        match out.iter_mut().find(|row| row.user == user) {
            Some(row) => {
                row.speaking |= speaking;
                if !is_content {
                    row.muted &= muted;
                }
            }
            None => out.push(HuddleParticipant {
                user: user.to_owned(),
                speaking,
                // A screen share carries no microphone of its own.
                muted: muted || is_content,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invite(channel: &str, room: &str) -> HuddleInvite {
        HuddleInvite {
            channel_id: channel.into(),
            call_id: room.into(),
            sender_user_id: Some("U2".into()),
        }
    }

    fn room(id: &str, channel: &str, ended: bool) -> Room {
        Room {
            id: id.into(),
            channels: vec![channel.into()],
            participants: vec!["U1".into()],
            has_ended: ended,
            ..Room::default()
        }
    }

    fn connected(state: &mut HuddleState, channel: &str, room: &str) -> u64 {
        let (generation, _) = state.begin_join("T1".into(), channel.into()).expect("join");
        assert!(state.joined(generation, room.into(), "a-1".into()));
        assert!(state.apply_media(generation, MediaEvent::Started).is_none());
        generation
    }

    #[test]
    fn join_walks_joining_connecting_connected() {
        let mut state = HuddleState::default();
        let (generation, replaced) = state.begin_join("T1".into(), "C1".into()).expect("join");
        assert!(replaced.is_none());
        assert_eq!(
            state.call.as_ref().map(|c| c.phase),
            Some(HuddlePhase::Joining)
        );
        assert!(state.joined(generation, "R1".into(), "a-1".into()));
        assert_eq!(
            state.call.as_ref().map(|c| c.phase),
            Some(HuddlePhase::Connecting)
        );
        state.apply_media(generation, MediaEvent::Started);
        let call = state.call.as_ref().expect("call");
        assert_eq!(call.phase, HuddlePhase::Connected);
        assert!(call.connected_at.is_some());
        assert_eq!(call.attendee.as_deref(), Some("a-1"));
    }

    #[test]
    fn joining_the_same_huddle_twice_is_a_no_op() {
        let mut state = HuddleState::default();
        state.begin_join("T1".into(), "C1".into()).expect("join");
        assert!(state.begin_join("T1".into(), "C1".into()).is_none());
    }

    #[test]
    fn joining_elsewhere_tears_down_the_current_call() {
        let mut state = HuddleState::default();
        let first = connected(&mut state, "C1", "R1");
        let (second, replaced) = state.begin_join("T1".into(), "C2".into()).expect("join");
        let replaced = replaced.expect("previous call handed back");
        assert_eq!(replaced.call.generation, first);
        assert_eq!(replaced.call.room.as_deref(), Some("R1"));
        assert!(replaced.reason.is_none(), "a switch is a user leave");
        assert!(state.is_current(second));
        assert!(!state.is_current(first));
    }

    #[test]
    fn a_slow_join_for_an_abandoned_call_is_refused() {
        let mut state = HuddleState::default();
        let (generation, _) = state.begin_join("T1".into(), "C1".into()).expect("join");
        state.leave();
        assert!(!state.joined(generation, "R1".into(), "a-1".into()));
        assert!(state.call.is_none());
    }

    #[test]
    fn media_events_from_an_old_generation_are_ignored() {
        let mut state = HuddleState::default();
        let old = connected(&mut state, "C1", "R1");
        state.leave();
        let new = connected(&mut state, "C2", "R2");
        assert!(
            state
                .apply_media(
                    old,
                    MediaEvent::Stopped {
                        status: "Left".into()
                    }
                )
                .is_none()
        );
        assert!(state.is_current(new));
    }

    #[test]
    fn an_unrequested_stop_ends_the_call_with_its_reason() {
        let mut state = HuddleState::default();
        let generation = connected(&mut state, "C1", "R1");
        let teardown = state
            .apply_media(
                generation,
                MediaEvent::Stopped {
                    status: "AudioJoinedFromAnotherDevice".into(),
                },
            )
            .expect("teardown");
        assert_eq!(teardown.reason, Some(HuddleEnd::MovedToAnotherDevice));
        assert_eq!(teardown.call.attendee.as_deref(), Some("a-1"));
        assert!(state.call.is_none());
    }

    #[test]
    fn a_media_failure_ends_the_call() {
        let mut state = HuddleState::default();
        let (generation, _) = state.begin_join("T1".into(), "C1".into()).expect("join");
        state.joined(generation, "R1".into(), "a-1".into());
        let teardown = state
            .apply_media(generation, MediaEvent::Failed("NotAllowedError".into()))
            .expect("teardown");
        assert!(matches!(teardown.reason, Some(HuddleEnd::Failed(_))));
    }

    #[test]
    fn reconnecting_is_its_own_phase() {
        let mut state = HuddleState::default();
        let generation = connected(&mut state, "C1", "R1");
        state.apply_media(generation, MediaEvent::Connecting { reconnecting: true });
        assert_eq!(
            state.call.as_ref().map(|c| c.phase),
            Some(HuddlePhase::Reconnecting)
        );
        state.apply_media(generation, MediaEvent::Started);
        assert_eq!(
            state.call.as_ref().map(|c| c.phase),
            Some(HuddlePhase::Connected)
        );
    }

    #[test]
    fn the_room_ending_ends_the_call_but_other_rooms_do_not() {
        let mut state = HuddleState::default();
        connected(&mut state, "C1", "R1");
        assert!(state.apply_room("T1", &room("R9", "C9", true)).is_none());
        assert!(state.apply_room("T1", &room("R1", "C1", false)).is_none());
        let teardown = state
            .apply_room("T1", &room("R1", "C1", true))
            .expect("ended");
        assert_eq!(teardown.reason, Some(HuddleEnd::Ended));
        assert!(state.call.is_none());
    }

    #[test]
    fn mute_toggles_and_the_session_has_the_last_word() {
        let mut state = HuddleState::default();
        let generation = connected(&mut state, "C1", "R1");
        assert_eq!(state.toggle_mute(), Some(true));
        state.apply_media(generation, MediaEvent::Muted(false));
        assert_eq!(state.call.as_ref().map(|c| c.muted), Some(false));
        state.leave();
        assert_eq!(state.toggle_mute(), None);
    }

    #[test]
    fn roster_merges_devices_and_screen_shares_per_user() {
        let mut state = HuddleState::default();
        let generation = connected(&mut state, "C1", "R1");
        state.apply_media(
            generation,
            MediaEvent::Roster(vec![
                ("T1-R1-U1".into(), false, false),
                ("T1-R1-U2".into(), false, true),
                ("T1-R1-U2-2".into(), true, true),
                ("T1-R1-U1#content".into(), false, false),
                ("recorder".into(), true, false),
            ]),
        );
        let participants = &state.call.as_ref().expect("call").participants;
        assert_eq!(
            participants,
            &vec![
                HuddleParticipant {
                    user: "U1".into(),
                    speaking: false,
                    muted: false
                },
                HuddleParticipant {
                    user: "U2".into(),
                    speaking: true,
                    muted: true
                },
            ]
        );
    }

    #[test]
    fn invites_dedupe_and_settle() {
        let mut state = HuddleState::default();
        let now = Instant::now();
        assert!(state.add_invite("T1".into(), invite("D1", "R1"), now));
        assert!(state.add_invite("T1".into(), invite("D1", "R2"), now));
        assert_eq!(state.invites.len(), 1);
        assert_eq!(state.invites[0].invite.call_id, "R2");

        // The ringing room ending withdraws the ring.
        state.apply_room("T1", &room("R2", "D1", true));
        assert!(state.invites.is_empty());

        state.add_invite("T1".into(), invite("D1", "R3"), now);
        assert!(state.cancel_invite("T1", "D1"));
        assert!(!state.cancel_invite("T1", "D1"));

        // Joining the conversation answers its ring.
        state.add_invite("T1".into(), invite("D1", "R3"), now);
        state.begin_join("T1".into(), "D1".into());
        assert!(state.invites.is_empty());
    }

    #[test]
    fn a_ring_for_the_huddle_already_joined_is_ignored() {
        let mut state = HuddleState::default();
        connected(&mut state, "C1", "R1");
        assert!(!state.add_invite("T1".into(), invite("C1", "R1"), Instant::now()));
        assert!(state.invites.is_empty());
    }

    #[test]
    fn stale_invites_expire() {
        let mut state = HuddleState::default();
        let then = Instant::now();
        state.add_invite("T1".into(), invite("D1", "R1"), then);
        let window = std::time::Duration::from_secs(45);
        assert!(!state.expire_invites(then, window));
        assert!(state.expire_invites(then + window, window));
        assert!(state.invites.is_empty());
    }
}
