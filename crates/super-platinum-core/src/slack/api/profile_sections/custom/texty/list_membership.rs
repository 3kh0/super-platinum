//! List Texty membership observations, from wfd7/mVvn/pfZb/9UBA/DMmC.
//! Account/channel/read lifetime, batching, throttling and async retirement belong
//! to the caller. An unknown observation is not a known nonmember or empty list.
use crate::{config::WorkspaceSession, slack::Error};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod tests;

pub const FIRST_MEMBERS: usize = 30;
pub const MEMBERSHIP_LIMIT: usize = 100;
pub const ATTEMPT_INTERVAL_MILLIS: u64 = 43_200_000;

fn unavailable() -> Error {
    Error::Api("list_member_membership_unavailable".into())
}
fn channel_id(id: &str) -> bool {
    id.len() >= 9
        && id.len() <= 255
        && matches!(id.as_bytes()[0], b'C' | b'G' | b'D' | b'Z')
        && id
            .bytes()
            .skip(1)
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn user_id(id: &str) -> bool {
    crate::block_actions::form::valid_message_user(id) || id == "USLACK"
}
fn owner_equal(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.enterprise_id == b.enterprise_id
        && a.user_id == b.user_id
        && a.token == b.token
        && a.url == b.url
        && a.name == b.name
}
pub(crate) fn valid_wire_body(method: &str, body: &Value) -> bool {
    let Some(original) = body.as_object() else {
        return false;
    };
    if !body["token"].as_str().is_some_and(|s| !s.is_empty())
        || !body
            .get("enterprise_token")
            .is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty()))
    {
        return false;
    }
    let expected = match method {
        "users/list" => {
            let Some(channels) = body["channels"].as_array().filter(|v| v.len() == 1) else {
                return false;
            };
            let Some(channel) = channels[0].as_str().filter(|v| channel_id(v)) else {
                return false;
            };
            json!({"channels":[channel],"present_first":true,"filter":"everyone AND NOT bots AND NOT apps","count":FIRST_MEMBERS})
        }
        "channels/membership" => {
            let Some(channel) = body["channel"].as_str().filter(|v| channel_id(v)) else {
                return false;
            };
            let Some(users) = body["users"]
                .as_array()
                .and_then(|a| a.iter().map(Value::as_str).collect::<Option<Vec<_>>>())
            else {
                return false;
            };
            if users.is_empty()
                || users.len() > MEMBERSHIP_LIMIT
                || !users.iter().all(|v| user_id(v))
                || users.iter().collect::<BTreeSet<_>>().len() != users.len()
            {
                return false;
            }
            json!({"channel":channel,"users":users,"as_admin":false})
        }
        _ => return false,
    };
    let mut fields = original.clone();
    fields.remove("token");
    fields.remove("enterprise_token");
    Value::Object(fields) == expected
}
#[derive(Clone, PartialEq, Eq)]
enum Kind {
    First,
    Membership(Vec<String>),
}
/// Original request scope. No Debug: credentials remain private.
#[derive(Clone)]
pub struct Read {
    owner: WorkspaceSession,
    channel: String,
    kind: Kind,
}
impl Read {
    pub fn first_members(owner: &WorkspaceSession, channel: &str) -> Result<Self, Error> {
        if !channel_id(channel)
            || owner.team_id.is_empty()
            || owner.user_id.is_empty()
            || owner.token.is_empty()
        {
            return Err(unavailable());
        }
        Ok(Self {
            owner: owner.clone(),
            channel: channel.into(),
            kind: Kind::First,
        })
    }
    /// One already deduplicated chunk. Empty lists need no request; callers may
    /// preserve an empty first-N result without claiming known-empty membership.
    pub fn membership(
        owner: &WorkspaceSession,
        channel: &str,
        users: Vec<String>,
    ) -> Result<Self, Error> {
        let mut read = Self::first_members(owner, channel)?;
        if users.is_empty()
            || users.len() > MEMBERSHIP_LIMIT
            || users.iter().collect::<BTreeSet<_>>().len() != users.len()
            || !users.iter().all(|id| user_id(id))
        {
            return Err(unavailable());
        }
        read.kind = Kind::Membership(users);
        Ok(read)
    }
    pub fn current(&self, owner: &WorkspaceSession) -> bool {
        owner_equal(&self.owner, owner)
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn users(&self) -> &[String] {
        match &self.kind {
            Kind::First => &[],
            Kind::Membership(ids) => ids,
        }
    }
    pub fn method(&self) -> &'static str {
        match self.kind {
            Kind::First => "users/list",
            Kind::Membership(_) => "channels/membership",
        }
    }
    pub fn body(&self) -> Value {
        match &self.kind {
            Kind::First => json!({"channels":[self.channel],"present_first":true,
                "filter":"everyone AND NOT bots AND NOT apps","count":FIRST_MEMBERS}),
            Kind::Membership(ids) => json!({"channel":self.channel,"users":ids,"as_admin":false}),
        }
    }
}

/// First-N is a bounded acquisition hint, never a complete membership census.
pub struct FirstMembers {
    read: Read,
    page: super::channels::Page,
    ids: Vec<String>,
}
impl FirstMembers {
    pub fn decode(
        read: &Read,
        owner: &WorkspaceSession,
        raw: Value,
        keys: Option<&[String]>,
    ) -> Result<Self, Error> {
        if !read.current(owner) || read.kind != Kind::First || raw.get("results").is_none() {
            return Err(unavailable());
        }
        let page = super::channels::Page::decode_ordered(raw, keys)?;
        if page.rows().len() > FIRST_MEMBERS {
            return Err(unavailable());
        }
        let mut ids = Vec::new();
        let mut seen = BTreeSet::new();
        for row in page.rows() {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| user_id(id))
                .ok_or_else(unavailable)?;
            if seen.insert(id.to_owned()) {
                ids.push(id.to_owned());
            }
        }
        Ok(Self {
            read: read.clone(),
            page,
            ids,
        })
    }
    pub fn channel(&self) -> &str {
        self.read.channel()
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn rows(&self) -> &[Value] {
        self.page.rows()
    }
    pub fn raw(&self) -> &Value {
        self.page.raw()
    }
    pub fn outcomes(&self) -> &super::channels::Outcomes {
        self.page.outcomes()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Unknown,
    Known(bool),
}
impl Entry {
    pub fn bonus(self) -> bool {
        self == Self::Known(true)
    }
}
/// A decoded response remains tied to its original exact request, not caller
/// supplied booleans. Foreign, overlapping and incomplete partitions refuse.
pub struct Membership {
    read: Read,
    raw: Value,
    values: BTreeMap<String, bool>,
}
impl Membership {
    pub fn decode(read: &Read, owner: &WorkspaceSession, raw: Value) -> Result<Self, Error> {
        if !read.current(owner)
            || !matches!(read.kind, Kind::Membership(_))
            || !raw.is_object()
            || raw.get("ok").is_some_and(|v| v != &Value::Bool(true))
            || raw.get("error").is_some_and(|v| !v.is_null())
            || raw
                .get("channel")
                .is_some_and(|v| v.as_str() != Some(read.channel()))
        {
            return Err(unavailable());
        }
        let requested: BTreeSet<_> = read.users().iter().map(String::as_str).collect();
        let mut values = BTreeMap::new();
        for (key, member) in [("members", true), ("non_members", false)] {
            let rows = match raw.get(key) {
                None | Some(Value::Null) => &[][..],
                Some(Value::Array(rows)) => rows.as_slice(),
                _ => return Err(unavailable()),
            };
            for value in rows {
                let id = value
                    .as_str()
                    .filter(|id| requested.contains(id))
                    .ok_or_else(unavailable)?;
                if let Some(prior) = values.insert(id.to_owned(), member)
                    && prior != member
                {
                    return Err(unavailable());
                }
            }
        }
        if values.len() != requested.len() {
            return Err(unavailable());
        }
        Ok(Self {
            read: read.clone(),
            raw,
            values,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn channel(&self) -> &str {
        self.read.channel()
    }
    pub fn values(&self) -> &BTreeMap<String, bool> {
        &self.values
    }
}

/// One explicitly initialized owner/channel cache. Missing and pending both
/// expose Unknown, while the internal presence bit guards cleared late results.
#[derive(Clone)]
pub struct Cache {
    owner: WorkspaceSession,
    channel: String,
    entries: BTreeMap<String, Entry>,
}
impl Cache {
    pub fn capture(owner: &WorkspaceSession, channel: &str) -> Result<Self, Error> {
        let read = Read::first_members(owner, channel)?;
        Ok(Self {
            owner: read.owner,
            channel: read.channel,
            entries: BTreeMap::new(),
        })
    }
    pub fn current(&self, owner: &WorkspaceSession, channel: &str) -> bool {
        owner_equal(&self.owner, owner) && self.channel == channel
    }
    fn admits(&self, read: &Read) -> bool {
        self.current(&read.owner, read.channel()) && matches!(read.kind, Kind::Membership(_))
    }
    pub fn entry(&self, user: &str) -> Entry {
        self.entries.get(user).copied().unwrap_or(Entry::Unknown)
    }
    pub fn bonus(&self, user: &str) -> bool {
        self.entry(user).bonus()
    }
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.entries
    }
    pub fn begin(&mut self, read: &Read) -> Result<(), Error> {
        if !self.admits(read) {
            return Err(unavailable());
        }
        for id in read.users() {
            self.entries.insert(id.clone(), Entry::Unknown);
        }
        Ok(())
    }
    /// Call only after the caller has revalidated the original async read owner.
    pub fn accept(&mut self, response: &Membership) -> Result<(), Error> {
        if !self.admits(&response.read) {
            return Err(unavailable());
        }
        for (id, value) in &response.values {
            if let Some(entry) = self.entries.get_mut(id) {
                *entry = Entry::Known(*value);
            }
        }
        Ok(())
    }
    /// Failure/retirement clears pending IDs, never turns them into nonmembers.
    pub fn clear(&mut self, read: &Read) -> Result<(), Error> {
        if !self.admits(read) {
            return Err(unavailable());
        }
        for id in read.users() {
            self.entries.remove(id);
        }
        Ok(())
    }
}
