use super::*;
use serde_json::json;
const PERMISSION: &str = "DELETE_OWN_MESSAGE";

/// Original permissions/info read scope. This is not deletion authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnDeletePermissionQuery {
    workspace: String,
    user: String,
    channel: String,
    as_admin: bool,
    enterprise_scope: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnDeletePermission {
    query: OwnDeletePermissionQuery,
    raw: Value,
    allowed: bool,
}
impl OwnDeletePermissionQuery {
    pub fn capture(
        workspace: &str,
        user: &str,
        channel: &str,
        as_admin: bool,
    ) -> Result<Self, ListError> {
        let value = Self {
            workspace: workspace.into(),
            user: user.into(),
            channel: channel.into(),
            as_admin,
            enterprise_scope: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), ListError> {
        if !identity(&self.workspace)
            || !identity(&self.user)
            || !identity(&self.channel)
            || self
                .enterprise_scope
                .as_deref()
                .is_some_and(|v| !identity(v))
        {
            return Err(ListError(
                "The own-delete permission identity is unavailable.",
            ));
        }
        Ok(())
    }
    /// Session routing custody, deliberately absent from the permission body.
    pub fn with_enterprise_scope(mut self, scope: Option<&str>) -> Result<Self, ListError> {
        self.enterprise_scope = scope.map(str::to_owned);
        self.validate()?;
        Ok(self)
    }
    pub fn enterprise_scope(&self) -> Option<&str> {
        self.enterprise_scope.as_deref()
    }
    pub fn edge_scope(&self) -> &str {
        self.enterprise_scope().unwrap_or(&self.workspace)
    }
    pub fn body(&self) -> Value {
        let mut value =
            json!({"user_id":self.user,"channel_id":self.channel,"permissions":[PERMISSION]});
        if self.as_admin {
            value["as_admin"] = json!(true);
        }
        value
    }
    pub fn accept(self, raw: Value) -> Result<OwnDeletePermission, ListError> {
        self.validate()?;
        let allowed = raw["allowed"]
            .as_array()
            .filter(|a| a.len() <= 1024 && a.iter().all(|v| v.as_str().is_some_and(identity)))
            .ok_or(ListError(
                "The own-delete permission response is unavailable.",
            ))?;
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(ListError("The own-delete permission was not confirmed."));
        }
        let allowed = allowed.iter().any(|v| v == PERMISSION);
        Ok(OwnDeletePermission {
            query: self,
            raw,
            allowed,
        })
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
}
impl OwnDeletePermission {
    pub fn query(&self) -> &OwnDeletePermissionQuery {
        &self.query
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn allowed(&self) -> bool {
        self.allowed
    }
}

/// Positive current host observations for retained DoBV/pS2E/pLOx selectors.
/// No missing permissions, edit window or composer mode defaults are supplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnDeletePolicy {
    workspace: String,
    user: String,
    channel: Value,
    can_thread: bool,
    read_only_file_thread: bool,
    author_external: bool,
    permission: OwnDeletePermission,
}
impl OwnDeletePolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn observed(
        workspace: &str,
        user: &str,
        channel: Value,
        can_thread: bool,
        read_only_file_thread: bool,
        author_external: bool,
        permission: &OwnDeletePermission,
    ) -> Result<Self, ListError> {
        if permission.query.workspace() != workspace
            || permission.query.user() != user
            || channel["id"].as_str() != Some(permission.query.channel())
            || !channel.is_object()
            || ["isUnknown", "isNonExistent"]
                .iter()
                .any(|k| source::truthy(channel.get(*k)))
        {
            return Err(ListError(
                "The own-delete permission belongs to another owner or channel.",
            ));
        }
        Ok(Self {
            workspace: workspace.into(),
            user: user.into(),
            channel,
            can_thread,
            read_only_file_thread,
            author_external,
            permission: permission.clone(),
        })
    }
    pub fn channel_raw(&self) -> &Value {
        &self.channel
    }
    pub fn permission(&self) -> &OwnDeletePermission {
        &self.permission
    }
    pub(super) fn admit(
        &self,
        workspace: &str,
        user: &str,
        target: &Target,
    ) -> Result<(), ListError> {
        target.validate()?;
        if self.workspace != workspace
            || self.user != user
            || self.channel["id"].as_str() != Some(target.channel_id())
            || !self.permission.allowed
            || !self.can_thread
            || self.read_only_file_thread
            || source::truthy(self.channel.get("is_non_threadable"))
            || (source::truthy(self.channel.get("is_ext_shared")) && self.author_external)
            || target.document_raw()["filetype"] == "thread_share"
            || target.document_raw()["mimetype"] == "application/vnd.slack-thread-share"
        {
            return Err(ListError(
                "The current policy does not permit this own comment deletion.",
            ));
        }
        Ok(())
    }
}
