//! Dedicated text multipart read. Native policy: one attempt, 30-second bound,
//! no redirect/retry/enterprise fallback; explicit user/outage retry owns renewal.
use super::*;
use crate::slack::api::file_pages::{self, Request};

impl Transport {
    pub async fn execute_file_page_current(
        &self,
        request: &Request,
        current: impl Fn() -> bool,
        mut observe: impl FnMut(&Result<Value, Error>),
    ) -> Result<Value, Error> {
        if !allowed(request) || !current() {
            return Err(unavailable());
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Files request was not sent while offline".into(),
            ));
        }
        let RequestBody::Form(fields) = &request.inner.body else {
            return Err(unavailable());
        };
        let form = fields
            .iter()
            .fold(wreq::multipart::Form::new(), |form, (k, v)| {
                form.text(k.clone(), v.clone())
            });
        let http = self.single_attempt_client()?;
        let mut builder = http
            .post(&request.inner.url)
            .timeout(Duration::from_secs(30));
        for (key, value) in &request.inner.headers {
            if key.eq_ignore_ascii_case("content-type") {
                return Err(unavailable());
            }
            builder = builder.header(key.as_str(), value.as_str());
        }
        let wire = builder
            .header("Cookie", self.cookie())
            .multipart(form)
            .build()
            .map_err(|_| unavailable())?;
        if !current() {
            return Err(unavailable());
        }
        // A dispatched read may finish after owner retirement. Observe its real
        // rate response BEFORE rejecting publication; no successor gets its page.
        let result = match tokio::time::timeout(Duration::from_secs(30), async {
            let response = http.execute(wire).await.map_err(|_| {
                self.set_health(Health::Offline);
                Error::Offline("Files connection unavailable".into())
            })?;
            self.set_health(Health::Online);
            let status = response.status().as_u16();
            let retry_after_secs = retry_after_secs(response.headers());
            if status == 429 {
                return Err(Error::RateLimited { retry_after_secs });
            }
            if !(200..300).contains(&status) {
                return Err(Error::HttpStatus {
                    status,
                    retry_after_secs,
                });
            }
            let bytes = read_media_body(response, file_pages::MAX_REPLY_BYTES)
                .await
                .map_err(|_| Error::Transport("Files response body unavailable".into()))?;
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| Error::Transport("Files response JSON unavailable".into()))?;
            if value.get("ok") != Some(&Value::Bool(true)) {
                let code = value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("people_search_failed");
                if matches!(
                    code,
                    "ratelimited"
                        | "rate_limited"
                        | "rate_limit_exceeded"
                        | "rate_limit_reached"
                        | "ratelimit"
                        | "rate_limit"
                ) {
                    return Err(Error::RateLimited { retry_after_secs });
                }
                return Err(Error::Api(
                    if code.len() <= 64
                        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    {
                        code.into()
                    } else {
                        "people_search_failed".into()
                    },
                ));
            }
            Ok(value)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                self.set_health(Health::Offline);
                Err(Error::Offline("Files response timed out".into()))
            }
        };
        observe(&result);
        if !current() {
            return Err(unavailable());
        }
        result
    }
}
fn unavailable() -> Error {
    Error::Api("file_page_not_admitted".into())
}
fn allowed(request: &Request) -> bool {
    let Ok(url) = url::Url::parse(&request.inner.url) else {
        return false;
    };
    let destination = url.host_str() == Some(request.destination.as_str())
        && url.path() == "/api/search.modules.files"
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none();
    let secure = url.scheme() == "https"
        && request.destination.ends_with(".slack.com")
        && url.port().is_none_or(|p| p == 443);
    #[cfg(test)]
    let secure = secure || (url.scheme() == "http" && request.destination == "127.0.0.1");
    destination
        && secure
        && request.inner.method == "POST"
        && (1..=file_pages::MAX_PAGE).contains(&request.page)
        && matches!(request.inner.body, RequestBody::Form(_))
}
#[cfg(test)]
mod tests;
