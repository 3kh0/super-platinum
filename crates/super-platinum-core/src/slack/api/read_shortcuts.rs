//! Exact explicit bulk-read and confirmation-pref requests, not a channel loop.
//! Matching R608486/610819, V2299474 and S1594849 (hashes in
//! state::read_shortcuts) establish users.markAllRead and snake-cased undo_ts.
//! V c3c571e00152d656 SHA d86c1c81334d0f50739b5dd137521be3dd843b3d51ff5566fdf0744df6f5c34a.
//! S1446324 establishes the separate one-pref name/value setter.
use crate::state::read_shortcuts::{CONFIRM_CLEAR_UNREADS, ClearUnreadsPreference};
use crate::{
    config::WorkspaceSession,
    slack::{
        Error,
        client::{PreparedRequest, SlackClient},
    },
};
use serde_json::Value;

/// Non-secret request scope. Epoch identifies the selected account lifetime;
/// desktop must ALSO retain/check CursorOwner's transport and RT generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadShortcutOwner {
    epoch: u64,
    team: String,
    user: String,
    url: String,
}

impl ReadShortcutOwner {
    pub fn capture(epoch: u64, workspace: &WorkspaceSession) -> Result<Self, Error> {
        // Source dialog supports enterprise/all-workspace filters, but its
        // generic token/filter route is not yet proven in this application.
        // Never send a workspace-labelled clear with potentially enterprise scope.
        if workspace.enterprise_id.is_some() {
            return Err(Error::Api(
                "read_shortcuts_enterprise_scope_unavailable".into(),
            ));
        }
        if workspace.team_id.is_empty() || workspace.user_id.is_empty() {
            return Err(Error::Api("read_shortcuts_owner_unavailable".into()));
        }
        let url = url::Url::parse(&workspace.url).map_err(|_| owner_changed())?;
        if url.scheme() != "https"
            || !url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port_or_known_default() != Some(443)
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
        {
            return Err(owner_changed());
        }
        Ok(Self {
            epoch,
            team: workspace.team_id.clone(),
            user: workspace.user_id.clone(),
            url: workspace.url.clone(),
        })
    }
    pub fn team_id(&self) -> &str {
        &self.team
    }
    pub fn account_epoch(&self) -> u64 {
        self.epoch
    }
    pub fn current(&self, epoch: u64, workspace: &WorkspaceSession) -> bool {
        workspace.enterprise_id.is_none()
            && epoch == self.epoch
            && workspace.team_id == self.team
            && workspace.user_id == self.user
            && workspace.url == self.url
    }
    fn validate(&self, epoch: u64, workspace: &WorkspaceSession) -> Result<(), Error> {
        if self.current(epoch, workspace) {
            Ok(())
        } else {
            Err(owner_changed())
        }
    }
}

/// Consumed when preparing one dispatch. Caller creates this only after the
/// renderer has resolved shortcut eligibility and any required confirmation.
pub struct BulkReadIntent {
    owner: ReadShortcutOwner,
    undo_ts: Option<String>,
}

impl BulkReadIntent {
    pub fn clear(owner: ReadShortcutOwner) -> Self {
        Self {
            owner,
            undo_ts: None,
        }
    }
    pub fn owner(&self) -> &ReadShortcutOwner {
        &self.owner
    }
    pub fn request(
        self,
        client: &SlackClient,
        workspace: &WorkspaceSession,
        epoch: u64,
    ) -> Result<PreparedRequest, Error> {
        self.owner.validate(epoch, workspace)?;
        let mut fields = Vec::new();
        let reason = if let Some(undo_ts) = self.undo_ts {
            fields.push(("undo_ts", undo_ts));
            "UndoShiftEscapeMarkAllRead"
        } else {
            "ShiftEscapeConfirmDialog"
        };
        fields.push(("_x_reason", reason.into()));
        Ok(client.rest_form(workspace, "users.markAllRead", fields))
    }
}

/// Receipt deliberately has no Clone. It is bound to its original account and
/// workspace and can produce one Undo intent. Renderer owns the six-second
/// toast lifetime (B24500431), checks it before calling into_undo, and discards
/// receipts on owner change. An Undo response must not create another Undo UI.
pub struct BulkReadReceipt {
    owner: ReadShortcutOwner,
    undo_ts: Option<String>,
}

impl BulkReadReceipt {
    pub fn parse(owner: ReadShortcutOwner, response: &Value) -> Result<Self, Error> {
        bulk_read_success(response)?;
        Ok(Self {
            owner,
            undo_ts: response
                .get("undo_ts")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
    pub fn has_undo(&self) -> bool {
        self.undo_ts.is_some()
    }
    pub fn into_undo(self) -> Option<BulkReadIntent> {
        Some(BulkReadIntent {
            owner: self.owner,
            undo_ts: Some(self.undo_ts?),
        })
    }
}

/// A positive acknowledgement confirms the operation, not individual cursor
/// timestamps. Refresh authoritative counts/thread/Activity state afterward.
/// Unknown/contradictory output stays uncertain; never release a no-replay guard
/// just because a familiar error code appeared alongside a mutation receipt.
pub(crate) fn bulk_read_success(response: &Value) -> Result<(), Error> {
    let Some(object) = response.as_object() else {
        return Err(unknown());
    };
    let clean = object.keys().all(|key| {
        matches!(
            key.as_str(),
            "ok" | "error" | "undo_ts" | "needed" | "provided" | "response_metadata"
        )
    }) && diagnostic_fields_valid(response);
    if clean
        && object.get("ok") == Some(&Value::Bool(true))
        && object.get("error").is_none_or(Value::is_null)
        && object
            .get("undo_ts")
            .is_none_or(|v| v.is_null() || v.as_str().is_some_and(valid_undo_ts))
    {
        return Ok(());
    }
    if clean
        && !object.contains_key("undo_ts")
        && object.get("ok") == Some(&Value::Bool(false))
        && let Some(code) = object.get("error").and_then(Value::as_str)
        && matches!(
            code,
            "invalid_auth"
                | "not_authed"
                | "token_revoked"
                | "token_expired"
                | "account_inactive"
                | "missing_scope"
                | "invalid_arguments"
                | "invalid_arg_name"
        )
    {
        return Err(Error::Api(code.into()));
    }
    Err(unknown())
}

fn diagnostic_fields_valid(response: &Value) -> bool {
    ["needed", "provided"]
        .iter()
        .all(|key| response.get(*key).is_none_or(Value::is_string))
        && response.get("response_metadata").is_none_or(|value| {
            value.as_object().is_some_and(|object| {
                object.iter().all(|(key, value)| {
                    matches!(key.as_str(), "warnings" | "messages")
                        && value
                            .as_array()
                            .is_some_and(|items| items.iter().all(Value::is_string))
                })
            })
        })
}

/// Only for errors returned by execute_read_shortcuts. Offline is returned only
/// before dispatch; the adapter maps a lost response after dispatch to Transport.
/// HTTP failures/rate limits/timeouts remain uncertain and must not trigger a
/// replay. Successful bulk receipts still require authoritative refresh.
pub fn read_shortcut_definitely_rejected(error: &Error) -> bool {
    matches!(error, Error::Offline(_))
        || matches!(error, Error::Api(code) if matches!(code.as_str(),
            "invalid_auth" | "not_authed" | "token_revoked" | "token_expired"
            | "account_inactive" | "missing_scope" | "invalid_arguments" | "invalid_arg_name"))
}

fn valid_undo_ts(ts: &str) -> bool {
    let Some((seconds, fraction)) = ts.split_once('.') else {
        return false;
    };
    !seconds.is_empty()
        && seconds.bytes().all(|b| b.is_ascii_digit())
        && seconds.parse::<u64>().is_ok()
        && !fraction.is_empty()
        && fraction.len() <= 6
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

pub fn clear_unreads_preference_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "users.prefs.get",
        vec![("prefs", CONFIRM_CLEAR_UNREADS.into())],
    )
}

pub fn parse_clear_unreads_preference(response: &Value) -> Result<ClearUnreadsPreference, Error> {
    super::super::transport::notification_preferences_success(response)?;
    let prefs = response
        .get("prefs")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Api("unsupported_clear_unreads_preference".into()))?;
    Ok(ClearUnreadsPreference::from_prefs(
        &prefs.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    ))
}

/// Only the checkbox's observed false write; never reuse the Reading radio
/// setter. Caller fresh-reads and compares raw baseline immediately beforehand,
/// then readbacks false before treating suppression as synchronized authority.
pub fn disable_clear_unreads_confirmation_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    epoch: u64,
    owner: &ReadShortcutOwner,
    baseline: &ClearUnreadsPreference,
    fresh: &ClearUnreadsPreference,
) -> Result<PreparedRequest, Error> {
    owner.validate(epoch, workspace)?;
    if baseline != fresh || fresh.requires_confirmation().is_err() {
        return Err(Error::Api("clear_unreads_preference_changed".into()));
    }
    Ok(client.rest_form(
        workspace,
        "users.prefs.set",
        vec![
            ("name", CONFIRM_CLEAR_UNREADS.into()),
            ("value", "false".into()),
            ("_x_reason", "prefs-api/setUserPrefByApi".into()),
        ],
    ))
}

fn owner_changed() -> Error {
    Error::Api("read_shortcuts_owner_changed".into())
}
fn unknown() -> Error {
    Error::Transport("Bulk read outcome is unconfirmed; do not replay".into())
}

#[cfg(test)]
mod tests;
