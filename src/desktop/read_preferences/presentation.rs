//! Renderer-only conversation positions and debounced visibility evidence.
use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Conversation {
    pub epoch: u64,
    pub account: String,
    pub team: String,
    pub channel: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Scrollmark {
    pub ts: String,
    pub offset: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Observation {
    pub scope: String,
    pub key: String,
    pub stable: bool,
    pub visible: bool,
    pub divider_visible: bool,
    pub ghost: bool,
    pub at_bottom: bool,
    pub pending: bool,
    pub first_id: Option<String>,
    pub offset: Option<f64>,
}

#[derive(Clone, Default)]
pub(crate) struct Presentation {
    marks: BTreeMap<Conversation, Scrollmark>,
    observation: Option<Observation>,
    /// An opening restoration is deliberately distinct from an explicit jump.
    pub restore: Option<(String, Scrollmark)>,
    pub unread_loading: Option<(String, String)>,
    pub unread_fetching: Option<(String, String)>,
    pub unread_continuation: Option<UnreadContinuation>,
    pub unread_paused: bool,
}

#[derive(Clone)]
pub(crate) struct UnreadContinuation {
    pub scope: String,
    pub boundary: String,
    pub pages: super::pagination::Pages,
    pub owner: std::sync::Arc<crate::realtime::activity_counts::CursorOwner>,
}

impl Presentation {
    pub fn saved(&self, conversation: &Conversation) -> Option<&Scrollmark> {
        self.marks.get(conversation)
    }

    pub fn forget(&mut self, conversation: &Conversation) {
        self.marks.remove(conversation);
    }

    pub fn observed(&self, scope: &str, key: &str) -> Option<&Observation> {
        self.observation
            .as_ref()
            .filter(|seen| seen.scope == scope && seen.key == key && seen.stable && seen.visible)
    }

    /// Old dispatcher messages cannot import a previous conversation's offset
    /// or attention. No message content, cookie or token enters this state.
    pub fn accept(
        &mut self,
        conversation: &Conversation,
        scope: &str,
        key: &str,
        sample: Observation,
    ) -> bool {
        if sample.scope != scope || sample.key != key {
            return false;
        }
        self.marks.retain(|owner, _| {
            owner.epoch == conversation.epoch && owner.account == conversation.account
        });
        if !sample.pending
            && sample.visible
            && let Some(ts) = sample.first_id.as_ref().filter(|id| !id.is_empty())
            && let Some(offset) = sample.offset.filter(|value| value.is_finite())
        {
            // A modest session-only bound; this is a viewport cache, not draft
            // or read-cursor persistence. Existing conversations are retained.
            if self.marks.len() < 512 || self.marks.contains_key(conversation) {
                self.marks.insert(
                    conversation.clone(),
                    Scrollmark {
                        ts: ts.clone(),
                        offset,
                    },
                );
            }
        }
        let changed = self.observation.as_ref() != Some(&sample);
        self.observation = Some(sample);
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(channel: &str) -> Conversation {
        Conversation {
            epoch: 1,
            account: "A".into(),
            team: "T".into(),
            channel: channel.into(),
        }
    }
    fn sample() -> Observation {
        Observation {
            scope: "visit".into(),
            key: "read-state".into(),
            stable: true,
            visible: true,
            divider_visible: true,
            ghost: false,
            at_bottom: false,
            pending: false,
            first_id: Some("123.000001".into()),
            offset: Some(-12.25),
        }
    }

    #[test]
    fn navigation_and_changed_read_state_reject_old_attention_and_position() {
        let mut p = Presentation::default();
        assert!(!p.accept(&owner("C"), "new-visit", "read-state", sample()));
        assert!(!p.accept(&owner("C"), "visit", "new-state", sample()));
        assert!(p.saved(&owner("C")).is_none());
        assert!(p.accept(&owner("C"), "visit", "read-state", sample()));
        assert!(p.observed("visit", "read-state").is_some());
        assert!(p.observed("visit", "new-state").is_none());
    }

    #[test]
    fn visibility_loss_revokes_debounced_permission_immediately() {
        let mut p = Presentation::default();
        p.accept(&owner("C"), "visit", "read-state", sample());
        let mut hidden = sample();
        hidden.stable = false;
        hidden.visible = false;
        hidden.offset = Some(999.0);
        p.accept(&owner("C"), "visit", "read-state", hidden);
        assert!(p.observed("visit", "read-state").is_none());
        assert_eq!(p.saved(&owner("C")).unwrap().offset, -12.25);
    }

    #[test]
    fn marks_are_conversation_and_account_scoped_and_pending_never_overwrites() {
        let mut p = Presentation::default();
        p.accept(&owner("C"), "visit", "read-state", sample());
        assert!(p.saved(&owner("OTHER")).is_none());
        let mut pending = sample();
        pending.pending = true;
        pending.offset = Some(200.0);
        p.accept(&owner("C"), "visit", "read-state", pending);
        assert_eq!(p.saved(&owner("C")).unwrap().offset, -12.25);
        let mut changed = owner("C");
        changed.epoch = 2;
        p.accept(&changed, "visit", "read-state", sample());
        assert!(p.saved(&owner("C")).is_none());
    }

    #[test]
    fn invalid_offsets_do_not_replace_a_valid_scrollmark() {
        let mut p = Presentation::default();
        p.accept(&owner("C"), "visit", "read-state", sample());
        let mut bad = sample();
        bad.offset = Some(f64::NAN);
        p.accept(&owner("C"), "visit", "read-state", bad);
        assert_eq!(p.saved(&owner("C")).unwrap().offset, -12.25);
    }
}
