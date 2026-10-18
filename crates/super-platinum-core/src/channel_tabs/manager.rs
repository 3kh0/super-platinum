//! Positive channel-manager ingress from the shipped client's dYxD/A4Tr path.
//! A missing grant is unavailable, not proof that all other ACL branches deny.
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        PreparedRequest, SlackClient,
        file_share_policy::{self, Eligibility, PostingFacts},
        models::User,
    },
};

#[derive(Clone, Debug)]
pub struct ManagerRead {
    channel: String,
    viewer: String,
}
impl ManagerRead {
    /// Membership/posting gates always precede the manager grant. This read is
    /// only the explicit `managers` branch, never Enterprise's absent-policy ACL.
    pub fn capture(raw: &Value, member: &User, facts: &PostingFacts) -> Result<Self, Fault> {
        let channel = raw["id"]
            .as_str()
            .filter(|id| channel_id(id))
            .ok_or(Fault::Shape)?;
        if member.deleted
            || member.id.is_empty()
            || member.id.len() > 64
            || raw["is_im"] == true
            || raw["is_mpim"] == true
            || raw
                .pointer("/properties/who_can_manage_tabs")
                .and_then(Value::as_str)
                != Some("managers")
            || file_share_policy::destination(raw, facts, None) != Eligibility::Allowed
        {
            return Err(Fault::Permission);
        }
        Ok(Self {
            channel: channel.into(),
            viewer: member.id.clone(),
        })
    }
    pub fn request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<PreparedRequest, Fault> {
        if session.user_id != self.viewer {
            return Err(Fault::Changed);
        }
        Ok(client.rest_form(
            session,
            "admin.roles.entity.listAssignments",
            vec![("entity_id", self.channel.clone())],
        ))
    }
    /// `camelCaseOutput` maps raw role_assignments/role_id to the source's
    /// roleAssignments/roleId. Only the exact Rl0A users grant is consumed.
    pub fn permission(&self, raw: &Value) -> Result<Permission, Fault> {
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(Fault::Shape);
        }
        if serde_json::to_vec(raw).map_err(|_| Fault::Shape)?.len() > 256 * 1024 {
            return Err(Fault::Limit);
        }
        let roles = raw["role_assignments"].as_array().ok_or(Fault::Shape)?;
        if roles.len() > 256 {
            return Err(Fault::Limit);
        }
        let mut manager = None;
        for role in roles {
            let id = role["role_id"].as_str().ok_or(Fault::Shape)?;
            if id != "Rl0A" {
                continue;
            }
            if manager.is_some() {
                return Err(Fault::Shape);
            }
            let users = role["users"].as_array().ok_or(Fault::Shape)?;
            if users.len() > 4096 {
                return Err(Fault::Limit);
            }
            let mut found = false;
            for user in users {
                let user = user
                    .as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 64)
                    .ok_or(Fault::Shape)?;
                found |= user == self.viewer;
            }
            manager = Some(found);
        }
        Ok(if manager == Some(true) {
            Permission::Allowed
        } else {
            Permission::Unavailable
        })
    }
}

#[cfg(test)]
mod tests;
