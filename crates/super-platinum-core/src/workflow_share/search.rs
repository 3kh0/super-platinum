//! rfHj multi-select Share preset. Acquisition custody, not eligibility/ranking.
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::{Value, json};
use std::{sync::OnceLock, time::Instant};
mod pages;
pub mod ranking;
mod routing;
pub use crate::slack::api::custom::texty::channels::{Outcomes, Page};
pub use pages::Pages;
pub use routing::AcquisitionRoute;
#[cfg(test)]
mod tests;
pub const LIMIT: usize = 24;
fn invalid() -> Error {
    Error::Api("workflow_share_search_invalid".into())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Members,
    Channels,
}
impl Kind {
    pub fn method(self) -> &'static str {
        match self {
            Self::Members => "users/search",
            Self::Channels => "channels/search",
        }
    }
}
#[derive(Clone)]
pub struct Query {
    raw: String,
    parsed: String,
    wire: String,
    kinds: Vec<Kind>,
}
impl Query {
    pub fn new(raw: &str) -> Result<Self, Error> {
        if raw.len() > 1_048_576 {
            return Err(invalid());
        }
        // Exact difI.gy /i expression used by mLHg.parseQueryForSearch.
        // ASCII folding preserves byte offsets and JS non-u ASCII /i semantics.
        static EMAIL: OnceLock<regress::Regex> = OnceLock::new();
        let email=EMAIL.get_or_init(|| regress::Regex::new(r"[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*@(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?").expect("retained difI expression"));
        let mut folded = raw.to_owned();
        folded.make_ascii_lowercase();
        let parsed = email
            .find(&folded)
            .map(|m| &raw[m.range()])
            .unwrap_or(raw)
            .to_owned();
        let first = parsed.chars().next();
        let kinds = match first {
            Some('@' | '\u{ff20}') => vec![Kind::Members],
            Some('#' | '\u{ff03}') => vec![Kind::Channels],
            _ => vec![Kind::Members, Kind::Channels],
        };
        let wire = if matches!(
            first,
            Some('@' | '\u{ff20}' | '#' | '\u{ff03}' | '/' | '\u{ff0f}')
        ) {
            parsed[first.unwrap().len_utf8()..].to_owned()
        } else {
            parsed.clone()
        };
        Ok(Self {
            raw: raw.into(),
            parsed,
            wire,
            kinds,
        })
    }
    pub fn raw(&self) -> &str {
        &self.raw
    }
    pub fn parsed(&self) -> &str {
        &self.parsed
    }
    pub fn wire(&self) -> &str {
        &self.wire
    }
    pub fn kinds(&self) -> &[Kind] {
        &self.kinds
    }
    pub fn includes(&self, kind: Kind) -> bool {
        self.kinds.contains(&kind)
    }
}
#[derive(Clone)]
pub struct Read {
    owner: WorkspaceSession,
    kind: Kind,
    body: Value,
}
impl Read {
    pub fn new(
        owner: &WorkspaceSession,
        kind: Kind,
        query: &Query,
        marker: Option<&str>,
    ) -> Result<Self, Error> {
        if !query.includes(kind)
            || owner.team_id.is_empty()
            || owner.user_id.is_empty()
            || owner.token.is_empty()
            || marker
                .is_some_and(|m| m.is_empty() || m.len() > 4096 || m.chars().any(char::is_control))
        {
            return Err(invalid());
        }
        let mut body = match kind {
            Kind::Members => {
                json!({"query":query.wire(),"count":LIMIT,"fuzz":1,"enable_workspace_ranking":true,"search_email":true,"filter":"NOT deactivated","include_profile_only_users":true})
            }
            Kind::Channels => {
                json!({"query":query.wire(),"count":LIMIT,"fuzz":1,"filter":"xws","include_record_channels":true,"check_membership":true})
            }
        };
        if let Some(m) = marker {
            body["marker"] = json!(m);
        }
        Ok(Self {
            owner: owner.clone(),
            kind,
            body,
        })
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn method(&self) -> &'static str {
        self.kind.method()
    }
    /// Source tracing reason; EVxR does not place this in Flannel JSON args.
    pub fn reason(&self) -> &'static str {
        match self.kind {
            Kind::Members => "flannel-call:users/search",
            Kind::Channels => "flannel-call:channels/search",
        }
    }
    pub fn body(&self) -> &Value {
        &self.body
    }
    pub fn current(&self, w: &WorkspaceSession) -> bool {
        self.owner.team_id == w.team_id
            && self.owner.user_id == w.user_id
            && self.owner.enterprise_id == w.enterprise_id
            && self.owner.token == w.token
            && self.owner.url == w.url
            && self.owner.name == w.name
    }
    pub fn request(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
        endpoint: &DirectoryEndpoint,
        now: Instant,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner) {
            return Err(invalid());
        }
        endpoint.request(client, owner, now, self.method(), self.body.clone())
    }
}
