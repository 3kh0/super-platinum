//! Single known local human selection: rfHj -> Cdn9 -> BGxa/6OO5.
//! Opening a conversation is independent of posting a workflow message. The
//! host owns current canonical member/channel observations and send permission.
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody},
};
use serde_json::{Value, json};

mod eligibility;
pub use eligibility::eligible;
// Same per-HTTP-attempt shape, retained in a separate open-operation ledger.
// Acknowledged here means correlated conversation open, never message delivery.
pub use super::{Attempt, Outcome};
pub const METHOD: &str = "conversations.open";
pub const REASON: &str = "startMpimByMemberIds";

#[derive(Clone)]
pub struct Open {
    peer: Value,
    team: String,
    existing: Option<Value>,
}
impl Open {
    pub fn new(peer: &Value, team: &str, existing: Option<&Value>) -> Result<Self, Error> {
        if !eligible(peer, team)
            || existing.is_some_and(|c| {
                !channel_matches(c, peer["id"].as_str().unwrap()) || c["fromAnotherTeam"] != false
            })
        {
            return Err(invalid());
        }
        Ok(Self {
            peer: peer.clone(),
            team: team.into(),
            existing: existing.cloned(),
        })
    }
    pub fn peer(&self) -> &Value {
        &self.peer
    }
    pub fn peer_id(&self) -> &str {
        self.peer["id"].as_str().expect("validated peer")
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
            || session.token.trim().is_empty()
            || session.user_id.is_empty()
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
        let mut fields = vec![("quiet", "1".into()), ("return_im", "1".into())];
        fields.push(match &self.existing {
            Some(c) => (
                "channel",
                c["id"].as_str().expect("validated channel").into(),
            ),
            None => ("users", self.peer_id().into()),
        });
        fields.push(("_x_reason", REASON.into()));
        fields
    }
    /// Strict native response correlation, beyond source's truthy channel check.
    /// No normalized or permission fields are manufactured from this response.
    pub fn decode(&self, raw: Value) -> Result<Opened, Error> {
        if !super::acknowledged(&raw)
            || !channel_matches(&raw["channel"], self.peer_id())
            || self
                .existing
                .as_ref()
                .is_some_and(|c| c["id"] != raw["channel"]["id"])
        {
            return Err(Error::Api("workflow_share_person_invalid_response".into()));
        }
        Ok(Opened { raw })
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
    pub fn peer_id(&self) -> &str {
        self.open.peer_id()
    }
    pub fn inspect(&self) -> Value {
        let RequestBody::Form(fields) = &self.request.body else {
            unreachable!("closed factory")
        };
        json!({"method":METHOD,"url":self.request.url,"fields":fields.iter().filter(|(k,_)|k!="token").collect::<Vec<_>>(),
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
        f.debug_struct("WorkflowSharePersonRequest")
            .field("method", &METHOD)
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct Opened {
    raw: Value,
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
    pub fn peer_id(&self) -> &str {
        self.raw["channel"]["user"]
            .as_str()
            .expect("validated peer")
    }
}
fn channel_matches(c: &Value, peer: &str) -> bool {
    c["id"]
        .as_str()
        .is_some_and(|id| eligibility::id_kind(id, b"D"))
        && c["is_im"] == true
        && c["user"] == peer
        && eligibility::clear(
            c,
            &[
                "is_channel",
                "is_group",
                "is_private",
                "is_mpim",
                "is_archived",
                "is_frozen",
                "isUnknown",
                "isNonExistent",
                "is_pending_ext_shared",
                "is_ext_shared",
                "is_file",
                "fromAnotherTeam",
            ],
        )
        && ["is_open", "is_read_only", "is_thread_only"]
            .iter()
            .all(|k| matches!(c.get(*k), None | Some(Value::Null) | Some(Value::Bool(_))))
        && match c.get("properties") {
            None | Some(Value::Null) => true,
            Some(Value::Object(p)) => match p.get("huddles") {
                None | Some(Value::Null) => true,
                Some(Value::Object(h)) => matches!(
                    h.get("is_huddle_channel"),
                    None | Some(Value::Null) | Some(Value::Bool(false))
                ),
                _ => false,
            },
            _ => false,
        }
}
fn invalid() -> Error {
    Error::Api("workflow_share_person_invalid_input".into())
}

#[cfg(test)]
mod tests;
