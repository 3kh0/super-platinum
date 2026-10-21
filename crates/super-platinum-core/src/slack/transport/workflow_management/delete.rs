//! A separate once-only Delete executor; metadata admission stays unchanged.
use super::*;
impl Transport {
    /// Exactly the two source-built Delete plans. The caller retains each slot
    /// receipt and must not replay a lost response or infer ACK from a read.
    pub async fn execute_workflow_delete(&self, request: PreparedRequest) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api(
                "workflow_management_not_sent_delete_method".into(),
            ));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Workflow deletion was not sent while offline".into(),
            ));
        }
        let http = self
            .single_attempt_client()
            .map_err(|_| Error::Api("workflow_management_not_sent_client".into()))?;
        self.execute_once_using(&request, true, http)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => {
                    Error::Transport("Workflow deletion response was lost after dispatch".into())
                }
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
    if !origin || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some()
    {
        return false;
    }
    let (key, reason) = match url.path() {
        "/api/workflows.triggers.delete" => ("trigger_id", "delete-hermes-workflow"),
        "/api/functions.workflows.delete" => ("workflow_id", "delete-workflow"),
        _ => return false,
    };
    fields.len() == 3
        && ["token", key, "_x_reason"]
            .iter()
            .all(|key| fields.iter().filter(|(k, _)| k == *key).count() == 1)
        && fields.iter().all(|(k, v)| {
            if k == "_x_reason" {
                v == reason
            } else {
                !v.is_empty()
            }
        })
}
#[cfg(test)]
mod tests;
