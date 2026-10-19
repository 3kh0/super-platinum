//! Closed Profile search.autocomplete.files read. Original UTF8 body bytes
//! supply nested channel_map order; this is not canonical publication authority.
use super::*;
#[cfg(test)]
mod tests;

impl Transport {
    pub async fn execute_profile_file_search(
        &self,
        req: PreparedRequest,
    ) -> Result<ProfileFileSearchResponse, Error> {
        self.execute_profile_file_search_current(req, || true).await
    }
    /// Preserve the existing link hold, retry classification and issued errors.
    /// The caller retains original account/query/source authority after return.
    pub async fn execute_profile_file_search_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<ProfileFileSearchResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_profile_file_search_request".into()));
        }
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Profile file search owner retired".into())
                }));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Profile file search owner retired".into())
                }));
            }
            link?;
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_profile_file_search_once(&req) => result,
                    () = self.link_dropped() => Err(Error::Offline("link dropped mid-call".into())),
                }
            } else {
                self.execute_profile_file_search_once(&req).await
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
    async fn execute_profile_file_search_once(
        &self,
        req: &PreparedRequest,
    ) -> Result<ProfileFileSearchResponse, Error> {
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
        // existing files::response semantic refusal instead of recategorizing it.
        let response = ProfileFileSearchResponse::decode(bytes.as_bytes())?;
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
        || url.path() != "/api/search.autocomplete.files"
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
    map.len() == 4
        && fields.len() == 4
        && map.get("token").is_some_and(|token| !token.is_empty())
        && map.contains_key("query")
        && map.get("include_shares") == Some(&"true")
        && map.get("_x_reason") == Some(&"texty-autocomplete")
}
