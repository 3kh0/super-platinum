//! PDF-purpose one-attempt reads. Returned bytes/JSON are never publication.
use super::*;
use crate::{
    file_preview::isolated_pdf::{self, MAX_BYTES, Projection},
    slack::api::file_pdf::{self, Request},
};
use std::collections::{BTreeMap, BTreeSet};
const MAX_METADATA: usize = 1024 * 1024;

fn unavailable() -> Error {
    Error::Api("pdf_preview_unavailable".into())
}
fn retired() -> Error {
    Error::Offline("PDF preview was not requested".into())
}

impl Transport {
    /// No link hold, redirect or retry. After dispatch this retains the bounded
    /// original outcome (including late429) even if its native caller retires.
    /// The caller must check its original purpose before publishing anything.
    pub async fn execute_file_pdf_current(
        &self,
        request: &Request,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        if !allowed(request) {
            return Err(unavailable());
        }
        if self.health() == Health::Offline || !current() {
            return Err(retired());
        }
        let http = self.single_attempt_client()?;
        #[cfg(test)]
        let fixture_http = loopback_client(&request.inner.url)?;
        #[cfg(test)]
        let http = fixture_http.as_ref().unwrap_or(http);
        let mut builder = http
            .post(&request.inner.url)
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none())
            .header("Cookie", self.cookie());
        for (k, v) in &request.inner.headers {
            if !k.eq_ignore_ascii_case("content-type") {
                builder = builder.header(k.as_str(), v.as_str());
            }
        }
        let RequestBody::Form(fields) = &request.inner.body else {
            return Err(unavailable());
        };
        if request.multipart {
            let mut form = wreq::multipart::Form::new();
            for (k, v) in fields {
                form = form.text(k.clone(), v.clone());
            }
            builder = builder.multipart(form);
        } else {
            builder = builder.form(fields);
        }
        let wire = builder.build().map_err(|_| unavailable())?;
        // No await separates this original-owner fence from the one dispatch.
        if self.health() == Health::Offline || !current() {
            return Err(retired());
        }
        let bytes = self.pdf_response(http, wire, MAX_METADATA, false).await?;
        let raw = crate::slash_commands::codec::decode(&bytes, MAX_METADATA)
            .map_err(|_| unavailable())?;
        if !raw.is_object() {
            return Err(unavailable());
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

    /// Authenticated full-buffer content only, not original-download identity.
    /// Precheck is deliberately not PDF decode or a ready/native receipt.
    pub async fn get_file_pdf_current(
        &self,
        projection: &Projection,
        current: impl Fn() -> bool,
    ) -> Result<Vec<u8>, Error> {
        if !isolated_pdf::valid_content_url(projection.url()) {
            return Err(unavailable());
        }
        if self.health() == Health::Offline || !current() {
            return Err(retired());
        }
        let http = self.single_attempt_client()?;
        #[cfg(test)]
        let fixture_http = loopback_client(projection.url())?;
        #[cfg(test)]
        let http = fixture_http.as_ref().unwrap_or(http);
        let wire = http
            .get(projection.url())
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none())
            .header("Cookie", self.cookie())
            .header(
                "User-Agent",
                crate::slack::xparams::Identity::from_capture().user_agent,
            )
            .header("Accept", "application/pdf")
            .header("Accept-Encoding", "identity")
            .build()
            .map_err(|_| unavailable())?;
        if self.health() == Health::Offline || !current() {
            return Err(retired());
        }
        let bytes = self.pdf_response(http, wire, MAX_BYTES, true).await?;
        projection
            .validate_bytes(&bytes)
            .map_err(|_| unavailable())?;
        Ok(bytes)
    }

    async fn pdf_response(
        &self,
        http: &wreq::Client,
        wire: wreq::Request,
        limit: usize,
        full: bool,
    ) -> Result<Vec<u8>, Error> {
        tokio::time::timeout(API_TIMEOUT, async {
            let response = http.execute(wire).await.map_err(|e| {
                let mapped = send_error(e.without_uri());
                if mapped.is_offline() {
                    self.set_health(Health::Offline);
                    Error::Offline("PDF preview connection unavailable".into())
                } else {
                    Error::Transport("PDF preview response unavailable".into())
                }
            })?;
            self.set_health(Health::Online);
            let status = response.status().as_u16();
            let retry_after_secs = pdf_retry_after(response.headers());
            if status == 429 {
                return Err(Error::RateLimited { retry_after_secs });
            }
            if !(200..300).contains(&status) || retry_after_secs.is_some() || full && status != 200
            {
                return Err(Error::HttpStatus {
                    status,
                    retry_after_secs,
                });
            }
            // The shared bounded reader can include an upstream URI in a
            // stream error. This purpose never retains signed content URLs.
            read_media_body(response, limit).await.map_err(|_| {
                Error::Transport("PDF preview body unavailable or exceeds size limit".into())
            })
        })
        .await
        .map_err(|_| Error::Transport("PDF preview response timed out".into()))?
    }
}

fn pdf_retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get_all("retry-after")
        .iter()
        .map(|v| {
            v.to_str()
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                // Any unknown/date/overflow value prevents a shorter hold.
                // Native Instant overflow is a permanent closed reservation.
                .unwrap_or(u64::MAX)
        })
        .max()
}
fn allowed(request: &Request) -> bool {
    let req = &request.inner;
    let Ok(url) = url::Url::parse(&req.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"));
    #[cfg(test)]
    let origin = origin || url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    if req.method != "POST"
        || !origin
        || url.as_str() != req.url
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path()
            != if request.multipart {
                "/api/teams.authorized.ip.range.checkIp"
            } else {
                "/api/files.info"
            }
    {
        return false;
    }
    let mut query = BTreeSet::new();
    for (k, v) in url.query_pairs() {
        if !query.insert(k.to_string())
            || !matches!(
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
            || k == "_x_num_retries" && v != "0"
        {
            return false;
        }
    }
    let mut headers = BTreeSet::new();
    if req.headers.len() != if request.multipart { 5 } else { 6 } {
        return false;
    }
    for (k, v) in &req.headers {
        let key = k.to_ascii_lowercase();
        if !headers.insert(key.clone())
            || v.is_empty()
            || !matches!(
                key.as_str(),
                "sec-ch-ua-platform"
                    | "referer"
                    | "user-agent"
                    | "sec-ch-ua"
                    | "sec-ch-ua-mobile"
                    | "content-type"
            )
            || key == "content-type"
                && (request.multipart || v != "application/x-www-form-urlencoded")
        {
            return false;
        }
    }
    if !request.multipart && !headers.contains("content-type") {
        return false;
    }
    let RequestBody::Form(rows) = &req.body else {
        return false;
    };
    let mut fields = BTreeMap::new();
    for (k, v) in rows {
        if fields.insert(k.as_str(), v.as_str()).is_some() {
            return false;
        }
    }
    fields.len() == (if request.multipart { 2 } else { 5 })
        && fields.get("token").is_some_and(|v| {
            !v.is_empty()
                && v.len() <= 16 * 1024
                && !v.chars().any(|c| c.is_whitespace() || c.is_control())
        })
        && fields.get("file").is_some_and(|v| file_pdf::valid_id(v))
        && (request.multipart
            || fields.get("count") == Some(&"1")
                && fields.get("page") == Some(&"1")
                && fields.get("truncate") == Some(&"false"))
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
        .map_err(|_| unavailable())
}

#[cfg(test)]
mod tests;
