//! Closed Activity users/search transport. The shared directory decoder retains
//! raw response and Object.keys encounter order from the same response bytes.
//! This observation grants no selection or member-store publication authority.
use super::*;
#[cfg(test)]
mod tests;

impl Transport {
    /// Existing link hold and retry classification; this POST is not retry-safe.
    pub async fn execute_activity_member_search(
        &self,
        req: PreparedRequest,
    ) -> Result<ActivityMemberResponse, Error> {
        self.execute_activity_member_search_current(req, || true)
            .await
    }

    /// Additional caller-owned admission after every link hold and before retries.
    /// Original dispatch/decoder policy and already-issued errors are retained.
    pub async fn execute_activity_member_search_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<ActivityMemberResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_activity_member_search_request".into()));
        }
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Activity member search owner retired".into())
                }));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Activity member search owner retired".into())
                }));
            }
            link?;
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_activity_member_once(&req) => result,
                    () = self.link_dropped() => {
                        Err(Error::Offline("link dropped mid-call".into()))
                    }
                }
            } else {
                self.execute_activity_member_once(&req).await
            };
            match attempted {
                Ok(value) => return Ok(value),
                Err(e) if req.retry_safe() && retryable_error(&e) && attempt < 2 => {
                    attempt += 1;
                    last_failure = Some(e.clone());
                    tokio::time::sleep(retry_delay(&e, attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn execute_activity_member_once(
        &self,
        req: &PreparedRequest,
    ) -> Result<ActivityMemberResponse, Error> {
        let mut keys = None;
        let raw = self
            .execute_once_using_decode_mode(req, false, self.http()?, false, Some(&mut keys))
            .await?;
        Ok(ActivityMemberResponse { raw, keys })
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
        || parts[2].is_empty()
        || parts[2].len() > 255
        || !parts[2]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || parts[3] != "users"
        || parts[4] != "search"
    {
        return false;
    }
    let RequestBody::Json(body) = &req.body else {
        return false;
    };
    let Some(object) = body.as_object() else {
        return false;
    };
    object.keys().all(|k| {
        matches!(
            k.as_str(),
            "token"
                | "enterprise_token"
                | "query"
                | "count"
                | "fuzz"
                | "enable_workspace_ranking"
                | "filter"
                | "include_profile_only_users"
                | "marker"
        )
    }) && body["token"].as_str().is_some_and(|s| !s.is_empty())
        && body
            .get("enterprise_token")
            .is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        && body["query"]
            .as_str()
            .is_some_and(|s| s.len() <= 4096 && !s.contains('\0'))
        && body["count"] == 20
        && body["fuzz"] == 1
        && body["enable_workspace_ranking"] == true
        && body["filter"] == "NOT deactivated"
        && body["include_profile_only_users"] == true
        && body.get("marker").is_none_or(|v| {
            v.as_str().is_some_and(|s| {
                !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
            })
        })
}
