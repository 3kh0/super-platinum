//! +bc7/eYRl Feature admission and raw result/readback boundaries.
//! Acquired facts are not owner-currentness; the host rechecks custody at dispatch.
use super::{Previews, feature_permission::Permission};
use crate::{
    block_actions::posting_policy::Flags,
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody},
};
use serde_json::{Value, json};

pub const METHOD: &str = "workflows.featured.add";
pub const REASON: &str = "set_channel_workflow_as_workflow_actions";
pub const LIMIT: usize = 15;

/// All observations must belong to the same current account/channel. `can_post`
/// is the composite FS2H admission, not a grant inferred from posting flags.
pub struct Facts<'a> {
    pub team_id: &'a str,
    pub top_level_team_id: &'a str,
    pub self_member: &'a Value,
    pub posting: Flags,
    pub can_post: bool,
    pub manage: Permission,
}
#[derive(Clone)]
pub struct Add {
    trigger: Value,
    channel: Value,
    previews: Value,
    count: usize,
    team: String,
    user: String,
}
fn invalid() -> Error {
    Error::Api("workflow_feature_invalid".into())
}
fn id(raw: &Value) -> Result<&str, Error> {
    raw.get("id")
        .and_then(Value::as_str)
        .filter(|s| super::identifier(s))
        .ok_or_else(invalid)
}
fn featured(channel: &Value) -> Result<&[Value], Error> {
    if channel
        .get("properties")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Err(invalid());
    }
    let rows = match channel.pointer("/properties/channel_workflows") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(rows)) => rows.as_slice(),
        _ => return Err(invalid()),
    };
    if rows.len() > super::MAX_COLLECTION_ROWS
        || rows.iter().any(|row| {
            !row.get("workflow_trigger_id")
                .and_then(Value::as_str)
                .is_some_and(super::identifier)
        })
    {
        return Err(invalid());
    }
    Ok(rows)
}
/// djZg validation excludes only an actual nonempty error other than access_denied.
/// Unknown/nonexistent flags and missing replies do not themselves reduce count.
pub fn validated_count(channel: &Value, previews: &Previews) -> Result<usize, Error> {
    let mut count = 0;
    for row in featured(channel)? {
        let trigger = row["workflow_trigger_id"].as_str().ok_or_else(invalid)?;
        let preview = previews
            .items()
            .iter()
            .find(|p| p.id() == trigger)
            .ok_or_else(invalid)?;
        let error = preview
            .raw()
            .and_then(|v| v.get("error"))
            .and_then(Value::as_str);
        if !error.is_some_and(|e| !e.is_empty() && e != "access_denied") {
            count += 1;
        }
    }
    Ok(count)
}
impl Add {
    pub fn new(
        trigger: &Value,
        channel: &Value,
        previews: &Previews,
        facts: Facts<'_>,
    ) -> Result<Self, Error> {
        let trigger_id = id(trigger)?;
        let channel_id = id(channel)?;
        let user = id(facts.self_member)?;
        if !super::identifier(facts.team_id)
            || !super::identifier(facts.top_level_team_id)
            || !matches!(
                channel_id.as_bytes().first(),
                Some(b'C' | b'G' | b'D' | b'Z')
            )
            || trigger["type"] != "shortcut"
            || trigger["isUnknown"] == true
            || trigger["isNonExistent"] == true
            || trigger
                .get("error")
                .is_some_and(|v| !v.is_null() && v != "")
            || channel["is_member"] != true
            || channel["is_im"] == true
            || channel["is_archived"] == true
            || channel["isUnknown"] == true
            || channel["isNonExistent"] == true
            || facts.self_member["is_restricted"] == true
            || facts
                .self_member
                .get("is_restricted")
                .is_some_and(|v| !v.is_boolean() && !v.is_null())
            || !facts.can_post
            || !facts.posting.permits_root_post()
            || facts.manage != Permission::Allowed
        {
            return Err(invalid());
        }
        if let Some(host) = channel.get("conversation_host_id").filter(|v| !v.is_null()) {
            let host = host.as_str().ok_or_else(invalid)?;
            if !host.is_empty() && host != facts.team_id && host != facts.top_level_team_id {
                return Err(invalid());
            }
        }
        // Deliberately inspect the raw collection as well as validated count:
        // a filtered error must not authorize a duplicate write.
        if featured(channel)?
            .iter()
            .any(|r| r["workflow_trigger_id"] == trigger_id)
        {
            return Err(Error::Api("workflow_feature_duplicate".into()));
        }
        let count = validated_count(channel, previews)?;
        if count >= LIMIT {
            return Err(Error::Api("workflow_feature_limit".into()));
        }
        Ok(Self {
            trigger: trigger.clone(),
            channel: channel.clone(),
            previews: previews.raw().clone(),
            count,
            team: facts.team_id.into(),
            user: user.into(),
        })
    }
    pub fn trigger(&self) -> &Value {
        &self.trigger
    }
    pub fn channel(&self) -> &Value {
        &self.channel
    }
    pub fn previews(&self) -> &Value {
        &self.previews
    }
    pub fn validated_count(&self) -> usize {
        self.count
    }
    pub fn request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<Request, Error> {
        if session.team_id != self.team
            || session.user_id != self.user
            || session.token.trim().is_empty()
        {
            return Err(invalid());
        }
        let channel = id(&self.channel)?;
        let trigger = id(&self.trigger)?;
        // Reuse only existing token-first multipart preparation; the closed
        // Feature executor owns this endpoint, never the avatar executor.
        Ok(Request {
            request: client.avatar_multipart(
                session,
                METHOD,
                vec![
                    ("trigger_ids", trigger.into()),
                    ("channel_id", channel.into()),
                    ("_x_reason", REASON.into()),
                ],
            ),
        })
    }
    /// Actual fresh channel object (not a mutation ACK). Presence is observation,
    /// never acknowledgment, permission, automatic replay or canonical adoption.
    pub fn observe_channel(&self, channel: &Value) -> Result<Readback, Error> {
        if id(channel)? != id(&self.channel)? {
            return Err(invalid());
        }
        let rows = featured(channel)?;
        let present = rows
            .iter()
            .any(|r| r["workflow_trigger_id"] == self.trigger["id"]);
        Ok(Readback {
            raw: channel.clone(),
            present,
        })
    }
}
#[derive(Clone)]
pub struct Readback {
    raw: Value,
    present: bool,
}
impl Readback {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn present(&self) -> bool {
        self.present
    }
}
/// Strict native acknowledgment envelope; no invented featured-result fields.
/// Retained source accepts truthy ok; this narrower native validation is explicit.
pub fn acknowledged(raw: &Value) -> bool {
    raw.get("ok") == Some(&Value::Bool(true)) && !raw.get("error").is_some_and(|v| !v.is_null())
}
pub fn acknowledge(raw: &Value) -> Result<(), Error> {
    if acknowledged(raw) {
        Ok(())
    } else {
        Err(Error::Api("workflow_feature_not_acknowledged".into()))
    }
}
#[derive(Clone)]
pub struct Request {
    pub(crate) request: PreparedRequest,
}
impl Request {
    pub fn inspect(&self) -> Value {
        let RequestBody::Form(fields) = &self.request.body else {
            unreachable!("closed factory")
        };
        json!({"method":METHOD,"url":self.request.url,
            "fields":fields.iter().filter(|(k,_)| k!="token").collect::<Vec<_>>(),
            "tokenPresent":fields.first().is_some_and(|(k,v)| k=="token"&&!v.is_empty())})
    }
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeatureRequest")
            .field("method", &METHOD)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Acknowledged,
    Rejected,
    Unknown,
    Retired,
    RateLimited,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub number: u8,
    pub status: Option<u16>,
    pub outcome: Outcome,
    pub raw: Option<Value>,
    pub response_text: Option<String>,
    pub retry_after_millis: Option<u64>,
    pub may_have_applied: bool,
}
#[cfg(test)]
mod tests;
