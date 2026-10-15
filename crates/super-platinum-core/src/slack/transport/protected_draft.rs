//! Bounded response seam for explicitly protected draft operations only.
//! Existing generic Transport and ordinary draft mutation callers are unchanged.
use super::*;

pub const MAX_PROTECTED_DRAFT_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
#[derive(Debug)]
pub enum ProtectedDraftFailure {
    /// No attempt was handed to the HTTP executor by this invocation.
    NotRequested(Error),
    /// At least one HTTP attempt began. An error never proves absence/delivery failure.
    Response(Error),
}
impl ProtectedDraftFailure {
    pub fn requested(&self) -> bool {
        matches!(self, Self::Response(_))
    }
    pub fn error(&self) -> &Error {
        match self {
            Self::NotRequested(e) | Self::Response(e) => e,
        }
    }
    pub fn into_error(self) -> Error {
        match self {
            Self::NotRequested(e) | Self::Response(e) => e,
        }
    }
}
impl Transport {
    pub async fn execute_protected_draft_mutation<F: Fn() -> bool>(
        &self,
        request: PreparedRequest,
        current: F,
    ) -> Result<Value, ProtectedDraftFailure> {
        if !allowed(&request, false) {
            return Err(ProtectedDraftFailure::NotRequested(unsupported()));
        }
        if !current() {
            return Err(ProtectedDraftFailure::NotRequested(retired()));
        }
        if self.health() == Health::Offline {
            return Err(ProtectedDraftFailure::NotRequested(Error::Offline(
                "protected draft not sent while offline".into(),
            )));
        }
        let mut dispatched = false;
        self.protected_draft_once(&request, &current, &mut dispatched)
            .await
            .map_err(|failure| match failure {
                ProtectedDraftFailure::Response(Error::Offline(_)) => {
                    ProtectedDraftFailure::Response(Error::Transport(
                        "protected draft response lost after dispatch".into(),
                    ))
                }
                other => other,
            })
    }
    pub async fn execute_protected_draft_read<F: Fn() -> bool>(
        &self,
        request: PreparedRequest,
        current: F,
    ) -> Result<Value, ProtectedDraftFailure> {
        if !allowed(&request, true) {
            return Err(ProtectedDraftFailure::NotRequested(unsupported()));
        }
        // The generic PreparedRequest policy searches URL substrings. Protected
        // admission instead derives retry eligibility from this exact endpoint.
        let retry_safe = url::Url::parse(&request.url).is_ok_and(|url| {
            matches!(
                url.path(),
                "/api/conversations.history" | "/api/conversations.replies"
            )
        });
        let mut dispatched = false;
        let mut attempt = 0;
        loop {
            if !current() {
                return Err(disposition(dispatched, retired()));
            }
            self.await_link()
                .await
                .map_err(|error| disposition(dispatched, error))?;
            // A read may wait for connectivity, but its owner cannot be renewed
            // by that wait or by a retry. Check again immediately before HTTP.
            if !current() {
                return Err(disposition(dispatched, retired()));
            }
            let result = if retry_safe {
                let selected = tokio::select! {
                    biased;
                    result = self.protected_draft_once(&request, &current, &mut dispatched) => Some(result),
                    () = self.link_dropped() => None,
                };
                selected.unwrap_or_else(|| {
                    Err(disposition(
                        dispatched,
                        Error::Offline("link dropped during protected read".into()),
                    ))
                })
            } else {
                self.protected_draft_once(&request, &current, &mut dispatched)
                    .await
            };
            match result {
                Err(ref failure)
                    if retry_safe
                        && matches!(failure, ProtectedDraftFailure::Response(_))
                        && retryable_error(failure.error())
                        && !known_limit(failure.error())
                        && attempt < 2 =>
                {
                    attempt += 1;
                    tokio::time::sleep(retry_delay(failure.error(), attempt)).await;
                }
                result => return result,
            }
        }
    }
    async fn protected_draft_once<F: Fn() -> bool>(
        &self,
        request: &PreparedRequest,
        current: &F,
        dispatched: &mut bool,
    ) -> Result<Value, ProtectedDraftFailure> {
        let http = self
            .single_attempt_client()
            .map_err(|e| disposition(*dispatched, e))?;
        let mut builder = http.post(&request.url).timeout(API_TIMEOUT);
        for (key, value) in &request.headers {
            if !key.eq_ignore_ascii_case("content-type") {
                builder = builder.header(key.as_str(), value.as_str());
            }
        }
        let RequestBody::Form(fields) = &request.body else {
            return Err(disposition(*dispatched, unsupported()));
        };
        let prepared = builder
            .header("Cookie", self.cookie())
            .form(fields)
            .build()
            .map_err(|_| {
                disposition(
                    *dispatched,
                    Error::Transport("invalid protected draft request".into()),
                )
            })?;
        if !current() {
            return Err(disposition(*dispatched, retired()));
        }
        if self.health() == Health::Offline {
            return Err(disposition(
                *dispatched,
                Error::Offline("protected draft not sent while offline".into()),
            ));
        }
        *dispatched = true;
        let response = http
            .execute(prepared)
            .await
            .map_err(|e| ProtectedDraftFailure::Response(self.protected_draft_error(e)))?;
        self.set_health(Health::Online);
        let status = response.status().as_u16();
        let retry_after_secs = retry_after_secs(response.headers());
        if status == 429 {
            return Err(ProtectedDraftFailure::Response(Error::RateLimited {
                retry_after_secs,
            }));
        }
        if !(200..300).contains(&status) {
            return Err(ProtectedDraftFailure::Response(Error::HttpStatus {
                status,
                retry_after_secs,
            }));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PROTECTED_DRAFT_RESPONSE_BYTES as u64)
        {
            return Err(ProtectedDraftFailure::Response(oversized()));
        }
        let mut body = Body::default();
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk
                .map_err(|e| ProtectedDraftFailure::Response(self.protected_draft_error(e)))?;
            body.push(&chunk).map_err(ProtectedDraftFailure::Response)?;
        }
        // Preserve a complete original response even if the owner retired while
        // it arrived. Captured completion may persist it; projection still checks
        // the host owner. A partial/oversized body is never decoded or returned.
        body.finish().map_err(ProtectedDraftFailure::Response)
    }
    fn protected_draft_error(&self, error: wreq::Error) -> Error {
        if send_error(error.without_uri()).is_offline() {
            self.set_health(Health::Offline);
            Error::Offline("protected draft response unavailable".into())
        } else {
            Error::Transport("protected draft response unavailable".into())
        }
    }
}
fn disposition(dispatched: bool, error: Error) -> ProtectedDraftFailure {
    if dispatched {
        ProtectedDraftFailure::Response(error)
    } else {
        ProtectedDraftFailure::NotRequested(error)
    }
}
fn known_limit(error: &Error) -> bool {
    matches!(
        error,
        Error::RateLimited { .. }
            | Error::HttpStatus {
                retry_after_secs: Some(_),
                ..
            }
    )
}
fn unsupported() -> Error {
    Error::Api("unsupported_protected_draft_request".into())
}
fn retired() -> Error {
    Error::Transport("protected draft owner retired".into())
}
fn oversized() -> Error {
    Error::Transport("protected draft response exceeds byte limit".into())
}
#[derive(Default)]
struct Body(Vec<u8>);
impl Body {
    fn push(&mut self, chunk: &[u8]) -> Result<(), Error> {
        if chunk.len() > MAX_PROTECTED_DRAFT_RESPONSE_BYTES - self.0.len() {
            return Err(oversized());
        }
        self.0.extend_from_slice(chunk);
        Ok(())
    }
    fn finish(self) -> Result<Value, Error> {
        // Reuse the already-public pure recursive decoder. It does not touch
        // slash state/admission; duplicate keys at every depth are rejected
        // before Value could erase contradictory receipt evidence.
        let value =
            crate::slash_commands::codec::decode(&self.0, MAX_PROTECTED_DRAFT_RESPONSE_BYTES)
                .map_err(|_| {
                    Error::Transport("invalid or ambiguous protected draft JSON response".into())
                })?;
        if !value.is_object() {
            return Err(Error::Transport(
                "invalid protected draft response object".into(),
            ));
        }
        Ok(value)
    }
}
fn allowed(request: &PreparedRequest, read: bool) -> bool {
    request.method == "POST"
        && matches!(request.body, RequestBody::Form(_))
        && url::Url::parse(&request.url).is_ok_and(|url| {
            let origin = url.scheme() == "https"
                && url
                    .host_str()
                    .is_some_and(|host| host.ends_with(".slack.com"))
                && url.port_or_known_default() == Some(443);
            #[cfg(test)]
            let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
            origin
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && if read {
                    matches!(
                        url.path(),
                        "/api/drafts.info"
                            | "/api/conversations.history"
                            | "/api/conversations.replies"
                    )
                } else {
                    matches!(
                        url.path(),
                        "/api/drafts.create"
                            | "/api/drafts.update"
                            | "/api/drafts.delete"
                            | "/api/chat.postMessage"
                    )
                }
        })
}
#[cfg(test)]
#[path = "protected_draft_tests.rs"]
mod tests;
