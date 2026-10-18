//! Ordered client.init/client.extras adoption, independent of userBoot.
//! The caller must consume its Refresh publication permit before dispatch and
//! settle it immediately afterwards, with no await. This module cannot grant
//! freshness, advance a major, or replace an owner/refresh token.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod dispatch;
mod prefs;
mod roles;
mod wire;
pub use prefs::Stores;

/// ECMAScript Number coercion used by the source temporary-mute timer.
/// Callers retain actual preference/clock observations; NaN remains NaN and
/// does not authorize a timer or fabricate an expiration.
pub fn temp_mute_number(value: &Value) -> Result<f64, Error> {
    prefs::number(value)
}

/// Original account/credential/epoch capture. Intentionally not Debug.
#[derive(Clone)]
pub struct Owner {
    account: String,
    session: crate::config::WorkspaceSession,
    cookie: String,
    epoch: u64,
}
impl Owner {
    pub fn capture(
        account: &str,
        session: &crate::config::WorkspaceSession,
        cookie: &str,
        epoch: u64,
    ) -> Self {
        Self {
            account: account.into(),
            session: session.clone(),
            cookie: cookie.into(),
            epoch,
        }
    }
    pub fn matches(
        &self,
        account: &str,
        session: &crate::config::WorkspaceSession,
        cookie: &str,
        epoch: u64,
    ) -> bool {
        self.account == account
            && self.epoch == epoch
            && self.session.team_id == session.team_id
            && self.session.user_id == session.user_id
            && self.session.token == session.token
            && self.cookie == cookie
            && self.session.url == session.url
            && self.session.enterprise_id == session.enterprise_id
    }
}

/// Actual current selector observations. None is unknown, never a fabricated
/// empty store. They are read again after earlier synchronous effects.
#[derive(Clone, Default)]
pub struct Facts {
    pub team: Option<String>,
    pub enterprise: Option<String>,
    pub workspace_ids: Option<Vec<String>>,
    pub auth_workspace_ids: Option<Vec<String>>,
    pub temp_mute_enabled: Option<bool>,
    pub unix_seconds: Option<f64>,
    pub screen_reader_enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    SelfId,
    ScreenReaderEnabled,
    TeamUpsert,
    MemberUpsert,
    MemberPrimaryOwner,
    MemberOwner,
    MemberAdmin,
    MemberRoleClear,
    PrimaryOwner,
    Owner,
    Admin,
    AccountTypes,
    EnterprisePrimaryOwner,
    EnterpriseOwner,
    EnterpriseAdmin,
    ResetCommands,
    InternalWorkspaces,
    ForceAuthAtStartup,
    UserPrefs,
    HiddenUsers,
    MutedChannels,
    TempMuteTimer,
    OnboardingData,
    OnboardingChannel,
    AcquisitionSource,
    UserDateCreated,
    RedirectSlackbot,
    EligibleBigMode,
    RedirectBigMode,
    SlackbotPermission,
    SlackbotChannel,
    FeatureUsage,
    SalesforceFeatures,
    TeamPrefs,
    PlanInfo,
    TeamClassification,
    CreatorEmailType,
    Misc,
    SsoRequired,
    TransferLinks,
    Subteams,
    EmojiReload,
    AppCommandsReload,
    PaidFeatures,
    EnterpriseSearch,
    InvitedGlowUp,
    SkillsAnnouncement,
    SkillsDayOne,
    DefaultWorkspace,
    SalesforceNux,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub kind: Kind,
    pub payload: Value,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Malformed(&'static str),
    Unavailable(&'static str),
    Superseded,
    Unsupported(Kind),
    Adoption,
}

/// Actual serial mutation boundary. apply must synchronously execute the
/// effect, or return Unsupported/Adoption. Enqueuing a source fire-and-forget
/// reload is sufficient only for that reload action; no dropped action counts
/// as success. current includes the original root token and native store
/// witnesses in addition to Owner::matches.
pub trait Adopter {
    fn current(&self, owner: &Owner) -> bool;
    fn facts(&self) -> Facts;
    fn apply(&mut self, effect: &Effect) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadKind {
    Init,
    Extras,
}
/// Raw response bytes and parsed encounter order are retained separately.
pub struct Payload {
    kind: PayloadKind,
    owner: Owner,
    bytes: String,
    raw: wire::Node,
}
pub struct Settlement {
    payload: Payload,
    applied: Vec<Effect>,
    failed: Option<Effect>,
    result: Result<(), Error>,
}
impl Settlement {
    pub fn original_bytes(&self) -> &str {
        &self.payload.bytes
    }
    pub fn kind(&self) -> PayloadKind {
        self.payload.kind
    }
    pub fn applied(&self) -> &[Effect] {
        &self.applied
    }
    pub fn failed_effect(&self) -> Option<&Effect> {
        self.failed.as_ref()
    }
    pub fn result(&self) -> &Result<(), Error> {
        &self.result
    }
    pub fn is_fatal(&self) -> bool {
        self.payload.kind == PayloadKind::Init && self.result.is_err()
    }
}
impl Payload {
    pub fn observed(
        kind: PayloadKind,
        owner: Owner,
        bytes: String,
    ) -> Result<Self, (Error, String)> {
        let raw: wire::Node = match serde_json::from_str(&bytes) {
            Ok(v) => v,
            Err(_) => return Err((Error::Malformed("JSON"), bytes)),
        };
        let v = raw.value();
        if !v.is_object()
            || v.get("ok") != Some(&json!(true))
            || v.get("error").is_some_and(|v| !v.is_null())
        {
            return Err((Error::Malformed("positive response"), bytes));
        }
        Ok(Self {
            kind,
            owner,
            bytes,
            raw,
        })
    }
    /// Consumes this payload exactly once. There is no replay/rollback on a
    /// later error: Settlement retains earlier applied actions and the failed
    /// action alongside the entire original response.
    pub fn dispatch(self, adopter: &mut impl Adopter) -> Settlement {
        let mut run = Run {
            adopter,
            owner: &self.owner,
            applied: Vec::new(),
            failed: None,
        };
        let result = if !run.adopter.current(run.owner) {
            Err(Error::Superseded)
        } else {
            match self.kind {
                PayloadKind::Init => dispatch::init(&mut run, &self.raw),
                PayloadKind::Extras => dispatch::extras(&mut run, &self.raw),
            }
        };
        let applied = std::mem::take(&mut run.applied);
        let failed = run.failed.take();
        Settlement {
            payload: self,
            applied,
            failed,
            result,
        }
    }
}
struct Run<'a, A> {
    adopter: &'a mut A,
    owner: &'a Owner,
    applied: Vec<Effect>,
    failed: Option<Effect>,
}
impl<A: Adopter> Run<'_, A> {
    fn emit(&mut self, kind: Kind, payload: Value) -> Result<(), Error> {
        if !self.adopter.current(self.owner) {
            return Err(Error::Superseded);
        }
        let effect = Effect { kind, payload };
        match self.adopter.apply(&effect) {
            Ok(()) => {
                self.applied.push(effect);
                Ok(())
            }
            Err(error) => {
                self.failed = Some(effect);
                Err(error)
            }
        }
    }
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
#[cfg(test)]
mod tests;
