//! Closed explicit group reads using the existing link/HTTP/error policy.
use super::*;
use crate::block_actions::directory_endpoint::usergroup_info;
#[cfg(test)]
mod tests;

impl Transport {
    pub async fn execute_directory_usergroup_info_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<DirectoryUsergroupInfoResponse, Error> {
        if !allowed(&req, true) {
            return Err(Error::Api(
                "invalid_directory_usergroup_info_request".into(),
            ));
        }
        self.execute_group_current(req, current, true).await
    }
    pub async fn execute_usergroup_fallback_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        if !allowed(&req, false) {
            return Err(Error::Api("invalid_usergroup_fallback_request".into()));
        }
        self.execute_group_current(req, current, false)
            .await
            .map(|response| response.raw)
    }
    async fn execute_group_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
        ordered: bool,
    ) -> Result<DirectoryUsergroupInfoResponse, Error> {
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure.unwrap_or_else(retired));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure.unwrap_or_else(retired));
            }
            link?;
            let result = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result=self.execute_group_once(&req,ordered)=>result,
                    ()=self.link_dropped()=>Err(Error::Offline("link dropped mid-call".into())),
                }
            } else {
                self.execute_group_once(&req, ordered).await
            };
            match result {
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
    async fn execute_group_once(
        &self,
        req: &PreparedRequest,
        ordered: bool,
    ) -> Result<DirectoryUsergroupInfoResponse, Error> {
        let mut keys = None;
        let raw = self
            .execute_once_using_decode_mode(
                req,
                false,
                self.single_attempt_client()?,
                true,
                if ordered { Some(&mut keys) } else { None },
            )
            .await?;
        Ok(DirectoryUsergroupInfoResponse { raw, keys })
    }
}
// Only these admitted group reads opt into preserving API-level Retry-After.
pub(super) fn rate_limited(req: &PreparedRequest, raw: &Value) -> bool {
    (allowed(req, true) || allowed(req, false))
        && raw["ok"] == false
        && raw["error"] == "ratelimited"
}
fn retired() -> Error {
    Error::Offline("Profile group read owner retired".into())
}
fn allowed(req: &PreparedRequest, ordered: bool) -> bool {
    if req.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&req.url) else {
        return false;
    };
    let origin = url.scheme() == "https" && url.port_or_known_default() == Some(443);
    // Test-only loopback still requires the exact route and request body below.
    let loopback = cfg!(test) && url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    if !(origin || loopback)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    if ordered {
        let parts: Vec<_> = url.path().split('/').collect();
        if !(url.host_str() == Some("edgeapi.slack.com") || loopback)
            || parts.len() != 5
            || parts[1] != "cache"
            || parts[2].is_empty()
            || parts[2].len() > 255
            || !parts[2]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || parts[3] != "usergroups"
            || parts[4] != "info"
        {
            return false;
        }
        let RequestBody::Json(body) = &req.body else {
            return false;
        };
        usergroup_info::valid_wire_body(body)
    } else {
        if !(url.host_str().is_some_and(|h| h.ends_with(".slack.com")) || loopback)
            || url.path() != "/api/subteams.info"
        {
            return false;
        }
        let RequestBody::Form(body) = &req.body else {
            return false;
        };
        usergroup_info::valid_fallback_fields(body)
    }
}
