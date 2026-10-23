//! rfHj -> Cdn9 group selection, separate from single-person DM resolution.
//! Complete returned membership correlation is defensive native policy beyond
//! source's truthy-channel check. This module never synthesizes missing members
//! or grants posting from an open ACK. Caller retains original peer/owner facts.
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody},
};
use serde_json::{Value, json};

mod eligibility;
pub use super::{Attempt, Outcome};
pub use eligibility::{eligible, eligible_with_membership};
pub const METHOD: &str = "conversations.open";
pub const REASON: &str = "startMpimByMemberIds";

#[derive(Clone)]
pub struct Open {
    peers: Vec<Value>,
    team: String,
    self_id: String,
    members: Vec<String>,
    existing: Option<Value>,
}
impl Open {
    pub fn new(
        peers: &[Value],
        team: &str,
        self_id: &str,
        existing: Option<&Value>,
    ) -> Result<Self, Error> {
        let (peers, members) = eligibility::members(peers, team, self_id).ok_or_else(invalid)?;
        if existing.is_some_and(|c| {
            !eligibility::channel_matches(c, &members) || c["fromAnotherTeam"] != false
        }) {
            return Err(invalid());
        }
        Ok(Self {
            peers,
            team: team.into(),
            self_id: self_id.into(),
            members,
            existing: existing.cloned(),
        })
    }
    pub fn peers(&self) -> &[Value] {
        &self.peers
    }
    pub fn member_ids(&self) -> &[String] {
        &self.members
    }
    pub fn self_id(&self) -> &str {
        &self.self_id
    }
    pub fn reusable_channel(&self) -> Option<&Value> {
        self.existing.as_ref().filter(|c| c["is_open"] == true)
    }
    pub fn request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<Request, Error> {
        if session.team_id != self.team
            || session.user_id != self.self_id
            || session.token.trim().is_empty()
            || self.reusable_channel().is_some()
        {
            return Err(invalid());
        }
        Ok(Request {
            request: client.avatar_multipart(session, METHOD, self.fields()),
            open: self.clone(),
        })
    }
    fn fields(&self) -> Vec<(&'static str, String)> {
        // Cdn9 passes memberIds even for a closed mapped MPIM. The single-DM
        // 6OO5 channel lookup branch does not apply to this >1-member array.
        vec![
            ("quiet", "1".into()),
            ("return_im", "1".into()),
            ("users", self.members.join(",")),
            ("_x_reason", REASON.into()),
        ]
    }
    pub fn decode(&self, raw: Value) -> Result<Opened, Error> {
        if !super::acknowledged(&raw)
            || !eligibility::channel_matches(&raw["channel"], &self.members)
        {
            return Err(Error::Api("workflow_share_group_invalid_response".into()));
        }
        // A closed mapped group is only a lookup hint: the source opens by
        // member set and accepts the actual matching returned group ID.
        let members = raw["channel"]["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        Ok(Opened { raw, members })
    }
}

#[derive(Clone)]
pub struct Request {
    pub(crate) request: PreparedRequest,
    open: Open,
}
impl Request {
    pub fn prepared(&self) -> &PreparedRequest {
        &self.request
    }
    pub fn open(&self) -> &Open {
        &self.open
    }
    pub fn inspect(&self) -> Value {
        let RequestBody::Form(fields) = &self.request.body else {
            unreachable!("closed factory")
        };
        json!({"method":METHOD,"url":self.request.url,
            "fields":fields.iter().filter(|(k,_)|k!="token").collect::<Vec<_>>(),
            "tokenPresent":fields.first().is_some_and(|(k,v)|k=="token"&&!v.is_empty())})
    }
    pub(crate) fn fields_current(&self) -> bool {
        let RequestBody::Form(fields) = &self.request.body else {
            return false;
        };
        fields
            .first()
            .is_some_and(|(k, v)| k == "token" && !v.trim().is_empty())
            && fields.len() == self.open.fields().len() + 1
            && fields
                .iter()
                .skip(1)
                .zip(self.open.fields())
                .all(|((k, v), (ek, ev))| k == ek && v == &ev)
    }
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowShareGroupRequest")
            .field("method", &METHOD)
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct Opened {
    raw: Value,
    members: Vec<String>,
}
impl Opened {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn channel(&self) -> &Value {
        &self.raw["channel"]
    }
    pub fn id(&self) -> &str {
        self.raw["channel"]["id"].as_str().expect("validated ID")
    }
    pub fn member_ids(&self) -> &[String] {
        &self.members
    }
}
fn invalid() -> Error {
    Error::Api("workflow_share_group_invalid_input".into())
}
#[cfg(test)]
mod tests;
