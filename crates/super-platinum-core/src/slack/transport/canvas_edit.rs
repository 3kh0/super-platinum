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
    pub async fn execute_canvas_edit(
        &self,
        req: PreparedRequest,
    ) -> Result<Vec<u8>, CanvasEditFailure> {
        use CanvasEditFailure::{NotSent, Uncertain};
        if !allowed(&req) {
            return Err(NotSent(Error::Api(
                "unsupported_canvas_edit_request".into(),
            )));
        }
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
