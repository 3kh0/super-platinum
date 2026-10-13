//! Read-only Canvas collaboration transport. No mutation path is allowlisted.
use super::*;

impl Transport {
    /// Bounded, single-attempt reads for controller initialization and canonical
    /// collaboration data. Never queue a visit-owned read across an outage.
    pub async fn execute_canvas_collab_read(&self, req: PreparedRequest) -> Result<Vec<u8>, Error> {
        if !allowed(&req) {
            return Err(Error::Api("unsupported_canvas_collab_read".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline("canvas read not sent while offline".into()));
        }
        let RequestBody::Form(fields) = &req.body else {
            return Err(Error::Api("unsupported_canvas_collab_read".into()));
        };
        if fields
            .iter()
            .try_fold(0usize, |sum, (key, value)| {
                sum.checked_add(key.len())?.checked_add(value.len())
            })
            .is_none_or(|length| length > 128 * 1024)
        {
            return Err(Error::Api("canvas_read_request_too_large".into()));
        }
        let mut builder = self
            .http
            .post(&req.url)
            .timeout(API_TIMEOUT)
            // A form token must never follow a cross-origin redirect, including
            // 307/308 redirects that replay the original POST body.
            .redirect(wreq::redirect::Policy::none());
        for (key, value) in &req.headers {
            if !key.eq_ignore_ascii_case("content-type") {
                builder = builder.header(key.as_str(), value.as_str());
            }
        }
        let response = builder
            .header("Cookie", self.cookie())
            .form(fields)
            .send()
            .await
            .map_err(|error| self.canvas_read_error(error))?;
        self.set_health(Health::Online);
        let status = response.status();
        let retry_after_secs = retry_after_secs(response.headers());
        if status.as_u16() == 429 {
            return Err(Error::RateLimited { retry_after_secs });
        }
        if !status.is_success() {
            return Err(Error::HttpStatus {
                status: status.as_u16(),
                retry_after_secs,
            });
        }
        let limit = crate::canvas::wire::MAX_BYTES;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(Error::Transport(
                "canvas response exceeds size limit".into(),
            ));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| self.canvas_read_error(error))?;
            if chunk.len() > limit - bytes.len() {
                return Err(Error::Transport(
                    "canvas response exceeds size limit".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn canvas_read_error(&self, error: wreq::Error) -> Error {
        if send_error(error).is_offline() {
            self.set_health(Health::Offline);
            Error::Offline("canvas read connection failed".into())
        } else {
            Error::Transport("canvas read failed".into())
        }
    }
}

fn allowed(req: &PreparedRequest) -> bool {
    if req.method != "POST" || !matches!(req.body, RequestBody::Form(_)) {
        return false;
    }
    url::Url::parse(&req.url).is_ok_and(|url| {
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
            && matches!(
                url.path(),
                "/api/quip.getControllerInitData"
                    | "/canvas/-/load-data"
                    | "/canvas/-/fetch-latest"
            )
    })
}

#[cfg(test)]
mod tests;
