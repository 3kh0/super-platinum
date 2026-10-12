//! Live drafts use `draft_create`, `draft_update`, and `draft_delete` frames.
//! Frame timestamps order socket events; draft revision timestamps order payloads.
//! The two clocks are distinct and must not be compared with one another.
use super::{ScheduledMessage, ScheduledState};
use crate::{slack::events::RawEvent, state::ts_key};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(super) struct RealtimeDrafts {
    versions: BTreeMap<String, String>,
    pub pending: BTreeMap<String, Option<ScheduledMessage>>,
    pub retired: BTreeSet<String>,
}

impl ScheduledState {
    /// Apply an account/workspace-scoped event without another network request.
    /// Keep changes arriving during a list fetch as an overlay on that snapshot.
    pub fn apply_event(&mut self, event: &RawEvent) -> bool {
        if !matches!(
            event.kind.as_str(),
            "draft_create" | "draft_update" | "draft_delete"
        ) {
            return false;
        }
        let Some((_, team)) = self.scope.as_ref() else {
            return false;
        };
        let Some(id) = event
            .rest
            .get("draft_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            return false;
        };
        let Some(stamp) = event
            .rest
            .get("event_ts")
            .and_then(|v| v.as_str())
            .filter(|s| ts_key(s).0 > 0)
        else {
            return false;
        };
        if self
            .realtime
            .versions
            .get(id)
            .is_some_and(|old| ts_key(old) >= ts_key(stamp))
        {
            return false;
        }
        let draft = if event.kind == "draft_delete" {
            None
        } else {
            let Some(value) = event.rest.get("draft") else {
                return false;
            };
            // These frames are full drafts. A partial or malformed payload must
            // not replace saved text, files, or destinations with defaults.
            if !["blocks", "file_ids", "destinations"]
                .iter()
                .all(|field| value.get(field).is_some_and(|v| v.is_array()))
                || value
                    .get("team_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|t| t != team)
            {
                return false;
            }
            let Ok(draft) = serde_json::from_value::<ScheduledMessage>(value.clone()) else {
                return false;
            };
            if draft.id != id || ts_key(&draft.last_updated_ts).0 == 0 {
                return false;
            }
            if self.remote_is_obsolete(&draft) {
                self.realtime.versions.insert(id.into(), stamp.into());
                return false;
            }
            draft.active().then_some(draft)
        };
        self.realtime.versions.insert(id.into(), stamp.into());
        if let Some(draft) = &draft {
            self.items.insert(id.into(), draft.clone());
        } else {
            self.realtime.retired.insert(id.into());
            self.items.remove(id);
        }
        if self.loading {
            self.realtime.pending.insert(id.into(), draft);
        }
        true
    }

    pub(super) fn remote_is_obsolete(&self, draft: &ScheduledMessage) -> bool {
        (draft.active() && self.realtime.retired.contains(&draft.id))
            || self.items.get(&draft.id).is_some_and(|current| {
                !current.last_updated_ts.is_empty()
                    && !draft.last_updated_ts.is_empty()
                    && ts_key(&current.last_updated_ts) > ts_key(&draft.last_updated_ts)
            })
    }

    pub(super) fn merge_page(
        &mut self,
        drafts: Vec<ScheduledMessage>,
    ) -> BTreeMap<String, ScheduledMessage> {
        let mut items = BTreeMap::new();
        for draft in drafts {
            if self.realtime.retired.contains(&draft.id) || !draft.active() {
                continue;
            }
            let draft = if self.remote_is_obsolete(&draft) {
                self.items.get(&draft.id).unwrap().clone()
            } else {
                draft
            };
            items.insert(draft.id.clone(), draft);
        }
        for (id, draft) in std::mem::take(&mut self.realtime.pending) {
            if let Some(draft) = draft {
                // A response can also be newer than the socket update.
                if items.get(&id).is_none_or(|current| {
                    ts_key(&current.last_updated_ts) <= ts_key(&draft.last_updated_ts)
                }) {
                    items.insert(id, draft);
                }
            } else {
                items.remove(&id);
            }
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduled::ScheduledPage;
    use serde_json::json;

    fn event(kind: &str, id: &str, stamp: &str, revision: &str) -> RawEvent {
        serde_json::from_value(json!({"type":kind,"draft_id":id,"event_ts":stamp,"draft":{
            "id":id,"team_id":"T1","last_updated_ts":revision,"blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":revision}]}]}],
            "file_ids":["F1"],"destinations":[{"channel_id":"C1","thread_ts":"1.2","broadcast":true}],"date_scheduled":0
        }})).unwrap()
    }
    fn state() -> ScheduledState {
        let mut state = ScheduledState::default();
        state.ensure_scope(1, "T1");
        state
    }
    fn draft(event: &RawEvent) -> ScheduledMessage {
        serde_json::from_value(event.rest["draft"].clone()).unwrap()
    }
    #[test]
    fn full_live_drafts_preserve_rich_content_files_thread_and_scheduling() {
        let mut state = state();
        let created = event("draft_create", "Dr1", "10.1", "10.2");
        assert!(state.apply_event(&created));
        assert_eq!(state.items["Dr1"].file_ids, vec!["F1"]);
        assert_eq!(state.items["Dr1"].thread_ts(), Some("1.2"));
        let mut changed = event("draft_update", "Dr1", "11.1", "11.2");
        changed.rest.get_mut("draft").unwrap()["date_scheduled"] = 100.into();
        assert!(state.apply_event(&changed));
        assert!(state.items["Dr1"].scheduled());
        assert_eq!(state.items["Dr1"].blocks, draft(&changed).blocks);
        assert!(!state.apply_event(&created));
        let deleted: RawEvent = serde_json::from_value(
            json!({"type":"draft_delete","draft_id":"Dr1","event_ts":"12.1"}),
        )
        .unwrap();
        assert!(state.apply_event(&deleted));
        assert!(!state.items.contains_key("Dr1"));
        assert!(!state.apply_event(&changed));
        // A delayed successful mutation response cannot resurrect a deleted draft.
        state.accept_remote(draft(&changed));
        assert!(!state.items.contains_key("Dr1"));
    }
    #[test]
    fn events_overlay_inflight_pages_without_canceling_the_load() {
        let mut state = state();
        let original = event("draft_create", "Dr1", "10.1", "10.2");
        let (generation, limit) = state.begin(false).unwrap();
        assert!(state.apply_event(&event("draft_update", "Dr1", "11.1", "11.2")));
        assert!(state.apply_event(&event("draft_create", "Dr2", "11.3", "11.4")));
        assert!(state.apply(
            generation,
            ScheduledPage {
                drafts: vec![draft(&original)],
                has_more: true
            },
            limit
        ));
        assert!(state.loaded && !state.loading && state.has_more);
        assert_eq!(state.items["Dr1"].last_updated_ts, "11.2");
        assert!(state.items.contains_key("Dr2"));
        let (generation, limit) = state.begin(true).unwrap();
        assert!(state.apply_event(&event("draft_delete", "Dr1", "12.1", "12.2")));
        assert!(state.apply(
            generation,
            ScheduledPage {
                drafts: vec![draft(&original)],
                has_more: false
            },
            limit
        ));
        assert!(!state.items.contains_key("Dr1"));
        // Even a later stale server snapshot must respect the deletion.
        let (generation, limit) = state.begin(false).unwrap();
        assert!(state.apply(
            generation,
            ScheduledPage {
                drafts: vec![draft(&original)],
                has_more: false
            },
            limit
        ));
        assert!(!state.items.contains_key("Dr1"));
    }
    #[test]
    fn revision_and_event_clocks_are_separate_and_ordered_per_draft() {
        let mut state = state();
        state.accept_remote(draft(&event("draft_update", "Dr1", "20.1", "20.9")));
        // New socket timestamp does not make an older payload authoritative.
        assert!(!state.apply_event(&event("draft_update", "Dr1", "21.1", "19.9")));
        assert!(state.apply_event(&event("draft_update", "Dr2", "10.1", "10.9")));
        // Delete's event clock can be earlier than the latest payload clock.
        assert!(state.apply_event(&event("draft_delete", "Dr2", "10.2", "10.9")));
        assert!(!state.items.contains_key("Dr2"));
        assert_eq!(state.items["Dr1"].last_updated_ts, "20.9");
    }
    #[test]
    fn malformed_partial_and_wrong_workspace_events_do_not_erase_content() {
        let mut state = state();
        let original = event("draft_create", "Dr1", "10.1", "10.2");
        assert!(state.apply_event(&original));
        for field in ["blocks", "file_ids", "destinations"] {
            let mut partial = event("draft_update", "Dr1", "11.1", "11.2");
            partial
                .rest
                .get_mut("draft")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(!state.apply_event(&partial));
        }
        let mut wrong = event("draft_update", "Dr1", "11.1", "11.2");
        wrong.rest.get_mut("draft").unwrap()["team_id"] = "T2".into();
        assert!(!state.apply_event(&wrong));
        wrong.rest.get_mut("draft").unwrap()["team_id"] = "T1".into();
        wrong.rest.get_mut("draft").unwrap()["id"] = "Another".into();
        assert!(!state.apply_event(&wrong));
        assert_eq!(state.items["Dr1"].blocks, draft(&original).blocks);
    }
    #[test]
    fn account_switch_discards_event_versions_and_tombstones() {
        let mut state = state();
        assert!(state.apply_event(&event("draft_delete", "Dr1", "20.1", "20.2")));
        state.ensure_scope(2, "T1");
        assert!(state.apply_event(&event("draft_create", "Dr1", "10.1", "10.2")));
        assert!(state.items.contains_key("Dr1"));
    }
    #[test]
    fn newer_http_snapshot_wins_over_an_earlier_inflight_event() {
        let mut state = state();
        let (generation, limit) = state.begin(false).unwrap();
        assert!(state.apply_event(&event("draft_update", "Dr1", "10.1", "10.2")));
        let newest = draft(&event("draft_update", "Dr1", "11.1", "11.2"));
        assert!(state.apply(
            generation,
            ScheduledPage {
                drafts: vec![newest.clone()],
                has_more: false
            },
            limit
        ));
        assert!(state.items["Dr1"].same_revision(&newest));
    }
}
