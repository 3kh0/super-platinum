//! Historical PlanInfo display data, never a live source or permission witness.
//! n4D5 persists its shallow-merged store; this identity-bound envelope is native.
use crate::config::WorkspaceSession;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanInfoCache {
    version: u32,
    account: String,
    team: String,
    user: String,
    url: String,
    enterprise: Option<String>,
    source: String,
    raw: Value,
}
impl PlanInfoCache {
    /// The caller must have accepted the original current source action first.
    pub fn accepted(account: &str, session: &WorkspaceSession, raw: Value) -> Option<Self> {
        if uuid::Uuid::parse_str(account).is_err()
            || session.team_id.is_empty()
            || session.user_id.is_empty()
            || session.url.is_empty()
            || !raw.is_object()
        {
            return None;
        }
        Some(Self {
            version: 1,
            account: account.into(),
            team: session.team_id.clone(),
            user: session.user_id.clone(),
            url: session.url.clone(),
            enterprise: session.enterprise_id.clone(),
            source: "accepted-plan-info".into(),
            raw,
        })
    }
    pub fn restore(value: &Value, session: &WorkspaceSession) -> Option<Self> {
        let retained: Self = serde_json::from_value(value.clone()).ok()?;
        (retained.valid() && retained.session_matches(session)).then_some(retained)
    }
    fn valid(&self) -> bool {
        self.version == 1
            && uuid::Uuid::parse_str(&self.account).is_ok()
            && !self.team.is_empty()
            && !self.user.is_empty()
            && !self.url.is_empty()
            && self.source == "accepted-plan-info"
            && self.raw.is_object()
    }
    fn session_matches(&self, session: &WorkspaceSession) -> bool {
        self.team == session.team_id
            && self.user == session.user_id
            && self.url == session.url
            && self.enterprise == session.enterprise_id
    }
    pub fn matches(&self, account: &str, session: &WorkspaceSession) -> bool {
        self.valid() && self.account == account && self.session_matches(session)
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
