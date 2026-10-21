//! Closed files.info singleton read. Original UTF8 body bytes
//! supply nested channel_map order; this is not canonical publication authority.
use super::*;
mod metadata;
#[cfg(test)]
mod tests;

impl Transport {
    /// Preserve the existing link hold, retry classification and issued errors.
    /// The caller retains original account/query/source authority after return.
    pub async fn execute_file_info_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<OrderedFileInfo, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_file_info_request".into()));
        }
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure
                    .unwrap_or_else(|| Error::Offline("File info owner retired".into())));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure
                    .unwrap_or_else(|| Error::Offline("File info owner retired".into())));
            }
            link?;
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_file_info_once(&req) => result,
                    () = self.link_dropped() => Err(Error::Offline("link dropped mid-call".into())),
                }
            } else {
                self.execute_file_info_once(&req).await
            };
            match attempted {
                Ok(value) => return Ok(value),
                Err(error) if req.retry_safe() && retryable_error(&error) && attempt < 2 => {
                    attempt += 1;
                    last_failure = Some(error.clone());
                    tokio::time::sleep(retry_delay(&error, attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }
    async fn execute_file_info_once(
        &self,
        req: &PreparedRequest,
    ) -> Result<OrderedFileInfo, Error> {
        let mut bytes = None;
        let raw = self
            .execute_once_using_observed_decode_mode(
                req,
                false,
                self.http()?,
                false,
                None,
                None,
                Some(&mut bytes),
            )
            .await?;
        let bytes =
            bytes.ok_or_else(|| Error::Transport("missing original file response bytes".into()))?;
        // JSON/UTF8 errors already came from the shared reader. Preserve the
        // singleton envelope semantic refusal instead of recategorizing it.
        let response = OrderedFileInfo::decode(bytes.as_bytes())?;
        if response.raw() != &raw {
            return Err(Error::Transport(
                "file response observation mismatch".into(),
            ));
        }
        Ok(response)
    }
}

fn allowed(req: &PreparedRequest) -> bool {
    if req.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&req.url) else {
        return false;
    };
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != "/api/files.info"
    {
        return false;
    }
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let map: std::collections::BTreeMap<_, _> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    map.len() == 5
        && fields.len() == 5
        && map.get("token").is_some_and(|token| !token.is_empty())
        && map.get("file").is_some_and(|id| {
            (2..=255).contains(&id.len())
                && id.starts_with('F')
                && id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        && map.get("page") == Some(&"1")
        && map.get("count") == Some(&"500")
        && map.get("truncate") == Some(&"true")
}
