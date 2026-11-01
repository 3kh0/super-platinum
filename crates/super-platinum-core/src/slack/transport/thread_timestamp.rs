//! One-attempt timestamp-purpose raw reads, never a navigation invocation.
use super::*;
use crate::slack::api::thread_timestamp::{Kind, Request};
use std::collections::{BTreeMap, BTreeSet};

const MAX_RESPONSE: usize = 1024 * 1024;
const MAX_REQUEST: usize = 32 * 1024;
fn invalid() -> Error {
    Error::Api("timestamp_navigation_request_unavailable".into())
}
fn retired() -> Error {
    Error::Offline("Timestamp navigation was not requested".into())
}

impl Transport {
    /// Two predispatch fences; no offline hold/retry/redirect. Once issued,
    /// drains the bounded ORIGINAL result, including late429 after retirement.
    /// The native purpose must separately guard every publication/invocation.
    pub async fn execute_thread_timestamp_current(
        &self,
        request: &Request,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        if !allowed(request) {
            return Err(invalid());
        }
        if self.health() == Health::Offline || !current() {
            return Err(retired());
        }
        // Preserve protected-disconnected transport refusal before test override.
        let http = self.single_attempt_client()?;
        #[cfg(test)]
        let fixture_http = loopback_client(&request.inner.prepared.url)?;
        #[cfg(test)]
        let http = fixture_http.as_ref().unwrap_or(http);
        let prepared = &request.inner.prepared;
        let mut builder = http
            .post(&prepared.url)
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none())
            .header("Cookie", self.cookie());
        for (key, value) in &prepared.headers {
            if !key.eq_ignore_ascii_case("content-type") {
                builder = builder.header(key.as_str(), value.as_str());
            }
        }
        let RequestBody::Form(fields) = &prepared.body else {
            return Err(invalid());
        };
        let wire = builder.form(fields).build().map_err(|_| invalid())?;
        if self.health() == Health::Offline || !current() {
            return Err(retired());
        }
        let bytes = tokio::time::timeout(API_TIMEOUT, async {
            let response = http.execute(wire).await.map_err(|error| {
                if send_error(error.without_uri()).is_offline() {
                    self.set_health(Health::Offline);
                    Error::Offline("Timestamp navigation connection unavailable".into())
                } else {
                    Error::Transport("Timestamp navigation response unavailable".into())
                }
            })?;
            self.set_health(Health::Online);
            let status = response.status().as_u16();
            let retry_after_secs = retry_after(response.headers());
            if status == 429 {
                return Err(Error::RateLimited { retry_after_secs });
            }
            if status != 200 || retry_after_secs.is_some() {
                return Err(Error::HttpStatus {
                    status,
                    retry_after_secs,
                });
            }
            read_media_body(response, MAX_RESPONSE).await.map_err(|_| {
                Error::Transport(
                    "Timestamp navigation body unavailable or exceeds size limit".into(),
                )
            })
        })
        .await
        .map_err(|_| Error::Transport("Timestamp navigation response timed out".into()))??;
        let raw = crate::slash_commands::codec::decode(&bytes, MAX_RESPONSE)
            .map_err(|_| Error::Api("timestamp_navigation_response_unavailable".into()))?;
        if !raw.is_object() {
            return Err(Error::Api(
                "timestamp_navigation_response_unavailable".into(),
            ));
        }
        if raw.get("ok") == Some(&Value::Bool(false))
            && raw.get("error").and_then(Value::as_str) == Some("ratelimited")
        {
            return Err(Error::RateLimited {
                retry_after_secs: None,
            });
        }
        Ok(raw)
    }
}
fn retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get_all("retry-after")
        .iter()
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(u64::MAX)
        })
        .max()
}
fn allowed(request: &Request) -> bool {
    let prepared = &request.inner.prepared;
    if !crate::thread_timestamp::channel_id(request.channel_id())
        || !crate::thread_timestamp::timestamp(request.timestamp())
        || prepared.method != "POST"
        || prepared.url.len() > 16 * 1024
    {
        return false;
    }
    let Ok(url) = url::Url::parse(&prepared.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"));
    #[cfg(test)]
    let origin = origin || url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    let path = match request.kind() {
        Kind::History => "/api/conversations.history",
        Kind::Channel => "/api/conversations.info",
    };
    if !origin
        || url.as_str() != prepared.url
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != path
    {
        return false;
    }
    let mut query = BTreeSet::new();
    for (key, value) in url.query_pairs() {
        if !query.insert(key.to_string())
            || !matches!(
                key.as_ref(),
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
            || key == "_x_num_retries" && value != "0"
        {
            return false;
        }
    }
    let mut size = prepared.url.len();
    let mut headers = BTreeSet::new();
    if prepared.headers.len() != 6 {
        return false;
    }
    for (key, value) in &prepared.headers {
        let key = key.to_ascii_lowercase();
        if !headers.insert(key.clone())
            || value.is_empty()
            || !matches!(
                key.as_str(),
                "sec-ch-ua-platform"
                    | "referer"
                    | "user-agent"
                    | "sec-ch-ua"
                    | "sec-ch-ua-mobile"
                    | "content-type"
            )
            || key == "content-type" && value != "application/x-www-form-urlencoded"
        {
            return false;
        }
        let Some(next) = size
            .checked_add(key.len())
            .and_then(|n| n.checked_add(value.len()))
        else {
            return false;
        };
        size = next;
    }
    let RequestBody::Form(rows) = &prepared.body else {
        return false;
    };
    let mut fields = BTreeMap::new();
    for (key, value) in rows {
        if fields.insert(key.as_str(), value.as_str()).is_some() {
            return false;
        }
        let Some(next) = size
            .checked_add(key.len())
            .and_then(|n| n.checked_add(value.len()))
        else {
            return false;
        };
        size = next;
    }
    if size > MAX_REQUEST
        || fields.get("channel") != Some(&request.channel_id())
        || fields.get("token").is_none_or(|v| {
            v.is_empty()
                || v.len() > 16 * 1024
                || v.chars().any(|c| c.is_whitespace() || c.is_control())
        })
    {
        return false;
    }
    match request.kind() {
        Kind::History => {
            fields.len() == 5
                && fields.get("latest") == Some(&request.timestamp())
                && fields.get("inclusive") == Some(&"true")
                && fields.get("limit") == Some(&"1")
        }
        Kind::Channel => fields.len() == 3 && fields.get("include_num_members") == Some(&"false"),
    }
}
#[cfg(test)]
fn loopback_client(input: &str) -> Result<Option<wreq::Client>, Error> {
    if url::Url::parse(input)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .as_deref()
        != Some("127.0.0.1")
    {
        return Ok(None);
    }
    wreq::Client::builder()
        .emulation(Emulation::Chrome140)
        .no_proxy()
        .redirect(wreq::redirect::Policy::none())
        .retry(wreq::retry::Policy::never())
        .build()
        .map(Some)
        .map_err(|_| invalid())
}

#[cfg(test)]
mod tests;
