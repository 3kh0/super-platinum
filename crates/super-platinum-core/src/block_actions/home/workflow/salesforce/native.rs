//! Native producer for the environment input: actual configured REST service,
//! never the local renderer URL or an independently running Slack browser.
use super::{Context, SlackClient, Value, WorkspaceSession};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    MalformedServiceUrl,
    NonOriginConfiguration,
    QueryOverride,
    UnsupportedServiceOrigin,
    UnexpectedRequestTarget,
    EnvironmentMismatch,
    MissingAssignments,
}

/// Keep this entire value in the parent owner snapshot. Equality includes the
/// exact configured URL as well as the generated request origin and assignments.
/// Credential equality remains the parent's existing account/transport guard.
#[derive(Clone, PartialEq)]
pub struct NativeContext {
    configured_url: String,
    request_origin: String,
    context: Context,
}

impl NativeContext {
    /// `workflow` and `jwt` are actual API-backed `_Z` assignment observations.
    /// This generates a prepared listOrgs request solely to inspect its actual
    /// route. It never executes the request or accesses a browser. The prepared
    /// body is discarded; no session token is retained or exposed in this value.
    pub fn observe(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        workflow: &Value,
        jwt: &Value,
    ) -> Result<Self, Unavailable> {
        let configured =
            url::Url::parse(&workspace.url).map_err(|_| Unavailable::MalformedServiceUrl)?;
        if configured.query().is_some() || configured.fragment().is_some() {
            return Err(Unavailable::QueryOverride);
        }
        if configured.path() != "/" {
            return Err(Unavailable::NonOriginConfiguration);
        }
        known_service(&configured)?;
        // Exact existing rest_form -> rest_base behavior, including Enterprise
        // rewriting. Do not reimplement that routing or use workspace.url alone.
        let prepared = client.rest_form(workspace, "sfdc.integration.listOrgs", vec![]);
        let request =
            url::Url::parse(&prepared.url).map_err(|_| Unavailable::MalformedServiceUrl)?;
        known_service(&request)?;
        if prepared.method != "POST"
            || request.path() != "/api/sfdc.integration.listOrgs"
            || request.fragment().is_some()
        {
            return Err(Unavailable::UnexpectedRequestTarget);
        }
        let configured_origin = configured.origin().ascii_serialization();
        let request_origin = request.origin().ascii_serialization();
        if [workflow, jwt].iter().any(|value| {
            !value
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 128)
        }) {
            return Err(Unavailable::MissingAssignments);
        }
        let configured_context = Context::observe(&configured_origin, workflow, jwt)
            .map_err(|_| Unavailable::UnsupportedServiceOrigin)?;
        let context = Context::observe(&request_origin, workflow, jwt)
            .map_err(|_| Unavailable::UnsupportedServiceOrigin)?;
        // Tin+.Ei depends only on the dev and gov predicates. QA/staging/prod
        // share its non-dev branch. Reject a rest_base rewrite that changes the
        // selected app rather than silently defaulting to the production app.
        if configured_context.app() != context.app() {
            return Err(Unavailable::EnvironmentMismatch);
        }
        Ok(Self {
            configured_url: workspace.url.clone(),
            request_origin,
            context,
        })
    }
    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn configured_url(&self) -> &str {
        &self.configured_url
    }
    pub fn request_origin(&self) -> &str {
        &self.request_origin
    }
}

fn known_service(url: &url::Url) -> Result<(), Unavailable> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(Unavailable::UnsupportedServiceOrigin);
    }
    let host = url
        .host_str()
        .ok_or(Unavailable::UnsupportedServiceOrigin)?;
    let roots = [
        "slack.com",
        "slack-dev.com",
        "slack-gov.com",
        "slack-gov-dev.com",
        "slack.mil",
        "slack-mil-dev.com",
    ];
    if !roots
        .iter()
        .any(|root| host == *root || host.ends_with(&format!(".{root}")))
    {
        return Err(Unavailable::UnsupportedServiceOrigin);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
