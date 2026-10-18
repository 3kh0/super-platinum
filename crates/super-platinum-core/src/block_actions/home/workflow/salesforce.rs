//! Source-backed Salesforce workflow authentication, separate from generic OAuth.
use super::*;
mod context;
mod events;
pub mod native;
pub use context::Context;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListReason {
    Initial,
    ForcedRefetch,
    ManualRecheck,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    List(ListReason),
    Silent,
    Manual,
    LoginUrl,
    Disconnect,
}
#[derive(Clone, PartialEq)]
pub struct Target {
    workflow: Workflow,
    auth: Auth,
    context: Context,
    org: String,
}
#[derive(Clone, PartialEq)]
pub struct Org {
    raw: Value,
    id: String,
    pub name: String,
    pub instance_url: String,
    pub username: String,
}
#[derive(Clone, PartialEq)]
pub struct OrgSnapshot {
    raw: Value,
    pub target: Option<Org>,
    owner: Target,
}
#[derive(Clone, PartialEq)]
pub enum AuthOutcome {
    CheckOrganization,
    LoginUrlRequired,
    OpenExternal(String),
}

impl Target {
    pub fn capture(
        review: &Preflight,
        current: &View,
        w: &WorkspaceSession,
        function: &str,
        provider: &str,
        context: &Context,
    ) -> Result<Self, Error> {
        if !review.workflow().same_view(current) || review.workflow().user != w.user_id {
            return Err(invalid());
        }
        let auth = review
            .connections()
            .iter()
            .find(|a| a.function == function && a.provider == provider)
            .ok_or_else(invalid)?;
        if !context.selected(auth) {
            return Err(invalid());
        }
        let org = auth
            .salesforce_org
            .as_ref()
            .filter(|s| !s.is_empty() && s.len() <= 255 && !s.chars().any(char::is_control))
            .ok_or_else(invalid)?
            .clone();
        Ok(Self {
            workflow: review.workflow().clone(),
            auth: auth.clone(),
            context: context.clone(),
            org,
        })
    }
    pub fn org(&self) -> &str {
        &self.org
    }
    pub fn function(&self) -> &str {
        &self.auth.function
    }
    pub fn provider(&self) -> &str {
        &self.auth.provider
    }
    pub fn jwt(&self) -> bool {
        self.context.jwt()
    }
    pub fn same_observations(&self, context: &Context) -> bool {
        &self.context == context
    }
    /// Token list updates do not replace the organization/function whose
    /// uncertain connection action is being explicitly reviewed.
    pub fn same_connection(&self, other: &Self) -> bool {
        self.workflow == other.workflow
            && self.context == other.context
            && self.org == other.org
            && self.auth.app == other.auth.app
            && self.auth.function == other.auth.function
            && self.auth.provider == other.auth.provider
    }
    fn current(&self, current: &View, w: &WorkspaceSession) -> bool {
        self.workflow.same_view(current) && self.workflow.user == w.user_id
    }
    pub fn request(
        &self,
        action: Action,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(current, w) {
            return Err(invalid());
        }
        let (method, reason, org, force) = match action {
            Action::List(ListReason::Initial) => (
                "sfdc.integration.listOrgs",
                "fetch-orgs-and-sign-in-silently-v2",
                false,
                false,
            ),
            Action::List(ListReason::ForcedRefetch) => (
                "sfdc.integration.listOrgs",
                "fetch-orgs-and-sign-in-silently-v2-refetch",
                false,
                false,
            ),
            Action::List(ListReason::ManualRecheck) => (
                "sfdc.integration.listOrgs",
                "use-sign-into-salesforce-org",
                false,
                false,
            ),
            Action::Silent => (
                "sfdc.integration.authUser",
                "fetch-orgs-and-sign-in-silently-v2",
                true,
                true,
            ),
            Action::Manual if !self.jwt() => (
                "sfdc.integration.authUser",
                "use-sign-into-salesforce-org",
                true,
                false,
            ),
            Action::LoginUrl => (
                "sfdc.integration.auth.loginUrl",
                "use-sign-into-salesforce-org",
                true,
                false,
            ),
            Action::Disconnect => (
                "sfdc.integration.disconnectUser",
                "use-sign-out-of-salesforce-org",
                true,
                false,
            ),
            _ => return Err(invalid()),
        };
        let mut fields = vec![("_x_reason", reason.into())];
        if org {
            fields.push(("salesforce_org_id", self.org.clone()));
        }
        if force {
            fields.push(("force_reauth", "true".into()));
        }
        Ok(client.rest_form(w, method, fields))
    }
    pub fn organizations(&self, raw: Value) -> Result<OrgSnapshot, Error> {
        if raw["ok"] != true || raw.to_string().len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
        let rows = raw["salesforce_orgs"]
            .as_array()
            .filter(|r| r.len() <= 1000)
            .ok_or_else(invalid)?;
        let mut matched = rows
            .iter()
            .filter(|r| r["salesforce_org_id"].as_str() == Some(&self.org));
        let target = matched.next().map(Org::decode).transpose()?;
        if matched.next().is_some() {
            return Err(invalid());
        }
        Ok(OrgSnapshot {
            raw,
            target,
            owner: self.clone(),
        })
    }
    pub fn manual_response(&self, action: Action, raw: &Value) -> Result<AuthOutcome, Error> {
        if raw["ok"] != true {
            return Err(invalid());
        }
        match action {
            Action::LoginUrl => match raw.get("auth_url") {
                None | Some(Value::Null) => Ok(AuthOutcome::CheckOrganization),
                Some(Value::String(value)) if value.is_empty() => {
                    Ok(AuthOutcome::CheckOrganization)
                }
                Some(_) => Ok(AuthOutcome::OpenExternal(external_url(raw, "auth_url")?)),
            },
            Action::Manual if !self.jwt() => {
                if manual_needed(raw)? {
                    return Ok(AuthOutcome::LoginUrlRequired);
                }
                if has_authorization_url(raw)? {
                    Ok(AuthOutcome::OpenExternal(external_url(
                        raw,
                        "authorization_url",
                    )?))
                } else {
                    Ok(AuthOutcome::CheckOrganization)
                }
            }
            _ => Err(invalid()),
        }
    }
    pub fn silent_succeeded(&self, raw: &Value) -> Result<bool, Error> {
        if raw["ok"] != true {
            return Err(invalid());
        }
        // WgYR: JWT uses is_manual_auth_needed only; old flow also checks URL.
        Ok(!manual_needed(raw)? && (self.jwt() || !has_authorization_url(raw)?))
    }
    pub fn accepts(&self, snapshot: &OrgSnapshot) -> bool {
        &snapshot.owner == self
    }
}
impl Org {
    fn decode(raw: &Value) -> Result<Self, Error> {
        let text = |key| match raw.get(key) {
            None | Some(Value::Null) => Ok(String::new()),
            Some(Value::String(s)) if s.len() <= 8192 => Ok(s.clone()),
            _ => Err(invalid()),
        };
        let id = bounded(raw, "salesforce_org_id", 255)?;
        if !matches!(
            raw["auth"].as_str(),
            Some("ok" | "none" | "error" | "auto_auth_error")
        ) {
            return Err(invalid());
        }
        Ok(Self {
            raw: raw.clone(),
            id,
            name: text("name")?,
            instance_url: text("instance_url")?,
            username: text("username")?,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn connected(&self) -> bool {
        self.raw["auth"] == "ok"
    }
    pub fn silent_candidate(&self) -> bool {
        self.raw["auth"] == "none"
            || self.raw["auth"] == "auto_auth_error"
                && self.raw["auth_failure_reason"] != "user_mapping_disabled"
    }
    pub fn failure_reason(&self) -> Option<&str> {
        self.raw["auth_failure_reason"].as_str()
    }
}
impl OrgSnapshot {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
fn external_url(raw: &Value, key: &str) -> Result<String, Error> {
    let value = bounded(raw, key, 8192)?;
    let parsed = url::Url::parse(&value).map_err(|_| invalid())?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(invalid());
    }
    Ok(value)
}
fn manual_needed(raw: &Value) -> Result<bool, Error> {
    match raw.get("is_manual_auth_needed") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(invalid()),
    }
}
fn has_authorization_url(raw: &Value) -> Result<bool, Error> {
    match raw.get("authorization_url") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::String(value)) => Ok(!value.is_empty()),
        _ => Err(invalid()),
    }
}
