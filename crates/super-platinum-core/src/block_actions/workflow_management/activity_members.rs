//! AjOk Activity member filter: 8Bbq/fRHe/WNgx/euu1, limit20.
//! Observations only; the desktop owns query/visit/credential/native fences.
mod member;
mod observations;
mod ordering;
use crate::block_actions::directory_endpoint::DirectoryEndpoint;
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
pub use member::{Eligibility, Member};
pub use observations::{ObservationPlan, RankInputs, RankObservations, RendererReply};
pub use ordering::{MatchFacts, match_members, rank_members};
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Instant};
pub const LIMIT: usize = 20;
fn invalid() -> Error {
    Error::Api("workflow_activity_member_unavailable".into())
}
fn id(s: &str) -> bool {
    matches!(s, "USLACKBOT" | "USEARCH" | "UCHANNELEXPERT" | "USLACK")
        || (s.len() >= 9
            && s.len() <= 255
            && matches!(s.as_bytes()[0], b'U' | b'W')
            && s.bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}
fn success(raw: &Value) -> Result<(), Error> {
    if raw.is_object()
        && raw.get("ok").is_none_or(|v| v == true)
        && raw.get("error").is_none_or(Value::is_null)
    {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn same(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.enterprise_id == b.enterprise_id
        && a.user_id == b.user_id
        && a.token == b.token
        && a.url == b.url
}
/// The parent chooses this only from observed connection and empty-query policy.
/// Offline uses Local. Missing policy is not authorization for a remote fetch.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchPath {
    Local,
    Remote,
}
#[derive(Clone)]
pub struct Query {
    owner: WorkspaceSession,
    raw: String,
}
impl Query {
    pub fn capture(owner: &WorkspaceSession, raw: &str) -> Result<Self, Error> {
        if !id(&owner.user_id) || raw.len() > 4096 || raw.chars().any(|c| c == '\0') {
            return Err(invalid());
        }
        Ok(Self {
            owner: owner.clone(),
            raw: raw.into(),
        })
    }
    pub fn current(&self, w: &WorkspaceSession) -> bool {
        same(&self.owner, w)
    }
    pub fn raw(&self) -> &str {
        &self.raw
    }
    /// Actual Ojx4: top-level keeps emoji/document triggers; searchLocal applies
    /// a second formatting stage that also strips a leading document trigger.
    pub fn text(&self, path: SearchPath) -> &str {
        let first = self
            .raw
            .strip_prefix(['@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}'])
            .unwrap_or(&self.raw);
        if path == SearchPath::Local {
            first
                .strip_prefix([
                    '@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}', '+', '\u{ff0b}',
                ])
                .unwrap_or(first)
        } else {
            first
        }
    }
    pub fn request(
        &self,
        c: &SlackClient,
        w: &WorkspaceSession,
        endpoint: &DirectoryEndpoint,
        now: Instant,
        marker: Option<&str>,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(w) {
            return Err(invalid());
        }
        let mut body = json!({"query":self.text(SearchPath::Remote),"count":LIMIT,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated","include_profile_only_users":true});
        // includeAgents remains true, so includeBots=false does not emit NOT bots.
        if let Some(marker) = marker {
            if marker.is_empty() || marker.len() > 1024 || marker.chars().any(char::is_control) {
                return Err(invalid());
            }
            body["marker"] = json!(marker);
        }
        endpoint.request(c, w, now, "users/search", body)
    }
}

/// Complete raw response retained separately from flattened source-order rows.
#[derive(Clone)]
pub struct Page {
    raw: Value,
    rows: Vec<Value>,
    marker: Option<String>,
}
impl Page {
    pub fn decode(raw: Value) -> Result<Self, Error> {
        Self::decode_ordered(raw, None)
    }
    /// Object-shaped results require actual Object.keys order captured before
    /// serde_json's map conversion. Never invent order from its sorted keys.
    pub fn decode_ordered(raw: Value, keys: Option<&[String]>) -> Result<Self, Error> {
        success(&raw)?;
        let mut rows = Vec::new();
        match raw.get("results") {
            None => {}
            Some(Value::Array(a)) => rows.extend(a.iter().cloned()),
            Some(Value::Object(o)) => {
                let keys = keys.ok_or_else(invalid)?;
                let unique: BTreeSet<_> = keys.iter().collect();
                if keys.len() != o.len()
                    || unique.len() != keys.len()
                    || keys.iter().any(|key| !o.contains_key(key))
                {
                    return Err(invalid());
                }
                for key in keys {
                    let group = &o[key];
                    match group {
                        Value::Array(a) => rows.extend(a.iter().cloned()),
                        Value::Object(_) => rows.push(group.clone()),
                        _ => return Err(invalid()),
                    }
                }
            }
            _ => return Err(invalid()),
        }
        if rows.len() > 1000
            || rows
                .iter()
                .any(|v| !v.is_object() || v["id"].as_str().is_none_or(|s| !id(s)))
        {
            return Err(invalid());
        }
        let marker = match raw.get("next_marker") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if s.len() <= 1024 && !s.chars().any(char::is_control) => {
                Some(s.clone())
            }
            _ => return Err(invalid()),
        };
        Ok(Self { raw, rows, marker })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn marker(&self) -> Option<&str> {
        self.marker.as_deref()
    }
}
#[derive(Clone, Default)]
pub struct Pages {
    pages: Vec<Page>,
    rows: Vec<Value>,
    marker: Option<String>,
    seen: BTreeSet<String>,
    complete: bool,
}
impl Pages {
    pub fn append(&mut self, page: Page) -> Result<(), Error> {
        if self.complete || self.pages.len() >= 50 {
            return Err(invalid());
        }
        let count = self.rows.len() + page.rows.len();
        let complete = page.marker.is_none() || count >= LIMIT;
        let marker = if count > LIMIT {
            page.rows
                .get(LIMIT - self.rows.len())
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            page.marker.clone()
        };
        if !complete
            && (self.pages.len() == 49 || marker.as_ref().is_some_and(|m| self.seen.contains(m)))
        {
            return Err(invalid());
        }
        if let Some(m) = &marker {
            self.seen.insert(m.clone());
        }
        self.rows
            .extend(page.rows.iter().take(LIMIT - self.rows.len()).cloned());
        self.pages.push(page);
        self.marker = marker;
        self.complete = complete;
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn marker(&self) -> Option<&str> {
        self.marker.as_deref()
    }
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
    pub fn rows(&self) -> Result<&[Value], Error> {
        if self.complete {
            Ok(&self.rows)
        } else {
            Err(invalid())
        }
    }
    /// euu1/An8m normalization only. Caller performs source-equivalent partial
    /// user-store merging and derived-name hydration before matching; no cached
    /// eligibility grant or replacement whole-user assignment is provided here.
    pub fn normalized_rows(&self, normalize_visible_external: bool) -> Result<Vec<Value>, Error> {
        let rows = self.rows()?;
        // Refuse malformed observations before returning anything for the
        // caller's user-store upsert, not only during later Member decoding.
        if rows.iter().any(|row| {
            row.get("is_profile_only_user")
                .is_some_and(|flag| !flag.is_boolean())
        }) {
            return Err(invalid());
        }
        Ok(rows
            .iter()
            .map(|row| {
                let mut row = row.clone();
                if normalize_visible_external && row["is_profile_only_user"] != true {
                    row["is_stranger"] = json!(false);
                }
                row
            })
            .collect())
    }
}
#[cfg(test)]
mod tests;
