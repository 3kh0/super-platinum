//! Read-only, request-bound own-profile capabilities for the messaging client.
//! Captured C41N/6NOe/E7ES gates; this is not the Gantry admin settings UI.
use super::*;
use crate::profile_text::Field;
use serde_json::{Value, json};
mod capabilities;
pub mod photo;
use capabilities::BootScope;
pub use capabilities::{FreshOwnProfileCapabilities, ProfileEditCapability};
#[cfg(test)]
mod tests;

/// Messaging `fetchClientInitData` S10301xx and `oiEO` client-boot1455579.
/// Includes authoritative self, team and internal workspace inventory.
pub fn own_profile_boot_request(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    c.rest_form(
        w,
        "client.init",
        vec![
            ("include_relevant_onboarding", "false".into()),
            ("_x_reason", "native-own-profile-capabilities".into()),
        ],
    )
}

/// Messaging team-pref hydration: client-boot1436284 reads toplevel_prefs
/// separately from user prefs and workspace_prefs.
pub fn own_profile_team_preferences_request(
    c: &SlackClient,
    w: &WorkspaceSession,
) -> PreparedRequest {
    c.rest_form(
        w,
        "client.extras",
        vec![
            ("extras", json!(["team_prefs"]).to_string()),
            ("_x_reason", "native-own-profile-capabilities".into()),
        ],
    )
}

/// M3rm S29238xx -> EVxR edge `permissions/info`. Suz6 S40183xx maps
/// protobuf enum KEYS to their string names; numeric values 69/81 are not wire
/// permissions. Captured requests and live Grid reads confirm string arrays.
/// Ordinary messaging context
/// has no admin page context: only the no-team_ids request sends as_admin=true.
pub fn own_profile_permissions_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    boot: &Value,
) -> Result<PreparedRequest, Error> {
    let scope = BootScope::capture(w, boot)?;
    let mut args = json!({"user_id":w.user_id,"permissions":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]});
    if scope.aggregate_workspaces() {
        args["team_ids"] = json!(scope.internal_teams);
    } else {
        args["as_admin"] = json!(true);
    }
    c.edge_json(w, "permissions/info", args)
        .map_err(|_| malformed("Could not prepare the profile permissions request."))
}

/// No secrets or cached capability grants are persisted. Call again for write
/// preflight; the desktop controller must still bind the async result to its
/// captured account, transport, credential and profile-editor generation.
pub async fn fetch_own_profile_capabilities(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<FreshOwnProfileCapabilities, Error> {
    let boot = t.execute(own_profile_boot_request(c, w)).await?;
    let scope = BootScope::capture(w, &boot)?;
    let permissions_request = own_profile_permissions_request(c, w, &boot)?;
    let (permissions, profile, extras) = tokio::try_join!(
        t.execute(permissions_request),
        t.execute(users_profile_get(c, w, w.user_id.clone())),
        t.execute(own_profile_team_preferences_request(c, w)),
    )?;
    let enterprise = if scope.enterprise {
        Some(
            t.execute(c.rest_form(
                w,
                "enterprise.prefs.get",
                vec![("_x_reason", "native-own-profile-capabilities".into())],
            ))
            .await?,
        )
    } else {
        None
    };
    FreshOwnProfileCapabilities::from_responses(
        w,
        &boot,
        &permissions,
        &profile,
        &extras,
        enterprise.as_ref(),
    )
}

fn malformed(reason: &str) -> Error {
    Error::Transport(reason.into())
}
fn success(value: &Value) -> Result<(), Error> {
    if value.get("ok").and_then(Value::as_bool) == Some(false)
        && let Some(error) = value
            .get("error")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    {
        return Err(Error::Api(error.into()));
    }
    if value.get("ok").and_then(Value::as_bool) != Some(true)
        || value.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(malformed(
            "Slack did not confirm the profile capability response.",
        ));
    }
    Ok(())
}
