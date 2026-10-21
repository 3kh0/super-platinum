//! Source DW51/GGwr retries, separate from workflow once-only executors. Each
//! actual attempt is observed, even if the owner retires after dispatch.
use super::*;
use crate::slack::api::avatar::{self, Attempt, AvatarRequest, Outcome};

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
    pub async fn execute_workflow_avatar(&self, request: &AvatarRequest) -> Result<Value, Error> {
        self.execute_workflow_avatar_current(request, || true, |_| {})
            .await
    }

    /// The host must retain observations and provide its original account,
    /// session, transport and modal-generation predicate. A dropped future or
    /// retired owner does not imply the server undid an already sent upload.
    pub async fn execute_workflow_avatar_current(
        &self,
        request: &AvatarRequest,
        current: impl Fn() -> bool,
        mut observe: impl FnMut(Attempt),
    ) -> Result<Value, Error> {
        self.avatar_execute(request, current, &mut observe, Duration::from_millis(100))
            .await
    }

    async fn avatar_execute(
        &self,
        request: &AvatarRequest,
        current: impl Fn() -> bool,
        observe: &mut impl FnMut(Attempt),
        poll: Duration,
    ) -> Result<Value, Error> {
        if !allowed(request) {
            return Err(Error::Api("avatar_not_sent_invalid_request".into()));
        }
        for number in 1..=3 {
            if !current() {
                return Err(Error::Api("avatar_not_sent_retired".into()));
            }
            if self.health() == Health::Offline {
                return Err(if number == 1 {
                    Error::Offline("Avatar request was not sent while offline".into())
                } else {
                    Error::Transport(
                        "Avatar retry stopped while offline; prior outcome retained".into(),
                    )
                });
            }
            let http = self.single_attempt_client()?;
            let mut url = url::Url::parse(&request.request.url)
                .map_err(|_| Error::Api("avatar_not_sent_invalid_request".into()))?;
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
            for (k, v) in &request.request.headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
            let built = builder
                .header("Cookie", self.cookie())
                .multipart(form)
                .build()
                .map_err(|_| Error::Api("avatar_not_sent_build".into()))?;
            if !current() {
                return Err(Error::Api("avatar_not_sent_retired".into()));
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
                        return Err(Error::Transport("Avatar owner retired after dispatch; outcome unknown".into()));
                    }
                }
            };
            let mut step = match result {
                Ok(Ok((status, retry, bytes))) => classify(request, number, status, retry, bytes),
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
                        "Avatar response was lost after dispatch".into(),
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
                    "Avatar owner retired after dispatch; response retained".into(),
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
            step.attempt.retry_after_millis = delay.map(|d| d.as_millis() as u64);
            observe(step.attempt);
            let Some(delay) = delay else {
                return step.result;
            };
            // Neither a rate limit nor a lost reply is silently turned into ACK.
            // Source permits another attempt, but only the original host can own it.
            let until = tokio::time::Instant::now() + delay;
            while tokio::time::Instant::now() < until {
                if !current() {
                    return Err(Error::Transport(
                        "Avatar owner retired before retry; prior outcome retained".into(),
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

fn multipart(request: &AvatarRequest) -> Result<wreq::multipart::Form, Error> {
    let RequestBody::Form(fields) = &request.request.body else {
        return Err(Error::Api("avatar_not_sent_invalid_request".into()));
    };
    let mut form = wreq::multipart::Form::new().text("token", fields[0].1.clone());
    if let Some(image) = &request.image {
        let part = wreq::multipart::Part::bytes(image.bytes().to_vec())
            .file_name(image.filename().to_owned())
            .mime_str(image.mime())
            .map_err(|_| Error::Api("avatar_not_sent_invalid_mime".into()))?;
        form = form.part("image", part);
    }
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
fn classify(
    request: &AvatarRequest,
    number: u8,
    status: u16,
    retry: Option<u64>,
    bytes: Vec<u8>,
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
        result: Err(Error::Transport("Avatar response was not usable".into())),
        retry: false,
        rate: false,
        server_delay: retry,
    };
    if status != 200 {
        step.retry = matches!(status, 429 | 503 | 504);
        step.rate = status == 429;
        if (400..500).contains(&status) {
            step.attempt.outcome = Outcome::Rejected;
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
            step.retry = code == "internal_error" || step.rate;
            if code != "internal_error" {
                step.attempt.outcome = Outcome::Rejected;
                step.attempt.may_have_applied = false;
            }
            // Do not expose arbitrary server error strings (possibly credentials).
            step.result = Err(if step.rate {
                Error::RateLimited {
                    retry_after_secs: retry,
                }
            } else if code == "internal_error" {
                Error::Transport("Avatar server internal error; application unknown".into())
            } else {
                Error::Api("avatar_api_rejected".into())
            });
        }
        return step;
    }
    let valid = if request.image.is_some() {
        avatar::UploadReply::parse(&raw).map(|_| ())
    } else {
        avatar::CropReply::parse(&raw).map(|_| ())
    };
    if valid.is_ok() {
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
        return Duration::from_secs(seconds.min(180));
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

fn allowed(request: &AvatarRequest) -> bool {
    let req = &request.request;
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
        || url.path() != format!("/api/{}", request.method())
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
    if request.image.is_some() {
        fields.len() == 2 && fields[1] == ("_x_reason".into(), avatar::UPLOAD_REASON.into())
    } else {
        fields.len() == 4
            && fields[1].0 == "avatar"
            && !fields[1].1.trim().is_empty()
            && fields[2].0 == "cropbox"
            && valid_cropbox(&fields[2].1)
            && fields[3] == ("_x_reason".into(), avatar::CROP_REASON.into())
    }
}
fn valid_cropbox(value: &str) -> bool {
    let p = value
        .split(',')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>();
    p.is_ok_and(|p| {
        p.len() == 3
            && (128..=2000).contains(&p[2])
            && p[0].checked_add(p[2]).is_some_and(|v| v <= 2000)
            && p[1].checked_add(p[2]).is_some_and(|v| v <= 2000)
    })
}

#[cfg(test)]
mod tests;
