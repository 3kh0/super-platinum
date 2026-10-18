//! Closed conditional or unconditional channel reacquisition. Existing link/HTTP/error policy
//! and ordered decoder are reused; no source selection authority is granted.
use super::*;
#[cfg(test)]
mod tests;

impl Transport {
    /// Same link hold, safe-request cancellation, retry decision and delay as
    /// execute. This closed POST is currently not retry_safe; preserve that
    /// policy rather than silently adding retries to a replaceable query.
    pub async fn execute_directory_channel_info(
        &self,
        req: PreparedRequest,
    ) -> Result<DirectoryChannelInfoResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_directory_channel_info_request".into()));
        }
        let mut attempt = 0;
        loop {
            self.await_link().await?;
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_directory_channel_once(&req) => result,
                    () = self.link_dropped() => {
                        Err(Error::Offline("link dropped mid-call".into()))
                    }
                }
            } else {
                self.execute_directory_channel_once(&req).await
            };
            match attempted {
                Ok(value) => return Ok(value),
                Err(e) if req.retry_safe() && retryable_error(&e) && attempt < 2 => {
                    attempt += 1;
                    tokio::time::sleep(retry_delay(&e, attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Additional caller-owned admission after every link hold and before retries.
    /// Original dispatch/decoder policy and already-issued errors are retained.
    pub async fn execute_directory_channel_info_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<DirectoryChannelInfoResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_directory_channel_info_request".into()));
        }
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure
                    .unwrap_or_else(|| Error::Offline("Profile read owner retired".into())));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure
                    .unwrap_or_else(|| Error::Offline("Profile read owner retired".into())));
            }
            link?;
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_directory_channel_once(&req) => result,
                    () = self.link_dropped() => {
                        Err(Error::Offline("link dropped mid-call".into()))
                    }
                }
            } else {
                self.execute_directory_channel_once(&req).await
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

    async fn execute_directory_channel_once(
        &self,
        req: &PreparedRequest,
    ) -> Result<DirectoryChannelInfoResponse, Error> {
        let mut keys = None;
        let raw = self
            .execute_once_using_decode_mode(req, false, self.http()?, false, Some(&mut keys))
            .await?;
        Ok(DirectoryChannelInfoResponse { raw, keys })
    }
}

pub(super) fn allowed(req: &PreparedRequest) -> bool {
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
        || parts[3] != "channels"
        || parts[4] != "info"
    {
        return false;
    }
    let RequestBody::Json(body) = &req.body else {
        return false;
    };
    crate::block_actions::directory_endpoint::channel_info::valid_wire_body(body)
}
