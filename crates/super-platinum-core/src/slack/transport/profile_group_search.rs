//! Closed Profile usergroups/search read using the shared same-byte directory
//! decoder. Ordered response custody is not canonical publication permission.
use super::*;
#[cfg(test)]
mod tests;

impl Transport {
    pub async fn execute_profile_group_search(
        &self,
        req: PreparedRequest,
    ) -> Result<ProfileGroupSearchResponse, Error> {
        self.execute_profile_group_search_current(req, || true)
            .await
    }
    /// Preserve the existing link hold, retry classification and issued errors.
    /// The caller retains original account/query/source authority after return.
    pub async fn execute_profile_group_search_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<ProfileGroupSearchResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_profile_group_search_request".into()));
        }
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Profile group search owner retired".into())
                }));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Profile group search owner retired".into())
                }));
            }
            link?;
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_profile_group_search_once(&req) => result,
                    () = self.link_dropped() => Err(Error::Offline("link dropped mid-call".into())),
                }
            } else {
                self.execute_profile_group_search_once(&req).await
            };
            match attempted {
                Ok(value) => return Ok(value),
                Err(error) if req.retry_safe() && retryable_error(&error) && attempt < 2 => {
                    attempt += 1;
                    last_failure = Some(error.clone());
                    tokio::time::sleep(retry_delay(&error, attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }
    async fn execute_profile_group_search_once(
        &self,
        req: &PreparedRequest,
    ) -> Result<ProfileGroupSearchResponse, Error> {
        let mut keys = None;
        let raw = self
            .execute_once_using_decode_mode(req, false, self.http()?, false, Some(&mut keys))
            .await?;
        Ok(ProfileGroupSearchResponse { raw, keys })
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
        || parts[2].len() > 256
        || !matches!(parts[2].as_bytes().first(), Some(b'T' | b'E'))
        || !parts[2]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || parts[3] != "usergroups"
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
    object.keys().all(|key| {
        matches!(
            key.as_str(),
            "token" | "enterprise_token" | "query" | "count" | "org_wide" | "marker"
        )
    }) && body["token"].as_str().is_some_and(|v| !v.is_empty())
        && body
            .get("enterprise_token")
            .is_none_or(|v| v.as_str().is_some_and(|v| !v.is_empty()))
        && body["query"].as_str().is_some_and(|v| v.len() <= 1_048_576)
        && body["count"] == 25
        && body["org_wide"].is_boolean()
        && body.get("marker").is_none_or(|v| {
            v.as_str().is_some_and(|v| {
                !v.is_empty() && v.len() <= 4096 && !v.chars().any(char::is_control)
            })
        })
}
