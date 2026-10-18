//! D1NF/RxWg metadata, q/ZW + RWbG external auth, and GGBU event ownership.
use super::*;

#[derive(Clone, PartialEq)]
pub struct Metadata {
    workflow: Workflow,
    raw: Value,
    workflow_id: String,
    app_id: String,
    pub title: String,
    pub owning_team: String,
}
impl Workflow {
    pub fn metadata_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
    ) -> Result<PreparedRequest, Error> {
        if self.user != w.user_id || !self.same_view(current) {
            return Err(invalid());
        }
        Ok(client.rest_form(
            w,
            "workflows.triggers.info",
            vec![
                ("trigger_id", self.trigger.clone()),
                ("include_workflow_details", "true".into()),
                ("_x_reason", "hermes_triggers_ufm".into()),
            ],
        ))
    }
    pub fn metadata(&self, response: Value) -> Result<Metadata, Error> {
        if response["ok"] != true
            || response["trigger"]["id"].as_str() != Some(&self.trigger)
            || response.to_string().len() > 2 * 1024 * 1024
        {
            return Err(invalid());
        }
        let trigger = &response["trigger"];
        let workflow_id = bounded(&trigger["workflow"], "workflow_id", 255)?;
        let app_id = trigger["workflow"]
            .get("app_id")
            .or_else(|| trigger["workflow"]["app"].get("id"))
            .and_then(Value::as_str)
            .filter(|s| id(s))
            .ok_or_else(invalid)?
            .to_owned();
        let owning_team = bounded(trigger, "owning_team_id", 255)?;
        if !id(&workflow_id) || !id(&owning_team) {
            return Err(invalid());
        }
        let title = trigger
            .get("name")
            .or_else(|| trigger["workflow"].get("title"))
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 8192)
            .unwrap_or(self.label())
            .to_owned();
        Ok(Metadata {
            workflow: self.clone(),
            raw: response,
            workflow_id,
            app_id,
            title,
            owning_team,
        })
    }
}
impl Metadata {
    pub fn app(&self) -> &str {
        &self.app_id
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
impl Preflight {
    fn authentication<'a>(
        &'a self,
        metadata: &Metadata,
        current: &View,
        w: &WorkspaceSession,
        function: &str,
        provider: &str,
    ) -> Result<&'a Auth, Error> {
        if metadata.workflow != self.workflow
            || !self.workflow.same_view(current)
            || self.workflow.user != w.user_id
        {
            return Err(invalid());
        }
        self.auth
            .iter()
            .find(|a| a.function == function && a.provider == provider)
            .ok_or_else(invalid)
    }
    pub fn connect_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        metadata: &Metadata,
        function: &str,
        provider: &str,
    ) -> Result<PreparedRequest, Error> {
        self.connect_request_with_context(client, w, current, metadata, function, provider, None)
    }
    /// 2rg7 selects Salesforce only for Add, from the observed feature/app
    /// context. Retained-token selection continues through ordinary mapping.
    pub fn connect_request_with_context(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        metadata: &Metadata,
        function: &str,
        provider: &str,
        context: Option<&salesforce::Context>,
    ) -> Result<PreparedRequest, Error> {
        let auth = self.authentication(metadata, current, w, function, provider)?;
        if matches!(
            auth.app.as_str(),
            "A04T99UKKQE" | "A014JPFMT8T" | "A1W53KSJQ" | "A07846N4L"
        ) {
            if context.is_none_or(|context| context.selected(auth)) {
                return Err(invalid());
            }
        } else if context.is_some_and(|context| context.selected(auth)) {
            return Err(invalid());
        }
        Ok(client.rest_form(
            w,
            "apps.auth.external.start",
            vec![
                ("app_id", auth.app.clone()),
                ("provider_key", auth.provider.clone()),
                ("function_id", auth.function.clone()),
                ("workflow_id", metadata.workflow_id.clone()),
                ("_x_reason", "start_function_auth".into()),
            ],
        ))
    }
    pub fn select_connection_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        metadata: &Metadata,
        function: &str,
        provider: &str,
        token: &str,
    ) -> Result<PreparedRequest, Error> {
        let auth = self.authentication(metadata, current, w, function, provider)?;
        if !auth.tokens.iter().any(|t| t.id == token) {
            return Err(invalid());
        }
        Ok(client.rest_form(
            w,
            "apps.auth.external.authMapping.update",
            vec![
                ("app_id", auth.app.clone()),
                ("provider_key", auth.provider.clone()),
                ("workflow_id", metadata.workflow_id.clone()),
                ("external_token_id", token.into()),
                ("mapping_owner_type", "END_USER".into()),
                ("_x_reason", "update_function_auth".into()),
            ],
        ))
    }
    /// Caller retains the exact request owner across await. A successful mapping response
    /// changes only this selected token; it does not fabricate a new authentication.
    pub fn selected_connection(
        &mut self,
        function: &str,
        provider: &str,
        token: &str,
        response: &Value,
    ) -> Result<(), Error> {
        if response["ok"] != true {
            return Err(invalid());
        }
        let auth = self
            .auth
            .iter_mut()
            .find(|a| a.function == function && a.provider == provider)
            .ok_or_else(invalid)?;
        if !auth.tokens.iter().any(|t| t.id == token) {
            return Err(invalid());
        }
        for value in &mut auth.tokens {
            value.default = value.id == token;
        }
        self.auth_confirmed = false;
        Ok(())
    }
    /// Called only from the current account's actual realtime envelope. Wrong app,
    /// user, function or provider cannot satisfy this retained preflight.
    pub fn external_auth_completed(
        &mut self,
        event: &Value,
        current: &View,
        w: &WorkspaceSession,
    ) -> Result<bool, Error> {
        if !self.workflow.same_view(current) || self.workflow.user != w.user_id {
            return Err(invalid());
        }
        if event["type"] != "external_auth_completed"
            || event["user_id"].as_str() != Some(&self.workflow.user)
        {
            return Ok(false);
        }
        let Some(auth) = self.auth.iter_mut().find(|a| {
            event["app_id"].as_str() == Some(&a.app)
                && event["function_id"].as_str() == Some(&a.function)
                && event["provider_key"].as_str() == Some(&a.provider)
        }) else {
            return Ok(false);
        };
        let token = bounded(event, "external_token_id", 255)?;
        if token.starts_with("sat_") {
            return Ok(false);
        }
        let user = bounded(event, "external_user_id", 8192)?;
        if !auth.tokens.iter().any(|t| t.id == token) && auth.tokens.len() >= 100 {
            return Err(invalid());
        }
        auth.tokens.retain(|t| t.id != token);
        for value in &mut auth.tokens {
            value.default = false;
        }
        auth.tokens.insert(
            0,
            AuthToken {
                id: token,
                user,
                default: true,
            },
        );
        self.auth_confirmed = false;
        Ok(true)
    }
    pub fn authorization_url(response: &Value) -> Result<String, Error> {
        if response["ok"] != true {
            return Err(invalid());
        }
        let url = bounded(response, "authorization_url", 8192)?;
        let parsed = url::Url::parse(&url).map_err(|_| invalid())?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(invalid());
        }
        Ok(url)
    }
}
