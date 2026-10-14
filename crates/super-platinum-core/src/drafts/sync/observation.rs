//! Ordered observations for an already owned draft. No enrollment or dispatch.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftObservation {
    pub scope: Scope,
    pub draft_id: String,
    pub client_msg_id: String,
    pub event_ts: String,
    pub kind: ObservationKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationKind {
    Active(RemoteDraft),
    Deleted,
    /// Full draft evidence whose lifecycle/content cannot be represented by
    /// RemoteDraft. Keep the first such observation and block further writes.
    Unsupported(Value),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ObservationError {
    #[error("draft observation scope does not match its owner")]
    Scope,
    #[error("draft observation identity is invalid or not owned")]
    Identity,
    #[error("draft observation event timestamp is malformed")]
    Timestamp,
    #[error("draft observation content is malformed")]
    Content,
}

fn event_timestamp(value: &str) -> bool {
    value.split_once('.').is_some_and(|(seconds, fraction)| {
        !seconds.is_empty()
            && seconds.bytes().all(|b| b.is_ascii_digit())
            && seconds.parse::<u64>().is_ok_and(|s| s > 0)
            && fraction.len() == 6
            && fraction.bytes().all(|b| b.is_ascii_digit())
    })
}

impl DraftObservation {
    fn validate(&self) -> Result<(), ObservationError> {
        if !identifier(&self.draft_id) || !nonnil_uuid(&self.client_msg_id) {
            return Err(ObservationError::Identity);
        }
        if !event_timestamp(&self.event_ts) {
            return Err(ObservationError::Timestamp);
        }
        match &self.kind {
            ObservationKind::Active(remote) => {
                if remote.scope != self.scope
                    || remote.id != self.draft_id
                    || remote.client_msg_id != self.client_msg_id
                    || DraftSync::adopt_remote(&self.scope, remote.clone()).is_err()
                {
                    return Err(ObservationError::Content);
                }
            }
            ObservationKind::Unsupported(raw) => {
                if raw.get("id").and_then(Value::as_str) != Some(&self.draft_id)
                    || raw.get("client_msg_id").and_then(Value::as_str)
                        != Some(&self.client_msg_id)
                {
                    return Err(ObservationError::Content);
                }
            }
            ObservationKind::Deleted => {}
        }
        Ok(())
    }
}

impl DraftSync {
    pub fn observed_event_ts(&self, draft_id: &str) -> Option<&str> {
        self.observed_events.get(draft_id).map(|event| event.event_ts.as_str())
    }

    pub fn blocked_observation(&self) -> Option<&DraftObservation> {
        self.blocked_observation.as_ref()
    }

    /// Used by durable loading as well as tests. Invalid ordering evidence must
    /// fail closed rather than authorize writes after a restart.
    pub fn observation_state_valid(&self) -> bool {
        self.observed_events
            .iter()
            .all(|(id, event)| event.scope == self.scope && id == &event.draft_id && event.validate().is_ok())
            && self.blocked_observation.as_ref().is_none_or(|event| {
                event.scope == self.scope
                    && event.validate().is_ok()
                    && matches!(self.workflow, Workflow::Conflict { .. })
                    && self
                        .observed_event_ts(&event.draft_id)
                        .is_some_and(|stamp| ts_key(stamp) >= ts_key(&event.event_ts))
            })
    }

    fn owns_observation(&self, event: &DraftObservation) -> bool {
        if let Some(base) = &self.acknowledged {
            return base.id == event.draft_id && base.client_msg_id == event.client_msg_id;
        }
        !self.observed_events.values().any(|old| {
            old.client_msg_id == event.client_msg_id && old.draft_id != event.draft_id
        }) && self.pending().is_some_and(|operation| {
            operation.kind == OperationKind::Create
                && operation.base.is_none()
                && operation.client_msg_id == event.client_msg_id
        })
    }

    fn block_observation(&mut self, event: &DraftObservation) {
        if self.blocked_observation.is_none() {
            self.blocked_observation = Some(event.clone());
        }
        if !matches!(self.workflow, Workflow::Conflict { .. }) {
            self.workflow = Workflow::Conflict {
                remote: None,
                pending: self.pending().cloned(),
            };
        }
    }

    /// Caller authenticates the socket/account/user before supplying the event.
    /// Returns true when either ordering evidence or owned content changed.
    /// Persist the entire result and paired local value in one transaction.
    pub fn observe_event(
        &mut self,
        event: &DraftObservation,
    ) -> Result<bool, ObservationError> {
        if event.scope != self.scope {
            return Err(ObservationError::Scope);
        }
        event.validate()?;
        if self.retired_ids.contains(&event.draft_id) {
            return Ok(false);
        }
        if !self.owns_observation(event) {
            return Err(ObservationError::Identity);
        }
        if let Some(old) = self.observed_events.get(&event.draft_id) {
            match ts_key(&old.event_ts).cmp(&ts_key(&event.event_ts)) {
                std::cmp::Ordering::Greater => return Ok(false),
                std::cmp::Ordering::Equal if old == event => return Ok(false),
                std::cmp::Ordering::Equal => {
                    let previous = self.clone();
                    self.block_observation(event);
                    return Ok(*self != previous);
                }
                std::cmp::Ordering::Less => {}
            }
        }
        match &event.kind {
            ObservationKind::Active(remote) => {
                // Unsupported but representable blocks/files become a conflict,
                // never a lossy local projection. Legacy observe stays intact.
                if crate::drafts::codec::decode(&self.scope.key, &remote.snapshot).is_ok() {
                    self.observe(remote.clone());
                } else {
                    self.observe_conflict(remote.clone());
                }
            }
            ObservationKind::Deleted => {
                self.observe_deleted(&event.scope, &event.draft_id);
            }
            ObservationKind::Unsupported(_) => {
                self.block_observation(event);
            }
        }
        self.observed_events
            .insert(event.draft_id.clone(), event.clone());
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
