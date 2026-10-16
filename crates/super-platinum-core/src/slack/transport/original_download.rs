//! Bounded, explicit original GET. Preview fetch policy remains unchanged.
use super::*;
use crate::file_download::{DownloadError, MAX_ORIGINAL_BYTES, OriginalSource, WRITE_CHUNK};
use std::{future::Future, sync::OnceLock};
use tokio::{io::AsyncWriteExt, sync::Semaphore};
const TOTAL: Duration = Duration::from_secs(30 * 60);
const IDLE: Duration = Duration::from_secs(30);
static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

impl Transport {
    pub async fn download_original(
        &self,
        source: &OriginalSource,
        user_agent: &str,
        file: &mut tokio::fs::File,
        current: impl Fn() -> bool,
    ) -> Result<u64, DownloadError> {
        self.http().map_err(DownloadError::Network)?;
        let _permit = SLOTS
            .get_or_init(|| Arc::new(Semaphore::new(2)))
            .clone()
            .try_acquire_owned()
            .map_err(|_| DownloadError::Busy)?;
        tokio::time::timeout(
            TOTAL,
            self.original_inner(source, user_agent, file, &current, MAX_ORIGINAL_BYTES),
        )
        .await
        .map_err(|_| DownloadError::Network(Error::Offline("original download timed out".into())))?
    }
    async fn original_wait<T>(
        &self,
        future: impl Future<Output = T>,
        current: &impl Fn() -> bool,
    ) -> Result<T, DownloadError> {
        tokio::pin!(future);
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            if !current() {
                return Err(DownloadError::Canceled);
            }
            if self.health() == Health::Offline {
                return Err(DownloadError::Network(Error::Offline(
                    "network is down".into(),
                )));
            }
            tokio::select! {biased; result=&mut future=>return Ok(result), _=tick.tick()=>{}}
        }
    }
    async fn original_inner(
        &self,
        source: &OriginalSource,
        user_agent: &str,
        file: &mut tokio::fs::File,
        current: &impl Fn() -> bool,
        limit: u64,
    ) -> Result<u64, DownloadError> {
        self.http().map_err(DownloadError::Network)?;
        // The fixture source is an exact loopback capability. System proxies
        // must not turn this disconnected test into an external request.
        #[cfg(feature = "original-download-fixture")]
        let fixture_http = if !source.permits_redirect() {
            Some(
                wreq::Client::builder()
                    .emulation(Emulation::Chrome140)
                    .no_proxy()
                    .redirect(wreq::redirect::Policy::none())
                    .retry(wreq::retry::Policy::never())
                    .build()
                    .map_err(|_| DownloadError::InvalidSource)?,
            )
        } else {
            None
        };
        #[cfg(feature = "original-download-fixture")]
        let http = match &fixture_http {
            Some(http) => http,
            None => self
                .single_attempt_client()
                .map_err(DownloadError::Network)?,
        };
        #[cfg(not(feature = "original-download-fixture"))]
        let http = self
            .single_attempt_client()
            .map_err(DownloadError::Network)?;
        let mut url = source.url.clone();
        let mut cookie = source.authenticated;
        for hop in 0..=10 {
            if !source.accepts(&url) {
                return Err(DownloadError::InvalidSource);
            }
            let mut request = http
                .get(url.as_str())
                .redirect(wreq::redirect::Policy::none())
                .timeout(TOTAL)
                .read_timeout(IDLE)
                .header("User-Agent", user_agent)
                .header("Accept-Encoding", "identity");
            if cookie {
                request = request.header("Cookie", self.cookie());
            }
            let request = request.build().map_err(|_| DownloadError::InvalidSource)?;
            let response = self
                .original_wait(tokio::time::timeout(IDLE, http.execute(request)), current)
                .await?
                .map_err(|_| {
                    DownloadError::Network(Error::Offline("original response timed out".into()))
                })?
                .map_err(|e| {
                    DownloadError::Network(if e.is_connect() || e.is_timeout() {
                        Error::Offline("original connection failed".into())
                    } else {
                        Error::Transport("original response failed".into())
                    })
                })?;
            // Observe known limits before owner/cancellation revalidation. Never retry.
            let status = response.status().as_u16();
            let retry_after_secs = original_retry_after(response.headers());
            if status == 429 {
                return Err(DownloadError::Network(Error::RateLimited {
                    retry_after_secs,
                }));
            }
            if retry_after_secs.is_some()
                || !(200..300).contains(&status) && !(300..400).contains(&status)
            {
                return Err(DownloadError::Network(Error::HttpStatus {
                    status,
                    retry_after_secs,
                }));
            }
            if !current() {
                return Err(DownloadError::Canceled);
            }
            if (300..400).contains(&status) {
                if !source.permits_redirect()
                    || !matches!(status, 301 | 302 | 303 | 307 | 308)
                    || hop == 10
                {
                    return Err(DownloadError::InvalidSource);
                }
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or(DownloadError::InvalidSource)?;
                if location.len() > 16 * 1024 {
                    return Err(DownloadError::InvalidSource);
                }
                let next = url
                    .join(location)
                    .map_err(|_| DownloadError::InvalidSource)?;
                if url.scheme() == "https" && next.scheme() != "https" {
                    return Err(DownloadError::InvalidSource);
                }
                // Once credentials leave their origin they can never be reattached.
                cookie &= next.origin() == url.origin();
                url = next;
                continue;
            }
            if status != 200 {
                return Err(DownloadError::Network(Error::HttpStatus {
                    status,
                    retry_after_secs: None,
                }));
            }
            let declared = response.content_length();
            if declared.is_some_and(|n| n > limit) {
                return Err(DownloadError::TooLarge);
            }
            let mut stream = response.bytes_stream();
            let mut total = 0u64;
            loop {
                let next = self
                    .original_wait(tokio::time::timeout(IDLE, stream.next()), current)
                    .await?
                    .map_err(|_| {
                        DownloadError::Network(Error::Offline("original body timed out".into()))
                    })?;
                let Some(chunk) = next else {
                    break;
                };
                let chunk = chunk.map_err(|_| {
                    DownloadError::Network(Error::Transport("original body incomplete".into()))
                })?;
                total = total
                    .checked_add(chunk.len() as u64)
                    .filter(|n| *n <= limit)
                    .ok_or(DownloadError::TooLarge)?;
                for part in chunk.chunks(WRITE_CHUNK) {
                    if !current() {
                        return Err(DownloadError::Canceled);
                    }
                    self.original_wait(file.write_all(part), current)
                        .await?
                        .map_err(|_| DownloadError::Io)?;
                }
            }
            if declared.is_some_and(|n| n != total) {
                return Err(DownloadError::Network(Error::Transport(
                    "original length mismatch".into(),
                )));
            }
            if !current() {
                return Err(DownloadError::Canceled);
            }
            return Ok(total);
        }
        Err(DownloadError::InvalidSource)
    }
}
fn original_retry_after(headers: &HeaderMap) -> Option<u64> {
    let value = headers.get("retry-after")?;
    let Some(raw) = value.to_str().ok().map(str::trim) else {
        return Some(30);
    };
    if let Ok(secs) = raw.parse::<u64>() {
        return Some(secs.max(1));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(raw) {
        let millis = date
            .signed_duration_since(chrono::Utc::now())
            .num_milliseconds();
        return Some(
            u64::try_from(millis)
                .unwrap_or(0)
                .saturating_add(999)
                .saturating_div(1000)
                .max(1),
        );
    }
    Some(30)
}
#[cfg(test)]
mod tests;
