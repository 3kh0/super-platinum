//! Reaction-only physical observation. Wire compatibility is never authority.
use super::*;
mod endpoints;
pub use endpoints::RootReactionRead;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionAdvice {
    None,
    Seconds(u64),
    Permanent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionRequestKind {
    Read,
    Write,
}
pub enum ReactionWireEvent {
    Attempted(ReactionRequestKind),
    Response {
        kind: ReactionRequestKind,
        status: u16,
        advice: ReactionAdvice,
    },
    ApiRateLimit(ReactionRequestKind),
    Completed {
        kind: ReactionRequestKind,
        result: Result<Value, Error>,
    },
}
#[derive(Debug)]
pub enum ObservedReactionFailure {
    NotRequested(Error),
    Unknown(Error),
}

// Complete original header list, not the generic first-header decoder.
// Unsupported dates/bytes refuse an invented finite delay. Keep maximum advice.
fn advice(headers: &HeaderMap, status: u16) -> ReactionAdvice {
    let mut maximum = None;
    for raw in headers.get_all("retry-after").iter() {
        let Ok(raw) = raw.to_str() else {
            return ReactionAdvice::Permanent;
        };
        let raw = raw.trim_matches([' ', '\t']);
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
            return ReactionAdvice::Permanent;
        }
        let Some(value) = raw.bytes().try_fold(0u64, |n, b| {
            n.checked_mul(10)?.checked_add(u64::from(b - b'0'))
        }) else {
            return ReactionAdvice::Permanent;
        };
        maximum = Some(maximum.unwrap_or(0).max(value.max(1)));
    }
    maximum.map_or_else(
        || {
            if status == 429 {
                ReactionAdvice::Seconds(60)
            } else {
                ReactionAdvice::None
            }
        },
        ReactionAdvice::Seconds,
    )
}
fn retry_seconds(advice: ReactionAdvice) -> Option<u64> {
    match advice {
        ReactionAdvice::None => None,
        ReactionAdvice::Seconds(s) => Some(s),
        ReactionAdvice::Permanent => Some(u64::MAX),
    }
}

impl Transport {
    /// Private mechanics; each public purpose supplies exact validation and
    /// original-owner fences. No retry/hold/redirect or cancellation after start.
    pub(super) async fn execute_reaction_observed(
        &self,
        request: PreparedRequest,
        kind: ReactionRequestKind,
        mut current: impl FnMut() -> bool,
        permitted: fn(&PreparedRequest) -> bool,
        mut observe: impl FnMut(ReactionWireEvent),
    ) -> Result<Value, ObservedReactionFailure> {
        use ObservedReactionFailure::{NotRequested, Unknown};
        if !permitted(&request) {
            return Err(NotRequested(Error::Api("invalid_reaction_request".into())));
        }
        if self.health() == Health::Offline || !current() {
            return Err(NotRequested(Error::Offline(
                "Reaction request was not started".into(),
            )));
        }
        let http = self.single_attempt_client().map_err(NotRequested)?;
        let mut builder = http
            .post(&request.url)
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none());
        for (name, value) in &request.headers {
            if !name.eq_ignore_ascii_case("content-type") {
                builder = builder.header(name.as_str(), value.as_str());
            }
        }
        builder = builder.header("Cookie", self.cookie());
        let builder = match &request.body {
            RequestBody::Form(fields) => builder.form(fields),
            RequestBody::Json(value) => builder.json(value),
        };
        let request = builder
            .build()
            .map_err(|_| NotRequested(Error::Api("invalid_reaction_request".into())))?;
        if self.health() == Health::Offline || !current() {
            return Err(NotRequested(Error::Offline(
                "Reaction request was not started".into(),
            )));
        }
        let result = tokio::time::timeout(API_TIMEOUT, async {
            // No await/construction between final original fence and HTTP start.
            observe(ReactionWireEvent::Attempted(kind));
            let response = http.execute(request).await.map_err(|error| {
                if send_error(error).is_offline() {
                    self.set_health(Health::Offline);
                }
                Error::Offline("Reaction response was lost after HTTP start".into())
            })?;
            self.set_health(Health::Online);
            let status = response.status().as_u16();
            let advice = advice(response.headers(), status);
            observe(ReactionWireEvent::Response {
                kind,
                status,
                advice,
            });
            if status == 429 {
                return Err(Error::RateLimited {
                    retry_after_secs: retry_seconds(advice),
                });
            }
            if !(200..300).contains(&status) {
                return Err(Error::HttpStatus {
                    status,
                    retry_after_secs: retry_seconds(advice),
                });
            }
            const LIMIT: usize = 1024 * 1024;
            if response
                .content_length()
                .is_some_and(|len| len > LIMIT as u64)
            {
                return Err(Error::Transport("Reaction response too large".into()));
            }
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|_| Error::Transport("Reaction body was interrupted".into()))?;
                if chunk.len() > LIMIT - bytes.len() {
                    return Err(Error::Transport("Reaction response too large".into()));
                }
                bytes.extend_from_slice(&chunk);
            }
            let value = crate::slash_commands::codec::decode(&bytes, LIMIT)
                .map_err(|_| Error::Transport("Invalid reaction response".into()))?;
            if !value.is_object() {
                return Err(Error::Transport("Invalid reaction response".into()));
            }
            if value["ok"] == false && value["error"] == "ratelimited" {
                observe(ReactionWireEvent::ApiRateLimit(kind));
                return Err(Error::RateLimited {
                    retry_after_secs: Some(60),
                });
            }
            // Slack explicitly permits partial success for these server errors;
            // they cannot release a physical mutation as a known rejection.
            if value["ok"] == false
                && matches!(
                    value["error"].as_str(),
                    Some("internal_error" | "fatal_error")
                )
            {
                return Err(Error::Api(value["error"].as_str().unwrap().into()));
            }
            Ok(value)
        })
        .await
        .unwrap_or_else(|_| Err(Error::Offline("Reaction response timed out".into())));
        observe(ReactionWireEvent::Completed {
            kind,
            result: result.clone(),
        });
        result.map_err(Unknown)
    }
}

#[cfg(test)]
mod tests;
