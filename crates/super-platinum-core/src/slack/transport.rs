use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;
use wreq::header::HeaderMap;
use wreq_util::Emulation;

use super::Error;
use super::client::{PreparedRequest, RequestBody, redact_secrets};

/// What the last Slack API call said about the link to Slack.
///
/// This is the shell's recovery signal as much as its failure signal: a request
/// that lands clears the offline mark without anything having to poll for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Health {
    /// Nothing has been attempted yet.
    #[default]
    Unknown,
    /// Slack answered — even with an error, which still means the link is up.
    Online,
    /// The request never reached Slack.
    Offline,
}

impl Health {
    fn code(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Online => 1,
            Self::Offline => 2,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Online,
            2 => Self::Offline,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone)]
pub struct Transport {
    http: wreq::Client,
    d_cookie: String,
    /// Shared with every clone: the shell holds one and the workers hold others,
    /// and all of them have to agree about whether the network is up.
    health: Arc<AtomicU8>,
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport").finish_non_exhaustive()
    }
}

/// Upper bound on a single media fetch (avatars, emoji, unfurl images, files).
const MEDIA_FETCH_TIMEOUT: Duration = Duration::from_secs(20);

impl Transport {
    pub fn new(d_cookie: impl Into<String>) -> Result<Self, Error> {
        let http = wreq::Client::builder()
            .emulation(Emulation::Chrome140)
            // Slack avatar and file URLs commonly redirect to a backing CDN or
            // object store. wreq currently defaults to no redirects; its
            // redirect layer removes sensitive headers on cross-host hops.
            .redirect(wreq::redirect::Policy::limited(10))
            .build()
            .map_err(|e| Error::Transport(format!("client build: {e}")))?;
        Ok(Self {
            http,
            d_cookie: d_cookie.into(),
            health: Arc::new(AtomicU8::new(Health::Unknown.code())),
        })
    }

    /// What the last Slack API call said about the link.
    pub fn health(&self) -> Health {
        Health::from_code(self.health.load(Ordering::Relaxed))
    }

    /// Records a verdict about the link. Media fetches deliberately do not call
    /// this: a third-party image host that is down says nothing about Slack.
    pub fn set_health(&self, health: Health) {
        self.health.store(health.code(), Ordering::Relaxed);
    }

    pub fn http(&self) -> &wreq::Client {
        &self.http
    }

    pub fn d_cookie(&self) -> &str {
        &self.d_cookie
    }

    fn cookie(&self) -> String {
        format!("d={}", self.d_cookie)
    }

    pub async fn execute(&self, req: PreparedRequest) -> Result<Value, Error> {
        let mut attempt = 0;
        loop {
            match self.execute_once(&req).await {
                Ok(value) => return Ok(value),
                Err(e) if req.retry_safe() && retryable_error(&e) && attempt < 2 => {
                    attempt += 1;
                    tokio::time::sleep(retry_delay(&e, attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn upload_bytes(
        &self,
        url: &str,
        bytes: Vec<u8>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), Error> {
        let length = bytes.len();
        let chunks = bytes
            .chunks(64 * 1024)
            .map(|chunk| chunk.to_vec())
            .collect::<Vec<_>>();
        let stream = futures::stream::iter(chunks.into_iter().map(move |chunk| {
            progress.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            Ok::<_, std::convert::Infallible>(chunk)
        }));
        let response = self
            .http
            .post(url)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", length)
            .body(wreq::Body::wrap_stream(stream))
            .send()
            .await
            .map_err(send_error)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::HttpStatus {
                status: response.status().as_u16(),
                retry_after_secs: retry_after_secs(response.headers()),
            })
        }
    }

    async fn execute_once(&self, req: &PreparedRequest) -> Result<Value, Error> {
        let req_url = req.url.clone();
        let mut builder = match req.method {
            "POST" => self.http.post(&req.url),
            "GET" => self.http.get(&req.url),
            other => return Err(Error::Transport(format!("unsupported method {other}"))),
        };

        for (key, value) in &req.headers {
            if key.eq_ignore_ascii_case("content-type") {
                continue;
            }
            builder = builder.header(key.as_str(), value.as_str());
        }
        builder = builder.header("Cookie", self.cookie());

        builder = match &req.body {
            RequestBody::Form(fields) => builder.form(fields),
            RequestBody::Json(value) => builder.json(value),
        };

        let response = builder.send().await.map_err(|error| {
            let error = send_error(error);
            if error.is_offline() {
                self.set_health(Health::Offline);
            }
            error
        })?;
        // Slack answered. Even a 500 or a rate limit means the link is up, so
        // the rail stops saying "connecting" as soon as anything lands.
        self.set_health(Health::Online);

        let status = response.status();
        let retry_after_secs = retry_after_secs(response.headers());
        if status.as_u16() == 429 {
            return Err(Error::RateLimited { retry_after_secs });
        }
        if !status.is_success() {
            tracing::warn!(
                url = %redact_secrets(&req_url),
                status = %status,
                retry_after_secs = ?retry_after_secs,
                "slack http error"
            );
            return Err(Error::HttpStatus {
                status: status.as_u16(),
                retry_after_secs,
            });
        }

        let value: Value = response
            .json()
            .await
            .map_err(|e| Error::Transport(format!("decode ({status}): {e}")))?;

        if value.get("ok").and_then(Value::as_bool) == Some(false) {
            let err = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown_error")
                .to_owned();
            tracing::warn!(
                url = %redact_secrets(&req_url),
                status = %status,
                body = %redact_secrets(&value.to_string()),
                "slack api error"
            );
            return Err(Error::Api(err));
        }

        Ok(value)
    }

    pub async fn get_text(&self, url: &str, user_agent: &str) -> Result<String, Error> {
        let response = self
            .http
            .get(url)
            .header("User-Agent", user_agent)
            .header("Cookie", self.cookie())
            .send()
            .await
            .map_err(send_error)?;
        response
            .text()
            .await
            .map_err(|e| Error::Transport(format!("read body: {e}")))
    }

    pub async fn get_bytes(&self, url: &str, user_agent: &str) -> Result<Vec<u8>, Error> {
        self.get_bytes_with_auth(url, user_agent, true, None).await
    }

    pub async fn get_public_bytes(&self, url: &str, user_agent: &str) -> Result<Vec<u8>, Error> {
        self.get_bytes_with_auth(url, user_agent, false, None).await
    }

    pub async fn get_public_gif_bytes(
        &self,
        url: &str,
        user_agent: &str,
    ) -> Result<Vec<u8>, Error> {
        self.get_bytes_with_auth(url, user_agent, false, Some("image/gif"))
            .await
    }

    async fn get_bytes_with_auth(
        &self,
        url: &str,
        user_agent: &str,
        authenticated: bool,
        accept: Option<&str>,
    ) -> Result<Vec<u8>, Error> {
        // Media hosts are third-party and occasionally never answer. Without a
        // bound, one stalled avatar or unfurl image wedges the whole media
        // refresh (and every later one, since loads are serialized).
        let mut request = self
            .http
            .get(url)
            .timeout(MEDIA_FETCH_TIMEOUT)
            .header("User-Agent", user_agent);
        if let Some(accept) = accept {
            request = request.header("Accept", accept);
        }
        if authenticated {
            request = request.header("Cookie", self.cookie());
        }
        let response = request.send().await.map_err(send_error)?;

        let status = response.status();
        let retry_after_secs = retry_after_secs(response.headers());
        if status.as_u16() == 429 {
            return Err(Error::RateLimited { retry_after_secs });
        }
        if !status.is_success() {
            tracing::warn!(
                url = %redact_secrets(url),
                status = %status,
                retry_after_secs = ?retry_after_secs,
                "slack file http error"
            );
            return Err(Error::HttpStatus {
                status: status.as_u16(),
                retry_after_secs,
            });
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|e| Error::Transport(format!("read body: {e}")))
    }
}

pub fn retry_after_secs(headers: &HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

/// A dropped link surfaces the same way on every call in flight: a refused or
/// unroutable connect, a name that will not resolve, a timeout, or a socket the
/// peer reset. Naming that class here is what lets the shell answer it with the
/// connection indicator rather than a toast full of signed URL.
fn send_error(error: wreq::Error) -> Error {
    if error.is_connect()
        || error.is_timeout()
        || error.is_connection_reset()
        || error.is_request()
    {
        return Error::Offline(error.without_uri().to_string());
    }
    Error::Transport(format!("send: {error}"))
}

fn retryable_error(error: &Error) -> bool {
    matches!(error, Error::RateLimited { .. })
        || matches!(
            error,
            Error::HttpStatus {
                status: 500..=599,
                ..
            }
        )
}

fn retry_delay(error: &Error, attempt: u32) -> Duration {
    let fallback_ms = 250 * 2_u64.saturating_pow(attempt.saturating_sub(1));
    let secs = match error {
        Error::RateLimited {
            retry_after_secs: Some(secs),
        }
        | Error::HttpStatus {
            retry_after_secs: Some(secs),
            ..
        } => Some((*secs).min(30)),
        _ => None,
    };
    secs.map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_millis(fallback_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use wreq::header::HeaderValue;

    #[test]
    fn parses_retry_after_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("12"));
        assert_eq!(retry_after_secs(&headers), Some(12));
    }

    #[test]
    fn ignores_invalid_retry_after() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("soon"));
        assert_eq!(retry_after_secs(&headers), None);
    }

    #[test]
    fn retry_policy_is_bounded() {
        assert!(retryable_error(&Error::RateLimited {
            retry_after_secs: Some(60)
        }));
        assert!(
            retry_delay(
                &Error::RateLimited {
                    retry_after_secs: Some(60)
                },
                1
            ) <= Duration::from_secs(30)
        );
        assert!(retryable_error(&Error::HttpStatus {
            status: 503,
            retry_after_secs: None
        }));
        assert!(!retryable_error(&Error::HttpStatus {
            status: 404,
            retry_after_secs: None
        }));
    }

    /// A refused connect is the shell's offline signal, and the transport has
    /// to leave the health cell saying so — that mark is what keeps a raw URL
    /// out of the toast strip and puts a spinner on the rail instead.
    #[tokio::test]
    async fn a_refused_connect_reads_as_offline_and_marks_the_health_cell() {
        // Bound and immediately dropped: nothing is listening on this port, so
        // the connect is refused rather than left hanging.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);

        let transport = Transport::new("secret-cookie").unwrap();
        assert_eq!(transport.health(), Health::Unknown);
        let request = PreparedRequest {
            method: "POST",
            url: format!("http://{address}/api/client.dms?token=secret"),
            headers: Vec::new(),
            body: RequestBody::Form(Vec::new()),
        };

        let error = transport.execute(request).await.unwrap_err();
        assert!(error.is_offline(), "{error}");
        assert_eq!(transport.health(), Health::Offline);

        // And the signed URL never rides along in the message.
        assert!(!error.to_string().contains("token=secret"), "{error}");
    }

    async fn captured_image_request(public: bool, gif: bool) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let read = socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        let transport = Transport::new("secret-cookie").unwrap();
        let url = format!("http://{address}/image.{}", if gif { "gif" } else { "png" });
        let bytes = if gif {
            transport
                .get_public_gif_bytes(&url, "super-platinum-test")
                .await
        } else if public {
            transport
                .get_public_bytes(&url, "super-platinum-test")
                .await
        } else {
            transport.get_bytes(&url, "super-platinum-test").await
        }
        .unwrap();
        assert_eq!(bytes, b"ok");
        server.await.unwrap()
    }

    #[tokio::test]
    async fn public_image_fetch_never_sends_slack_cookie() {
        let request = captured_image_request(true, false).await;
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
    }

    #[tokio::test]
    async fn public_gif_fetch_forces_gif_content_negotiation_without_cookie() {
        let request = captured_image_request(true, true)
            .await
            .to_ascii_lowercase();
        assert!(request.contains("accept: image/gif"));
        assert!(!request.contains("cookie:"));
    }

    #[tokio::test]
    async fn slack_image_fetch_sends_session_cookie() {
        let request = captured_image_request(false, false).await;
        assert!(
            request
                .to_ascii_lowercase()
                .contains("cookie: d=secret-cookie")
        );
    }

    #[tokio::test]
    async fn image_fetch_follows_cross_host_redirect_without_forwarding_cookie() {
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination_address = destination.local_addr().unwrap();
        let destination_server = tokio::spawn(async move {
            let (mut socket, _) = destination.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let read = socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        let redirect = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let redirect_address = redirect.local_addr().unwrap();
        let redirect_server = tokio::spawn(async move {
            let (mut socket, _) = redirect.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let _ = socket.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{destination_address}/avatar.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let transport = Transport::new("secret-cookie").unwrap();
        let bytes = transport
            .get_bytes(
                &format!("http://{redirect_address}/avatar"),
                "super-platinum-test",
            )
            .await
            .unwrap();
        assert_eq!(bytes, b"ok");
        redirect_server.await.unwrap();
        let destination_request = destination_server.await.unwrap().to_ascii_lowercase();
        assert!(!destination_request.contains("cookie:"));
    }
}
