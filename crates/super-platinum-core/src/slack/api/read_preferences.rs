//! Exact two-key conversation read preferences, distinct from notification prefs.
//!
//! B 26072315 emits both boolean fields. S 1445370 selects comma-separated
//! users.prefs.get names; S 1447048 sends JSON prefs with the multi-pref reason.
//! Bundle hashes and decoded-character offset convention are documented in
//! state::read_preferences. Builders do not dispatch or retry mutations.

use serde_json::Value;

use crate::config::WorkspaceSession;
use crate::slack::Error;
use crate::slack::client::{PreparedRequest, SlackClient};
use crate::state::{READ_PREFERENCE_NAMES, ReadMode, ReadPreferences};

pub fn read_preferences_request(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    c.rest_form(
        w,
        "users.prefs.get",
        vec![("prefs", READ_PREFERENCE_NAMES.join(","))],
    )
}

/// A successful complete requested snapshot is authority even when a key is
/// absent. Nonboolean present fields are returned intact: the caller must adopt
/// this snapshot to revoke older read permission, then use `mode()` to determine
/// whether it can present/edit the policy. Intent capture independently refuses
/// unsupported values. HTTP success alone is not a preference read.
pub fn parse_read_preferences(response: &Value) -> Result<ReadPreferences, Error> {
    super::super::transport::notification_preferences_success(response)?;
    let prefs = response
        .get("prefs")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    Ok(ReadPreferences::from_prefs(
        &prefs.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    ))
}

#[derive(Clone, Debug)]
pub struct ReadPreferenceIntent {
    team: String,
    user: String,
    baseline: ReadPreferences,
    desired: ReadMode,
}

impl ReadPreferenceIntent {
    pub fn capture(
        w: &WorkspaceSession,
        baseline: &ReadPreferences,
        desired: ReadMode,
    ) -> Result<Self, Error> {
        baseline.mode().map_err(|_| invalid())?;
        if w.team_id.is_empty() || w.user_id.is_empty() {
            return Err(Error::Api("read_preferences_owner_unavailable".into()));
        }
        Ok(Self {
            team: w.team_id.clone(),
            user: w.user_id.clone(),
            baseline: baseline.clone(),
            desired,
        })
    }

    pub fn team_id(&self) -> &str {
        &self.team
    }

    pub fn user_id(&self) -> &str {
        &self.user
    }

    pub fn desired(&self) -> ReadMode {
        self.desired
    }

    /// This is a local preflight check, not a server compare-and-swap promise.
    /// Same effective mode with different raw fields is still a changed baseline.
    pub fn revalidate(&self, fresh: &ReadPreferences) -> Result<(), Error> {
        fresh.mode().map_err(|_| invalid())?;
        if self.baseline != *fresh {
            return Err(Error::Api("read_preferences_changed".into()));
        }
        Ok(())
    }

    /// Only an exact positive readback confirms both requested fields. Missing
    /// or semantically equivalent fields cannot confirm the submitted patch.
    pub fn confirms(&self, fresh: &ReadPreferences) -> bool {
        fresh.mode().is_ok() && fresh.raw() == &self.desired.patch()
    }
}

pub fn read_preferences_update_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &ReadPreferenceIntent,
    fresh: &ReadPreferences,
) -> Result<PreparedRequest, Error> {
    if w.team_id != intent.team || w.user_id != intent.user {
        return Err(Error::Api("read_preferences_owner_changed".into()));
    }
    intent.revalidate(fresh)?;
    Ok(c.rest_form(
        w,
        "users.prefs.set",
        vec![
            (
                "prefs",
                serde_json::to_string(&intent.desired.patch()).map_err(|_| invalid())?,
            ),
            ("_x_reason", "prefs-api/setMultipleUserPrefsByApi".into()),
        ],
    ))
}

fn invalid() -> Error {
    Error::Api("unsupported_read_preferences".into())
}

#[cfg(test)]
mod tests;
