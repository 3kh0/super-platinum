//! Closed Share search preset; shared ordered bytes engine, no policy grant.
use super::*;
#[cfg(test)]
mod tests;
impl Transport {
    pub async fn execute_workflow_share_search_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<WorkflowShareSearchResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_workflow_share_search_request".into()));
        }
        let result = self
            .execute_profile_member_read_admitted(req, &current)
            .await;
        if !current() {
            return Err(Error::Offline("Workflow Share search owner retired".into()));
        }
        result
    }
}
fn allowed(req: &PreparedRequest) -> bool {
    if req.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&req.url) else {
        return false;
    };
    let parts: Vec<_> = url.path().split('/').collect();
    if url.scheme() != "https"
        || url.host_str() != Some("edgeapi.slack.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || parts.len() != 5
        || parts[1] != "cache"
        || parts[2].len() > 256
        || !matches!(parts[2].as_bytes().first(), Some(b'T' | b'E'))
        || !parts[2]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || parts[4] != "search"
    {
        return false;
    }
    let members = match parts[3] {
        "users" => true,
        "channels" => false,
        _ => return false,
    };
    let RequestBody::Json(body) = &req.body else {
        return false;
    };
    let Some(obj) = body.as_object() else {
        return false;
    };
    let common = obj.keys().all(|k| {
        matches!(
            k.as_str(),
            "token" | "enterprise_token" | "query" | "count" | "fuzz" | "filter" | "marker"
        ) || if members {
            matches!(
                k.as_str(),
                "enable_workspace_ranking" | "search_email" | "include_profile_only_users"
            )
        } else {
            matches!(k.as_str(), "include_record_channels" | "check_membership")
        }
    }) && body["token"].as_str().is_some_and(|s| !s.is_empty())
        && body
            .get("enterprise_token")
            .is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        && body["query"].as_str().is_some_and(|s| s.len() <= 1_048_576)
        && body["count"] == 24
        && body["fuzz"] == 1
        && body.get("marker").is_none_or(|v| {
            v.as_str().is_some_and(|s| {
                !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
            })
        });
    common
        && if members {
            body["enable_workspace_ranking"] == true
                && body["search_email"] == true
                && body["include_profile_only_users"] == true
                && body["filter"] == "NOT deactivated"
        } else {
            body["include_record_channels"] == true
                && body["check_membership"] == true
                && body["filter"] == "xws"
        }
}
