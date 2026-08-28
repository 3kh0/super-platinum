use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::watch;

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

#[derive(Clone)]
pub struct Transport {
    http: wreq::Client,
    d_cookie: String,
    /// Shared with every clone: the shell holds one and the workers hold others,
    /// and all of them have to agree about whether the network is up. A watch
    /// rather than a flag, because held requests wait on it changing.
    health: Arc<watch::Sender<Health>>,
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport").finish_non_exhaustive()
    }
}

/// Upper bound on a single media fetch (avatars, emoji, unfurl images, files).
const MEDIA_FETCH_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a call waits for a downed link to come back before giving up.
///
/// The shell confirms recovery by reaching the workspace host, so this is a
/// backstop for the case where that confirmation never arrives — a wait with no
/// end is the hang this whole path exists to remove.
const OFFLINE_HOLD: Duration = Duration::from_secs(60);

/// Upper bound on a single Slack API call.
///
/// Without one, a link that disappears mid-flight leaves the request hanging
/// forever — the pane keeps saying "Loading replies…" and nothing ever reports
/// the failure that would put the connection indicator on the rail. Generous
/// enough for a cold `client.userBoot`, which is the largest of these by far.
const API_TIMEOUT: Duration = Duration::from_secs(30);

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
            health: Arc::new(watch::Sender::new(Health::Unknown)),
        })
    }

    /// What the last Slack API call said about the link.
    pub fn health(&self) -> Health {
        *self.health.borrow()
    }

    /// A view of the link's health for anything that has to react to it rather
    /// than ask — the realtime supervisor, which must not sit on a dead socket
    /// or dial into a link that is known to be gone.
    pub fn health_watch(&self) -> watch::Receiver<Health> {
        self.health.subscribe()
    }

    /// Records a verdict about the link. Media fetches deliberately do not call
    /// this: a third-party image host that is down says nothing about Slack.
    pub fn set_health(&self, health: Health) {
        self.health.send_replace(health);
    }

    /// Resolves when the shell decides the link has gone.
    async fn link_dropped(&self) {
        let mut health = self.health.subscribe();
        let _ = health.wait_for(|health| *health == Health::Offline).await;
    }

    /// Parks a call while the link is known to be down.
    ///
    /// Firing into a dead link is how the shell used to end up with panes stuck
    /// on "Loading…" behind requests that could never land, so a call waits for
    /// the link to be confirmed back instead. Bounded: past the hold it fails
    /// like any other offline call, and the caller's own error path runs.
    async fn await_link(&self) -> Result<(), Error> {
        if self.health() != Health::Offline {
            return Ok(());
        }
        let mut health = self.health.subscribe();
        let back = health.wait_for(|health| *health != Health::Offline);
        match tokio::time::timeout(OFFLINE_HOLD, back).await {
            Ok(Ok(_)) => Ok(()),
            _ => Err(Error::Offline(format!(
                "held {}s waiting for the network",
                OFFLINE_HOLD.as_secs()
            ))),
        }
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
            self.await_link().await?;
            // A call already on the wire when the link goes has nothing to wait
            // for but its own timeout. Cutting it loose the moment the shell
            // says the network is gone is what keeps a pane from sitting on
            // "Loading…" for half a minute; the retry below re-enters the hold
            // and goes out again once the link is confirmed back. Only for
            // calls that are safe to repeat — a send that may have landed must
            // not be fired twice.
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_once(&req) => result,
                    () = self.link_dropped() => {
                        Err(Error::Offline("link dropped mid-call".into()))
                    }
                }
            } else {
                self.execute_once(&req).await
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
        builder = builder.timeout(API_TIMEOUT);

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
        // Media never waits: loads are serialized behind one lock, so holding
        // here would park every later image behind a link that may be gone for
        // minutes. Failing now is free — the loader retries on its own delay.
        if self.health() == Health::Offline {
            return Err(Error::Offline("network is down".into()));
        }
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
    if error.is_connect() || error.is_timeout() || error.is_connection_reset() || error.is_request()
    {
        return Error::Offline(error.without_uri().to_string());
    }
    Error::Transport(format!("send: {error}"))
}

fn retryable_error(error: &Error) -> bool {
    // A link that just came back hands out one more failure before it settles:
    // the pooled socket is dead, or DNS has not caught up. Retrying re-enters
    // the hold, so the second attempt waits for the link to be confirmed again
    // rather than firing into the same gap.
    error.is_offline()
        || matches!(error, Error::RateLimited { .. })
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
        assert!(retryable_error(&Error::Offline(
            "client error (Connect)".into()
        )));
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

    /// Requests are held, not fired, while the link is known to be down — and
    /// released the moment recovery is confirmed. Firing anyway is what left
    /// panes stuck on "Loading…" behind a request that could never land.
    #[tokio::test]
    async fn a_call_is_held_while_the_link_is_down_and_released_when_it_returns() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"ok\":true}\r\n",
                )
                .await
                .unwrap();
        });

        let transport = Transport::new("secret-cookie").unwrap();
        transport.set_health(Health::Offline);
        let held = tokio::spawn({
            let transport = transport.clone();
            async move {
                transport
                    .execute(PreparedRequest {
                        method: "POST",
                        url: format!("http://{address}/api/client.dms"),
                        headers: Vec::new(),
                        body: RequestBody::Form(Vec::new()),
                    })
                    .await
            }
        });

        // Nothing has been sent: the server is still waiting to be accepted.
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(!held.is_finished(), "the call went out over a dead link");

        // The shell confirms the network is back.
        transport.set_health(Health::Unknown);
        let value = tokio::time::timeout(Duration::from_secs(5), held)
            .await
            .expect("released")
            .unwrap()
            .unwrap();
        assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
        server.await.unwrap();
    }

    /// The whole cycle a dropped link puts a call through: cut loose mid-flight
    /// rather than left to time out, held until recovery is confirmed, then
    /// sent again. The pane behind it fills in instead of staying on "Loading…".
    #[tokio::test]
    async fn a_call_cut_loose_by_a_drop_is_held_and_then_sent_again() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            // The first connection is the one the drop cuts loose: read the
            // request, answer nothing.
            let (mut dead, _) = listener.accept().await.unwrap();
            dead.read(&mut vec![0; 8192]).await.unwrap();
            let (mut socket, _) = listener.accept().await.unwrap();
            socket.read(&mut vec![0; 8192]).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"ok\":true}\r\n",
                )
                .await
                .unwrap();
        });

        let transport = Transport::new("secret-cookie").unwrap();
        let call = tokio::spawn({
            let transport = transport.clone();
            async move {
                transport
                    .execute(PreparedRequest {
                        method: "GET",
                        url: format!("http://{address}/api/conversations.history?"),
                        headers: Vec::new(),
                        body: RequestBody::Form(Vec::new()),
                    })
                    .await
            }
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        transport.set_health(Health::Offline);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!call.is_finished(), "gave up instead of holding");

        transport.set_health(Health::Unknown);
        let value = tokio::time::timeout(Duration::from_secs(5), call)
            .await
            .expect("released well inside the request timeout")
            .unwrap()
            .unwrap();
        assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
        server.await.unwrap();
    }

    /// The hold is bounded. A confirmation that never comes must not turn into
    /// the very hang this path exists to remove.
    #[tokio::test(start_paused = true)]
    async fn a_held_call_gives_up_rather_than_hanging_forever() {
        let transport = Transport::new("secret-cookie").unwrap();
        transport.set_health(Health::Offline);
        let error = transport
            .execute(PreparedRequest {
                method: "POST",
                url: "http://127.0.0.1:1/api/client.dms".to_owned(),
                headers: Vec::new(),
                body: RequestBody::Form(Vec::new()),
            })
            .await
            .unwrap_err();
        assert!(error.is_offline(), "{error}");
    }

    /// Media is the exception: loads are serialized, so holding one would park
    /// every later image behind it. It fails immediately instead, without
    /// putting a doomed request on the wire.
    #[tokio::test]
    async fn a_media_fetch_fails_immediately_instead_of_holding_the_loader() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let transport = Transport::new("secret-cookie").unwrap();
        transport.set_health(Health::Offline);
        let error = transport
            .get_bytes(
                &format!("http://{address}/avatar.png"),
                "super-platinum-test",
            )
            .await
            .unwrap_err();
        assert!(error.is_offline(), "{error}");

        let accepted = tokio::time::timeout(Duration::from_millis(150), listener.accept()).await;
        assert!(accepted.is_err(), "a doomed fetch still went out");
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
