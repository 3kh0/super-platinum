//! Exact record delete / authoritative record-info paths. Both send once with
//! no offline hold; only pre-dispatch failures prove a delete was not sent.
use super::*;

#[derive(Debug)]
pub enum ListDeleteFailure {
    NotSent(Error),
    Uncertain(Error),
}

impl Transport {
    pub async fn execute_list_record_delete(
        &self,
        req: PreparedRequest,
    ) -> Result<Value, ListDeleteFailure> {
        let value = self
            .execute_list_record_request(req, "/api/lists.records.delete")
            .await?;
        if value["ok"] != true || value.get("error").is_some_and(|error| !error.is_null()) {
            return Err(ListDeleteFailure::Uncertain(Error::Api(
                "List deletion was not confirmed".into(),
            )));
        }
        Ok(value)
    }

    pub async fn execute_list_record_update_deletion(
        &self,
        req: PreparedRequest,
    ) -> Result<Value, ListDeleteFailure> {
        let value = self
            .execute_list_record_request(req, "/api/lists.records.updateDeletion")
            .await?;
        if value["ok"] != true || value.get("error").is_some_and(|error| !error.is_null()) {
            return Err(ListDeleteFailure::Uncertain(Error::Api(
                "List deletion-state change was not confirmed".into(),
            )));
        }
        Ok(value)
    }

    /// Preserve the raw API error envelope: only record_deleted is authoritative,
    /// and its optional identity fields must be checked before reducing it.
    pub async fn execute_list_record_info(&self, req: PreparedRequest) -> Result<Value, Error> {
        self.execute_list_record_request(req, "/api/lists.records.info")
            .await
            .map_err(|failure| match failure {
                ListDeleteFailure::NotSent(error) | ListDeleteFailure::Uncertain(error) => error,
            })
    }

    async fn execute_list_record_request(
        &self,
        req: PreparedRequest,
        path: &str,
    ) -> Result<Value, ListDeleteFailure> {
        use ListDeleteFailure::{NotSent, Uncertain};
        if !allowed(&req, path) {
            return Err(NotSent(Error::Api(
                "unsupported_list_record_request".into(),
            )));
        }
        if self.health() == Health::Offline {
            return Err(NotSent(Error::Offline(
                "List record request not sent while offline".into(),
            )));
        }
        let RequestBody::Form(fields) = &req.body else {
            unreachable!("validated form")
        };
        if fields
            .iter()
            .try_fold(0usize, |sum, (key, value)| {
                sum.checked_add(key.len())?.checked_add(value.len())
            })
            .is_none_or(|length| length > 16 * 1024)
        {
            return Err(NotSent(Error::Api("list_record_request_too_large".into())));
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
        let request = builder
            .header("Cookie", self.cookie())
            .form(fields)
            .build()
            .map_err(|_| NotSent(Error::Api("invalid_list_record_request".into())))?;
        if self.health() == Health::Offline {
            return Err(NotSent(Error::Offline(
                "List record request not sent while offline".into(),
            )));
        }
        let response = http
            .execute(request)
            .await
            .map_err(|error| Uncertain(self.list_record_error(error)))?;
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
        const LIMIT: usize = 8 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > LIMIT as u64)
        {
            return Err(Uncertain(Error::Transport(
                "List record response exceeds size limit".into(),
            )));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| Uncertain(self.list_record_error(error)))?;
            if chunk.len() > LIMIT - bytes.len() {
                return Err(Uncertain(Error::Transport(
                    "List record response exceeds size limit".into(),
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Uncertain(Error::Transport("Invalid List record response".into())))?;
        if !value.is_object() {
            return Err(Uncertain(Error::Transport(
                "Invalid List record response object".into(),
            )));
        }
        if value["ok"] == false && value["error"] == "ratelimited" {
            return Err(Uncertain(Error::RateLimited { retry_after_secs }));
        }
        Ok(value)
    }

    fn list_record_error(&self, error: wreq::Error) -> Error {
        if send_error(error).is_offline() {
            self.set_health(Health::Offline);
        }
        Error::Transport("List record response was lost after dispatch".into())
    }
}

fn allowed(req: &PreparedRequest, path: &str) -> bool {
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
                && url.path() == path
        })
}

#[cfg(test)]
mod tests;
