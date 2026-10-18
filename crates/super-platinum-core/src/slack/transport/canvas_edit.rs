//! One-attempt canvas saves, with delivery uncertainty preserved explicitly.
use super::*;

/// Only pre-dispatch failures prove that this attempt did not reach Slack.
/// Neither an HTTP failure nor a lost response authorizes automatic replay.
#[derive(Debug)]
pub enum CanvasEditFailure {
    NotSent(Error),
    Uncertain(Error),
}

impl Transport {
    pub async fn execute_canvas_comment_bootstrap(
        &self,
        req: PreparedRequest,
    ) -> Result<Vec<u8>, CanvasEditFailure> {
        if !bootstrap_allowed(&req) {
            return Err(CanvasEditFailure::NotSent(Error::Api(
                "unsupported_canvas_bootstrap".into(),
            )));
        }
        self.execute_canvas_write(req).await
    }
    pub async fn execute_canvas_edit(
        &self,
        req: PreparedRequest,
    ) -> Result<Vec<u8>, CanvasEditFailure> {
        use CanvasEditFailure::NotSent;
        if !allowed(&req) {
            return Err(NotSent(Error::Api(
                "unsupported_canvas_edit_request".into(),
            )));
        }
        self.execute_canvas_write(req).await
    }

    /// Explicit comment resolve/reopen shares the no-hold, one-attempt boundary.
    pub async fn execute_canvas_comment_archive(
        &self,
        req: PreparedRequest,
    ) -> Result<Vec<u8>, CanvasEditFailure> {
        if !comment_allowed(&req) {
            return Err(CanvasEditFailure::NotSent(Error::Api(
                "unsupported_canvas_comment_write".into(),
            )));
        }
        self.execute_canvas_write(req).await
    }

    pub async fn execute_canvas_comment_reply(
        &self,
        req: PreparedRequest,
    ) -> Result<Vec<u8>, CanvasEditFailure> {
        if !reply_allowed(&req) {
            return Err(CanvasEditFailure::NotSent(Error::Api(
                "unsupported_canvas_reply".into(),
            )));
        }
        self.execute_canvas_write(req).await
    }

    async fn execute_canvas_write(
        &self,
        req: PreparedRequest,
    ) -> Result<Vec<u8>, CanvasEditFailure> {
        use CanvasEditFailure::{NotSent, Uncertain};
        if self.health() == Health::Offline {
            return Err(NotSent(Error::Offline(
                "canvas edit not sent while offline".into(),
            )));
        }
        let RequestBody::Form(fields) = &req.body else {
            unreachable!("validated form");
        };
        if fields
            .iter()
            .try_fold(0usize, |sum, (key, value)| {
                sum.checked_add(key.len())?.checked_add(value.len())
            })
            .is_none_or(|length| length > 512 * 1024)
        {
            return Err(NotSent(Error::Api("canvas_edit_request_too_large".into())));
        }
        let http = self.single_attempt_client().map_err(NotSent)?;
        let mut builder = http
            .post(&req.url)
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none());
        for (key, value) in &req.headers {
            if !key.eq_ignore_ascii_case("content-type") {
                builder = builder.header(key.as_str(), value.as_str());
            }
        }
        // Build before crossing the dispatch boundary so malformed headers or
        // form encoding cannot be mistaken for a potentially accepted write.
        let request = builder
            .header("Cookie", self.cookie())
            .form(fields)
            .build()
            .map_err(|_| NotSent(Error::Api("invalid_canvas_edit_request".into())))?;
        // Recheck after preparation: authorization is not queued across outages.
        if self.health() == Health::Offline {
            return Err(NotSent(Error::Offline(
                "canvas edit not sent while offline".into(),
            )));
        }
        let response = http
            .execute(request)
            .await
            .map_err(|error| Uncertain(self.canvas_edit_error(error)))?;
        self.set_health(Health::Online);
        let status = response.status();
        let retry_after_secs = retry_after_secs(response.headers());
        if status.as_u16() == 429 {
            return Err(Uncertain(Error::RateLimited { retry_after_secs }));
        }
        if !status.is_success() {
            return Err(Uncertain(Error::HttpStatus {
                status: status.as_u16(),
                retry_after_secs,
            }));
        }
        let limit = crate::canvas::wire::MAX_BYTES;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(Uncertain(Error::Transport(
                "canvas edit response exceeds size limit".into(),
            )));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| Uncertain(self.canvas_edit_error(error)))?;
            if chunk.len() > limit - bytes.len() {
                return Err(Uncertain(Error::Transport(
                    "canvas edit response exceeds size limit".into(),
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn canvas_edit_error(&self, error: wreq::Error) -> Error {
        if send_error(error).is_offline() {
            self.set_health(Health::Offline);
        }
        // Never retain a URL, response body, credentials or document content.
        Error::Transport("canvas edit response was lost after dispatch".into())
    }

    pub(super) fn single_attempt_client(&self) -> Result<&wreq::Client, Error> {
        self.http()?;
        if let Some(http) = self.canvas_http.get() {
            return Ok(http);
        }
        let http = wreq::Client::builder()
            .emulation(Emulation::Chrome140)
            .redirect(wreq::redirect::Policy::none())
            .retry(wreq::retry::Policy::never())
            .build()
            .map_err(|_| Error::Transport("single-attempt client unavailable".into()))?;
        // A concurrent initializer may win; all clones then share that pool.
        let _ = self.canvas_http.set(http);
        self.canvas_http
            .get()
            .ok_or_else(|| Error::Transport("single-attempt client unavailable".into()))
    }
}

mod annotation_guard;
fn bootstrap_allowed(req: &PreparedRequest) -> bool {
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    if fields.len() != 8 {
        return false;
    }
    let map = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    if map.len() != 8
        || map.keys().any(|k| {
            !matches!(
                *k,
                "token"
                    | "channel"
                    | "ts"
                    | "latest"
                    | "limit"
                    | "inclusive"
                    | "content_snippet"
                    | "cached_latest_updates"
            )
        })
    {
        return false;
    }
    let Some(channel) = map.get("channel").and_then(|v| v.strip_prefix('C')) else {
        return false;
    };
    let Some(control) = map.get("ts").and_then(|v| v.strip_prefix("Qpc:t:")) else {
        return false;
    };
    let control = format!("temp:{control}");
    let snippet = map.get("content_snippet").and_then(|s| {
        if s.len() > 192 * 1024 {
            None
        } else {
            serde_json::from_str::<serde_json::Value>(s).ok()
        }
    });
    if crate::canvas::comments::channel(&format!("F{channel}")).is_err()
        || map.get("token").is_none_or(|s| s.is_empty())
        || map
            .get("latest")
            .is_none_or(|s| !crate::canvas::comments::bootstrap::valid_latest(s))
        || map.get("limit") != Some(&"28")
        || map.get("inclusive") != Some(&"true")
        || map.get("cached_latest_updates") != Some(&"{}")
        || snippet
            .as_ref()
            .is_none_or(|s| !annotation_guard::snippet(s, &control))
    {
        return false;
    }
    let Ok(mut url) = url::Url::parse(&req.url) else {
        return false;
    };
    if req.method != "POST"
        || url.path() != "/api/conversations.replies"
        || !annotation_guard::query(&url)
        || url.fragment().is_some()
    {
        return false;
    }
    url.set_path("/canvas/-/edit-document");
    // Reuse the established Slack-host/loopback-test endpoint policy without
    // broadening the ordinary retry-safe conversations.replies executor.
    let mut probe = req.clone();
    probe.url = url.into();
    allowed(&probe)
}

fn reply_allowed(req: &PreparedRequest) -> bool {
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let mut seen = std::collections::HashSet::new();
    if !matches!(fields.len(), 6 | 7)
        || fields.iter().any(|(key, value)| {
            !seen.insert(key.as_str())
                || match key.as_str() {
                    "token" => value.is_empty(),
                    "channel" => {
                        !value.starts_with('C')
                            || crate::canvas::comments::channel(&format!("F{}", &value[1..]))
                                .is_err()
                    }
                    "thread_ts" => !crate::canvas::comments::timestamp(value),
                    "client_msg_id" => uuid::Uuid::parse_str(value).is_err(),
                    "blocks" => {
                        value.len() > crate::canvas::comments::content::MAX_BLOCK_BYTES
                            || serde_json::from_str::<Vec<serde_json::Value>>(value)
                                .ok()
                                .and_then(|blocks| {
                                    crate::canvas::comments::content::fallback(&blocks).ok()
                                })
                                .is_none_or(|text| {
                                    fields
                                        .iter()
                                        .find(|(k, _)| k == "text")
                                        .is_none_or(|(_, v)| v != &text)
                                })
                    }
                    "reply_broadcast" => value != "false",
                    "text" => {
                        value.trim().is_empty()
                            || !crate::canvas::comments::reply::valid_text(value)
                    }
                    _ => true,
                }
        })
    {
        return false;
    }
    let Ok(mut url) = url::Url::parse(&req.url) else {
        return false;
    };
    if ![
        "token",
        "channel",
        "thread_ts",
        "client_msg_id",
        "reply_broadcast",
        "text",
    ]
    .iter()
    .all(|key| seen.contains(key))
        || url.path() != "/api/chat.postMessage"
    {
        return false;
    }
    url.set_path("/canvas/-/edit-document");
    let mut check = req.clone();
    check.url = url.into();
    allowed(&check)
}

fn comment_allowed(req: &PreparedRequest) -> bool {
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let mut seen = std::collections::HashSet::new();
    if fields.len() != 3
        || fields.iter().any(|(key, value)| {
            !seen.insert(key.as_str())
                || match key.as_str() {
                    "token" => value.is_empty(),
                    "channel_id" => crate::canvas::comments::channel(&format!(
                        "F{}",
                        value.strip_prefix('C').unwrap_or("")
                    ))
                    .is_err(),
                    "thread_ts" => !crate::canvas::comments::timestamp(value),
                    _ => true,
                }
        })
    {
        return false;
    }
    let mut test = req.clone();
    let Ok(mut url) = url::Url::parse(&req.url) else {
        return false;
    };
    if !matches!(
        url.path(),
        "/api/quip.thread.archive" | "/api/quip.thread.unarchive"
    ) {
        return false;
    }
    url.set_path("/canvas/-/edit-document");
    test.url = url.into();
    allowed(&test)
}

fn allowed(req: &PreparedRequest) -> bool {
    req.method == "POST"
        && matches!(req.body, RequestBody::Form(_))
        && url::Url::parse(&req.url).is_ok_and(|url| {
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
                && url.path() == "/canvas/-/edit-document"
        })
}

#[cfg(test)]
mod tests;
