//! Read-only Share requests retain one account/workspace deadline across visits.
use super::*;
use std::time::Instant;

pub struct FileShareReadGate {
    epoch: Instant,
    until_ms: tokio::sync::Mutex<u128>,
}
impl Default for FileShareReadGate {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            until_ms: tokio::sync::Mutex::new(0),
        }
    }
}
impl FileShareReadGate {
    /// Only an idle expired gate can be removed from its owning account map.
    pub fn expired(&self) -> bool {
        self.until_ms
            .try_lock()
            .is_ok_and(|until| *until <= self.epoch.elapsed().as_millis())
    }
    async fn execute<F, Fut>(&self, work: F) -> Result<Value, Error>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Value, Error>>,
    {
        // Reject overlapping clicks instead of queueing stale picker owners.
        let mut until = self
            .until_ms
            .try_lock()
            .map_err(|_| Error::Api("file_share_read_pending".into()))?;
        let now = self.epoch.elapsed().as_millis();
        if *until > now {
            let seconds = (*until - now).div_ceil(1000).min(u128::from(u64::MAX)) as u64;
            return Err(Error::RateLimited {
                retry_after_secs: Some(seconds),
            });
        }
        let result = work().await;
        let delay = match &result {
            Err(Error::RateLimited { retry_after_secs }) => Some(retry_after_secs.unwrap_or(30)),
            Err(Error::HttpStatus {
                retry_after_secs: Some(seconds),
                ..
            }) => Some(*seconds),
            Err(Error::Api(code)) if code == "ratelimited" => Some(30),
            _ => None,
        };
        if let Some(seconds) = delay {
            *until = self.epoch.elapsed().as_millis() + u128::from(seconds.max(1)) * 1000;
        }
        result
    }
}
impl Transport {
    pub async fn execute_file_share_read(
        &self,
        req: PreparedRequest,
        gate: &FileShareReadGate,
    ) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_file_share_read_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "File share read not sent while offline".into(),
            ));
        }
        gate.execute(|| async {
            self.execute_once_using_read_mode(&req, false, self.single_attempt_client()?, true)
                .await
        })
        .await
    }
}
pub(super) fn rate_limited(req: &PreparedRequest, raw: &Value) -> bool {
    allowed(req) && raw["ok"] == false && raw["error"] == "ratelimited"
}
fn allowed(req: &PreparedRequest) -> bool {
    if req.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&req.url) else {
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
    let path = url.path();
    let parts: Vec<_> = path.split('/').collect();
    matches!(
        path,
        "/api/files.info"
            | "/api/conversations.info"
            | "/api/search.modules.files"
            | "/api/im.list"
    ) || (parts.len() == 5
        && parts[1] == "cache"
        && !parts[2].is_empty()
        && parts[3] == "users"
        && parts[4] == "info")
}

#[cfg(test)]
mod tests;
