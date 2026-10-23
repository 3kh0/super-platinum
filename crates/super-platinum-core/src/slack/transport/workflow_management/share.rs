//! Endpoint-specific Share multipart executor. Source DW51/GGwr retry classes. Each
//! actual attempt is observed, even if the owner retires after dispatch.
use super::*;
use crate::workflow_share::{self as share, Attempt, Outcome, Request, person};

mod group_open;
mod person_open;

const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_REPLY: usize = 1024 * 1024;
const RATE_CODES: [&str; 6] = [
    "ratelimited",
    "rate_limited",
    "rate_limit_exceeded",
    "rate_limit_reached",
    "ratelimit",
    "rate_limit",
];

impl Transport {
    /// The host must retain observations and provide its original account,
    /// session, transport and modal-generation predicate. A dropped future or
    /// retired owner does not imply the server undid an already sent mutation.
    pub async fn execute_workflow_share_current(
        &self,
        request: &Request,
        current: impl Fn() -> bool,
        mut observe: impl FnMut(Attempt),
    ) -> Result<Value, Error> {
        self.share_execute(request, current, &mut observe, Duration::from_millis(100))
            .await
    }

    async fn share_execute(
        &self,
        request: &Request,
        current: impl Fn() -> bool,
        observe: &mut impl FnMut(Attempt),
        poll: Duration,
    ) -> Result<Value, Error> {
        if !allowed(request) {
            return Err(Error::Api("workflow_share_not_sent_invalid_request".into()));
        }
        self.share_wire_execute(
            &request.request,
            current,
            observe,
            poll,
            share::acknowledged,
        )
        .await
    }

    // Closed callers validate their own endpoint and immutable fields first.
    // The engine shares retry/cancellation only; success correlation is per endpoint.
    async fn share_wire_execute(
        &self,
        request: &PreparedRequest,
        current: impl Fn() -> bool,
        observe: &mut impl FnMut(Attempt),
        poll: Duration,
        acknowledged: impl Fn(&Value) -> bool,
    ) -> Result<Value, Error> {
        for number in 1..=3 {
            if !current() {
                return Err(if number == 1 {
                    Error::Api("workflow_share_not_sent_retired".into())
                } else {
                    Error::Transport(
                        "Share owner retired before retry; prior attempts retained".into(),
                    )
                });
            }
            if self.health() == Health::Offline {
                return Err(if number == 1 {
                    Error::Offline("Share request was not sent while offline".into())
                } else {
                    Error::Transport(
                        "Share retry stopped while offline; prior outcome retained".into(),
                    )
                });
            }
            let http = self.single_attempt_client()?;
            let mut url = url::Url::parse(&request.url)
                .map_err(|_| Error::Api("workflow_share_not_sent_invalid_request".into()))?;
            let pairs = url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<Vec<_>>();
            url.set_query(None);
            for (k, v) in pairs {
                url.query_pairs_mut().append_pair(
                    &k,
                    if k == "_x_num_retries" {
                        match number {
                            1 => "0",
                            2 => "1",
                            _ => "2",
                        }
                    } else {
                        &v
                    },
                );
            }
            let form = multipart(request)?;
            let mut builder = http.post(url.as_str()).timeout(TIMEOUT);
            for (k, v) in &request.headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
            let built = builder
                .header("Cookie", self.cookie())
                .multipart(form)
                .build()
                .map_err(|_| Error::Api("workflow_share_not_sent_build".into()))?;
            if !current() {
                return Err(if number == 1 {
                    Error::Api("workflow_share_not_sent_retired".into())
                } else {
                    Error::Transport(
                        "Share owner retired before retry; prior attempts retained".into(),
                    )
                });
            }
            let mut pending = std::pin::pin!(tokio::time::timeout(TIMEOUT, async {
                let response = http.execute(built).await.map_err(|_| ())?;
                self.set_health(Health::Online);
                let status = response.status().as_u16();
                let retry = retry_after_secs(response.headers());
                let body = read_media_body(response, MAX_REPLY).await.map_err(|_| ())?;
                Ok::<_, ()>((status, retry, body))
            }));
            let result = loop {
                tokio::select! {
                    value=&mut pending=>break value,
                    _=tokio::time::sleep(poll)=>if !current() {
                        observe(Attempt{number,status:None,outcome:Outcome::Retired,raw:None,response_text:None,retry_after_millis:None,may_have_applied:true});
                        return Err(Error::Transport("Share owner retired after dispatch; outcome unknown".into()));
                    }
                }
            };
            let mut step = match result {
                Ok(Ok((status, retry, bytes))) => {
                    classify_with(number, status, retry, bytes, &acknowledged)
                }
                _ => Step {
                    attempt: Attempt {
                        number,
                        status: None,
                        outcome: Outcome::Unknown,
                        raw: None,
                        response_text: None,
                        retry_after_millis: None,
                        may_have_applied: true,
                    },
                    result: Err(Error::Transport(
                        "Share response was lost after dispatch".into(),
                    )),
                    retry: true,
                    rate: false,
                    server_delay: None,
                },
            };
            if !current() {
                step.attempt.outcome = Outcome::Retired;
                step.attempt.may_have_applied = true;
                observe(step.attempt);
                return Err(Error::Transport(
                    "Share owner retired after dispatch; response retained".into(),
                ));
            }
            let delay = (step.retry && number < 3).then(|| {
                retry_delay(
                    number,
                    step.attempt.status,
                    step.rate,
                    step.server_delay,
                    random_fraction(),
                )
            });
            step.attempt.retry_after_millis = delay
                .or_else(|| step.server_delay.map(Duration::from_secs))
                .or_else(|| step.rate.then_some(Duration::from_secs(60)))
                .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64);
            observe(step.attempt);
            let Some(delay) = delay else {
                return step.result;
            };
            // Neither a rate limit nor a lost reply is silently turned into ACK.
            // Source permits another attempt, but only the original host can own it.
            let Some(until) = tokio::time::Instant::now().checked_add(delay) else {
                return Err(Error::Transport(
                    "Share retry deadline overflow; no retry sent".into(),
                ));
            };
            while tokio::time::Instant::now() < until {
                if !current() {
                    return Err(Error::Transport(
                        "Share owner retired before retry; prior outcome retained".into(),
                    ));
                }
                tokio::time::sleep(
                    poll.min(until.saturating_duration_since(tokio::time::Instant::now())),
                )
                .await;
            }
        }
        unreachable!("third attempt returns")
    }
}

fn multipart(request: &PreparedRequest) -> Result<wreq::multipart::Form, Error> {
    let RequestBody::Form(fields) = &request.body else {
        return Err(Error::Api("workflow_share_not_sent_invalid_request".into()));
    };
    let mut form = wreq::multipart::Form::new().text("token", fields[0].1.clone());
    for (k, v) in fields.iter().skip(1) {
        form = form.text(k.clone(), v.clone());
    }
    Ok(form)
}

struct Step {
    attempt: Attempt,
    result: Result<Value, Error>,
    retry: bool,
    rate: bool,
    server_delay: Option<u64>,
}
#[cfg(test)]
fn classify(number: u8, status: u16, retry: Option<u64>, bytes: Vec<u8>) -> Step {
    classify_with(number, status, retry, bytes, &share::acknowledged)
}
fn classify_with(
    number: u8,
    status: u16,
    retry: Option<u64>,
    bytes: Vec<u8>,
    acknowledged: &impl Fn(&Value) -> bool,
) -> Step {
    let text = String::from_utf8(bytes).ok();
    let raw = text
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let mut step = Step {
        attempt: Attempt {
            number,
            status: Some(status),
            outcome: Outcome::Unknown,
            raw: raw.clone(),
            response_text: text,
            retry_after_millis: None,
            may_have_applied: true,
        },
        result: Err(Error::Transport("Share response was not usable".into())),
        retry: false,
        rate: false,
        server_delay: retry,
    };
    if status != 200 {
        step.retry = matches!(status, 429 | 503 | 504);
        step.rate = status == 429;
        if (400..500).contains(&status) {
            step.attempt.outcome = if status == 429 {
                Outcome::RateLimited
            } else {
                Outcome::Rejected
            };
            step.attempt.may_have_applied = false;
        }
        step.result = Err(if status == 429 {
            Error::RateLimited {
                retry_after_secs: retry,
            }
        } else {
            Error::HttpStatus {
                status,
                retry_after_secs: retry,
            }
        });
        return step;
    }
    let Some(raw) = raw else { return step };
    if raw.get("ok") == Some(&Value::Bool(false)) {
        if let Some(code) = raw
            .get("error")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            step.rate = RATE_CODES.contains(&code);
            // Source API200 rate-limit uses a browser dummy queue, not ordinary
            // replay. Native adaptation: report explicit retained RateLimited and
            // leave recovery to a later user action after its deadline.
            step.retry = code == "internal_error";
            if code != "internal_error" {
                step.attempt.outcome = if step.rate {
                    Outcome::RateLimited
                } else {
                    Outcome::Rejected
                };
                step.attempt.may_have_applied = false;
            }
            // Do not expose arbitrary server error strings (possibly credentials).
            step.result = Err(if step.rate {
                Error::RateLimited {
                    retry_after_secs: retry,
                }
            } else if code == "internal_error" {
                Error::Transport("Share server internal error; application unknown".into())
            } else {
                Error::Api("workflow_share_api_rejected".into())
            });
        }
        return step;
    }
    if acknowledged(&raw) {
        step.attempt.outcome = Outcome::Acknowledged;
        step.result = Ok(raw);
    }
    step
}

fn random_fraction() -> f64 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    // Uniform 53 random bits, equivalent range [0,1) to Math.random.
    let mut random = [0u8; 8];
    random[..7].copy_from_slice(&bytes[9..]);
    let n = u64::from_le_bytes(random) >> 3;
    n as f64 / (1u64 << 53) as f64
}
fn retry_delay(
    number: u8,
    status: Option<u16>,
    rate: bool,
    server: Option<u64>,
    random: f64,
) -> Duration {
    if let Some(seconds) = server.filter(|s| *s > 0) {
        // Respect the actual server deadline even beyond source
        // maxRetryAfter180 (native safety adaptation required by host policy).
        return Duration::from_secs(seconds);
    }
    let base = if rate || matches!(status, Some(429 | 503)) {
        60
    } else if status == Some(504) {
        15
    } else if status.is_some_and(|s| s >= 500) {
        30
    } else {
        1
    };
    // ZI29.qH: floor(random * (min(cap,5*2**attempts)-1)). Preserve its
    // exclusive upper bound rather than substituting a standard backoff helper.
    let upper = (180 - base).min(5 * (1u64 << number));
    let jitter = (random * (upper - 1) as f64).floor() as u64;
    Duration::from_secs((base + jitter).min(180))
}

fn allowed(request: &Request) -> bool {
    allowed_prepared(&request.request, "/api/chat.postMessage") && request.fields_current()
}
fn allowed_prepared(req: &PreparedRequest, path: &str) -> bool {
    let Ok(url) = url::Url::parse(&req.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || req.method != "POST"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != path
    {
        return false;
    }
    let mut seen = std::collections::BTreeSet::new();
    if !url.query_pairs().all(|(k, v)| {
        !v.is_empty()
            && seen.insert(k.to_string())
            && matches!(
                k.as_ref(),
                "_x_id"
                    | "_x_csid"
                    | "_x_desktop_ia"
                    | "_x_foreground"
                    | "_x_frontend_build_type"
                    | "_x_gantry"
                    | "_x_num_retries"
                    | "_x_version_ts"
                    | "fp"
                    | "slack_route"
            )
            && (k != "_x_num_retries" || v == "0")
    }) {
        return false;
    }
    if !seen.contains("_x_num_retries") {
        return false;
    }
    if req.headers.iter().any(|(k, _)| {
        matches!(
            k.to_ascii_lowercase().as_str(),
            "content-type" | "content-length" | "authorization" | "cookie" | "host"
        )
    }) {
        return false;
    }
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    if !fields
        .first()
        .is_some_and(|(k, v)| k == "token" && !v.trim().is_empty())
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests;
