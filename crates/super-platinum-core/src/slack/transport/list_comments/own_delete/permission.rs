//! M3rm ordinary channel permission query; no anonymous or cached grants.
use super::*;
use crate::slack_lists::comments::{OwnDeletePermission, OwnDeletePermissionQuery};

impl Transport {
    pub async fn execute_list_comment_delete_permission(
        &self,
        request: PreparedRequest,
        query: OwnDeletePermissionQuery,
        current: impl FnMut() -> bool,
    ) -> Result<OwnDeletePermission, Error> {
        if !matches_query(&request, &query) {
            return Err(Error::Api("invalid_list_comment_delete_permission".into()));
        }
        let current = std::sync::Mutex::new(current);
        let check = || current.lock().is_ok_and(|mut current| current());
        let work = async {
            self.execute_list_rename_once(request, check, allowed)
                .await
                .map_err(|e| match e {
                    ListColumnRenameFailure::NotRequested(e)
                    | ListColumnRenameFailure::Unconfirmed(e) => e,
                })
        };
        let raw = guarded(check, work).await?;
        query.accept(raw).map_err(|e| Error::Api(e.to_string()))
    }
}

fn matches_query(request: &PreparedRequest, query: &OwnDeletePermissionQuery) -> bool {
    if !allowed(request) || query.validate().is_err() {
        return false;
    }
    let RequestBody::Json(raw) = &request.body else {
        return false;
    };
    if raw.get("enterprise_id").and_then(Value::as_str) != query.enterprise_scope() {
        return false;
    }
    let mut expected = query.body();
    for key in ["token", "enterprise_token"] {
        if let Some(value) = raw.get(key) {
            expected[key] = value.clone();
        }
    }
    if let Some(scope) = query.enterprise_scope() {
        expected["enterprise_id"] = Value::String(scope.into());
    }
    let scope = query.edge_scope();
    raw == &expected
        && url::Url::parse(&request.url)
            .is_ok_and(|u| u.path() == format!("/cache/{scope}/permissions/info"))
}

fn allowed(request: &PreparedRequest) -> bool {
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.host_str() == Some("edgeapi.slack.com")
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if request.method != "POST"
        || !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query_pairs().any(|(k, _)| {
            !matches!(
                k.as_ref(),
                "_x_app_name"
                    | "_x_b3_sampled"
                    | "_x_b3_spanid"
                    | "_x_b3_traceid"
                    | "_x_num_retries"
                    | "fp"
            )
        })
    {
        return false;
    }
    let segments: Vec<_> = url.path().split('/').collect();
    if segments.len() != 5
        || segments[1] != "cache"
        || !comments::identity(segments[2])
        || segments[3] != "permissions"
        || segments[4] != "info"
    {
        return false;
    }
    let RequestBody::Json(Value::Object(raw)) = &request.body else {
        return false;
    };
    if serde_json::to_vec(raw).map_or(true, |v| v.len() > 128 * 1024)
        || raw.keys().any(|k| {
            !matches!(
                k.as_str(),
                "token"
                    | "user_id"
                    | "channel_id"
                    | "permissions"
                    | "as_admin"
                    | "enterprise_id"
                    | "enterprise_token"
            )
        })
        || raw
            .get("token")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || !raw
            .get("user_id")
            .and_then(Value::as_str)
            .is_some_and(comments::identity)
        || !raw
            .get("channel_id")
            .and_then(Value::as_str)
            .is_some_and(is_channel)
        || raw.get("permissions") != Some(&serde_json::json!(["DELETE_OWN_MESSAGE"]))
        || raw.get("as_admin").is_some_and(|v| v != &Value::Bool(true))
    {
        return false;
    }
    match (raw.get("enterprise_id"), raw.get("enterprise_token")) {
        (None, None) => true,
        (Some(Value::String(id)), Some(Value::String(token))) => {
            comments::identity(id) && id == segments[2] && !token.is_empty()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
