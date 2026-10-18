//! Enterprise default channel-tab policy, using the messaging client's scoped
//! edge permission request. Numeric protobuf enum values are never wire names.
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        PreparedRequest, SlackClient,
        file_share_policy::{self, Eligibility, PostingFacts},
        models::User,
    },
};
use serde_json::json;

#[derive(Clone, Debug)]
pub struct ChannelPermissionRead {
    channel: String,
    viewer: String,
    enterprise: String,
    enterprise_metadata: Option<String>,
    team: String,
}
impl ChannelPermissionRead {
    pub fn capture(
        raw: &Value,
        member: &User,
        facts: &PostingFacts,
        session: &WorkspaceSession,
    ) -> Result<Self, Fault> {
        let enterprise = if session.team_id.starts_with('E') {
            if session
                .enterprise_id
                .as_ref()
                .is_some_and(|id| id != &session.team_id)
            {
                return Err(Fault::Changed);
            }
            session.team_id.clone()
        } else {
            session
                .enterprise_id
                .as_ref()
                .filter(|id| !id.is_empty())
                .ok_or(Fault::Unsupported)?
                .clone()
        };
        let channel = raw["id"]
            .as_str()
            .filter(|id| channel_id(id))
            .ok_or(Fault::Shape)?;
        if member.deleted
            || member.id.is_empty()
            || member.id != session.user_id
            || raw["is_im"] == true
            || raw["is_mpim"] == true
            || !raw
                .pointer("/properties/who_can_manage_tabs")
                .is_none_or(Value::is_null)
            || file_share_policy::destination(raw, facts, None) != Eligibility::Allowed
        {
            return Err(Fault::Permission);
        }
        Ok(Self {
            channel: channel.into(),
            viewer: member.id.clone(),
            enterprise,
            enterprise_metadata: session.enterprise_id.clone(),
            team: session.team_id.clone(),
        })
    }
    fn same_session(&self, session: &WorkspaceSession) -> Result<(), Fault> {
        if session.team_id != self.team
            || session.user_id != self.viewer
            || session.enterprise_id != self.enterprise_metadata
        {
            return Err(Fault::Changed);
        }
        Ok(())
    }
    /// EVxR obtains the endpoint for this credential context before querying it.
    pub fn discovery(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<PreparedRequest, Fault> {
        self.same_session(session)?;
        Ok(client.rest_form(
            session,
            "api.getFlannelHttpUrl",
            vec![("include_external_workspaces", "true".into())],
        ))
    }
    pub fn request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
        discovery: &Value,
    ) -> Result<PreparedRequest, Fault> {
        self.same_session(session)?;
        if discovery["ok"] != true
            || discovery.get("error").is_some_and(|v| !v.is_null())
            || discovery["ttl_seconds"].as_u64().is_none_or(|ttl| ttl == 0)
        {
            return Err(Fault::Shape);
        }
        let endpoint = discovery["url"].as_str().ok_or(Fault::Shape)?;
        // Local credential boundary: only the normal Slack edge origin and an
        // exact captured account context. Never send tokens to an arbitrary URL.
        let team_url = format!("https://edgeapi.slack.com/cache/{}", self.team);
        let enterprise_url = format!("https://edgeapi.slack.com/cache/{}", self.enterprise);
        if endpoint != team_url && endpoint != enterprise_url {
            return Err(Fault::Unsupported);
        }
        // QFsU resolves the current context token; EVxR adds enterprise_token
        // only for an Enterprise boot. The permission arguments contain no
        // enterprise_id, as_admin, or aggregate team_ids fields.
        let mut context = session.clone();
        context.enterprise_id = None;
        let mut request = client.edge_json(&context,"permissions/info",json!({"user_id":self.viewer,"channel_id":self.channel,"permissions":["MANAGE_CHANNEL_TABS"]})).map_err(|_|Fault::Shape)?;
        if self.team.starts_with('E') {
            let crate::slack::client::RequestBody::Json(body) = &mut request.body else {
                return Err(Fault::Shape);
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
    pub fn permission(&self, raw: &Value) -> Result<Permission, Fault> {
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(Fault::Shape);
        }
        if serde_json::to_vec(raw).map_err(|_| Fault::Shape)?.len() > 256 * 1024 {
            return Err(Fault::Limit);
        }
        let allowed = raw["allowed"].as_array().ok_or(Fault::Shape)?;
        if allowed.len() > 4096 {
            return Err(Fault::Limit);
        }
        let mut found = false;
        for value in allowed {
            let name = value
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .ok_or(Fault::Shape)?;
            found |= name == "MANAGE_CHANNEL_TABS";
        }
        Ok(if found {
            Permission::Allowed
        } else {
            Permission::Denied
        })
    }
}
#[cfg(test)]
mod tests;
