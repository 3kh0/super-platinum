//! A separate once-only Copy permission executor; metadata admission stays unchanged.
use super::*;
impl Transport {
    /// Exactly the source-built Copy permission plans. The caller retains each slot
    /// receipt and must not replay a lost response or infer ACK from a read.
    pub async fn execute_workflow_copy_permission(
        &self,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api(
                "workflow_management_not_sent_copy_permission_method".into(),
            ));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Workflow copy permission was not sent while offline".into(),
            ));
        }
        let http = self
            .single_attempt_client()
            .map_err(|_| Error::Api("workflow_management_not_sent_client".into()))?;
        self.execute_once_using(&request, true, http)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => Error::Transport(
                    "Workflow copy permission response was lost after dispatch".into(),
                ),
                error => error,
            })
    }
}
fn allowed(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !metadata_query(&url)
    {
        return false;
    }
    let (key, reason) = match url.path() {
        "/api/functions.workflows.permissions.copy.set" => ("workflow_id", "set-copy-permission"),
        _ => return false,
    };
    fields.len() == 4
        && ["token", key, "permission_type", "_x_reason"]
            .iter()
            .all(|key| fields.iter().filter(|(k, _)| k == *key).count() == 1)
        && fields.iter().all(|(k, v)| {
            if k == "_x_reason" {
                v == reason
            } else if k == "permission_type" {
                matches!(v.as_str(), "everyone" | "collaborators")
            } else {
                !v.is_empty()
            }
        })
}
pub(super) fn metadata_query(url: &url::Url) -> bool {
    // SlackClient::rest_form appends these transport metadata fields. They
    // cannot carry or override the operation's body arguments.
    let mut seen = std::collections::BTreeSet::new();
    url.query_pairs().all(|(key, value)| {
        !value.is_empty()
            && seen.insert(key.to_string())
            && matches!(
                key.as_ref(),
                "_x_id"
                    | "_x_csid"
                    | "_x_desktop_ia"
                    | "_x_foreground"
                    | "_x_frontend_build_type"
                    | "_x_gantry"
                    | "_x_num_retries"
                    | "_x_version_ts"
                    | "fp"
                    | "slack_route"
            )
            && (key != "_x_num_retries" || value == "0")
    })
}
#[cfg(test)]
mod tests;
