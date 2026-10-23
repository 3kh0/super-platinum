//! Cold MANAGE_CHANNEL_WORKFLOWS acquisition (g6Ad -> mLk1 -> GpMd -> M3rm).
//! A positive response is not a channel/posting grant or a fresh native owner.
//! Callers must retain and recheck original account, channel, source and UI custody
//! before using it. This helper neither caches permission nor dispatches mutations.
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Allowed,
    Denied,
}

#[derive(Clone)]
pub struct Read {
    channel: String,
    session: WorkspaceSession,
}
impl std::fmt::Debug for Read {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeaturePermissionRead")
            .field("channel", &self.channel)
            .field("team", &self.session.team_id)
            .field("viewer", &self.session.user_id)
            .finish_non_exhaustive()
    }
}
fn invalid() -> Error {
    Error::Api("workflow_feature_permission_invalid".into())
}
fn channel_id(id: &str) -> bool {
    (9..=255).contains(&id.len())
        && matches!(id.as_bytes()[0], b'C' | b'G' | b'D' | b'Z')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn scope_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 255 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
impl Read {
    /// Reads permission for this actual non-IM channel. Membership, posting,
    /// guest/host restrictions and Feature count are separately acquired facts.
    pub fn capture(raw_channel: &Value, session: &WorkspaceSession) -> Result<Self, Error> {
        let channel = raw_channel
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| channel_id(id))
            .ok_or_else(invalid)?;
        if raw_channel["is_im"] == true
            || raw_channel["isUnknown"] == true
            || raw_channel["isNonExistent"] == true
            || raw_channel.get("is_im").is_some_and(|v| !v.is_boolean())
            || !scope_id(&session.team_id)
            || !matches!(session.team_id.as_bytes()[0], b'T' | b'E')
            || !scope_id(&session.user_id)
            || session.token.is_empty()
            || session
                .enterprise_id
                .as_ref()
                .is_some_and(|id| !scope_id(id) || !id.starts_with('E'))
            || (session.team_id.starts_with('E')
                && session
                    .enterprise_id
                    .as_ref()
                    .is_some_and(|id| id != &session.team_id))
        {
            return Err(invalid());
        }
        Ok(Self {
            channel: channel.into(),
            session: session.clone(),
        })
    }
    fn same_session(&self, session: &WorkspaceSession) -> Result<(), Error> {
        let old = &self.session;
        if session.team_id != old.team_id
            || session.enterprise_id != old.enterprise_id
            || session.user_id != old.user_id
            || session.token != old.token
            || session.url != old.url
        {
            return Err(Error::Api(
                "workflow_feature_permission_context_changed".into(),
            ));
        }
        Ok(())
    }
    pub fn discovery(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        self.same_session(session)?;
        Ok(client.rest_form(
            session,
            "api.getFlannelHttpUrl",
            vec![
                ("include_external_workspaces", "true".into()),
                ("_x_reason", "client-needs-valid-flannel-url".into()),
            ],
        ))
    }
    pub fn request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
        discovery: &Value,
    ) -> Result<PreparedRequest, Error> {
        self.same_session(session)?;
        if discovery["ok"] != true
            || discovery.get("error").is_some_and(|v| !v.is_null())
            || discovery["ttl_seconds"].as_u64().is_none_or(|ttl| ttl == 0)
        {
            return Err(invalid());
        }
        let endpoint = discovery["url"].as_str().ok_or_else(invalid)?;
        let team_url = format!("https://edgeapi.slack.com/cache/{}", self.session.team_id);
        let enterprise_url = self
            .session
            .enterprise_id
            .as_ref()
            .map(|id| format!("https://edgeapi.slack.com/cache/{id}"));
        if endpoint != team_url && enterprise_url.as_deref() != Some(endpoint) {
            return Err(invalid());
        }
        // EVxR uses the actual boot context. Enterprise metadata on an ordinary
        // workspace is not an Enterprise boot and must not inject either field.
        let mut context = session.clone();
        context.enterprise_id = None;
        let mut request = client
            .edge_json(
                &context,
                "permissions/info",
                json!({
                    "user_id":self.session.user_id,"channel_id":self.channel,
                    "permissions":["MANAGE_CHANNEL_WORKFLOWS"]
                }),
            )
            .map_err(|_| invalid())?;
        if self.session.team_id.starts_with('E') {
            let RequestBody::Json(body) = &mut request.body else {
                return Err(invalid());
            };
            body["enterprise_token"] = session.token.clone().into();
        }
        let suffix = request
            .url
            .split_once('?')
            .map(|(_, query)| format!("?{query}"))
            .unwrap_or_default();
        request.url = format!("{endpoint}/permissions/info{suffix}");
        Ok(request)
    }
    /// M3rm's channel branch uses only `allowed`; aggregate `allowed_per_team`
    /// cannot grant. Missing/null allowed is source-equivalent empty/denied.
    /// Invalid envelope/element types remain unavailable, never false success.
    pub fn permission(&self, raw: &Value) -> Result<Permission, Error> {
        if raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || serde_json::to_vec(raw).map_err(|_| invalid())?.len() > 256 * 1024
        {
            return Err(invalid());
        }
        let allowed = match raw.get("allowed") {
            None | Some(Value::Null) => return Ok(Permission::Denied),
            Some(Value::Array(values)) if values.len() <= 4096 => values,
            _ => return Err(invalid()),
        };
        let mut granted = false;
        for value in allowed {
            let name = value
                .as_str()
                .filter(|name| !name.is_empty() && name.len() <= 128)
                .ok_or_else(invalid)?;
            granted |= name == "MANAGE_CHANNEL_WORKFLOWS";
        }
        Ok(if granted {
            Permission::Allowed
        } else {
            Permission::Denied
        })
    }
}
#[cfg(test)]
mod tests;
