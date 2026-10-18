//! Original autocomplete offline-feature request with same-byte priority order.
//! The caller owns account/cache publication; this receipt grants no UI action.
use super::*;
#[cfg(test)]
mod tests;

impl Transport {
    pub async fn execute_autocomplete_offline_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<AutocompleteOfflineResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_autocomplete_offline_request".into()));
        }
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Autocomplete offline owner retired".into())
                }));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure.unwrap_or_else(|| {
                    Error::Offline("Autocomplete offline owner retired".into())
                }));
            }
            link?;
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_autocomplete_offline_once(&req) => result,
                    () = self.link_dropped() => Err(Error::Offline("link dropped mid-call".into())),
                }
            } else {
                self.execute_autocomplete_offline_once(&req).await
            };
            match attempted {
                Ok(value) => return Ok(value),
                Err(e) if req.retry_safe() && retryable_error(&e) && attempt < 2 => {
                    attempt += 1;
                    last_failure = Some(e.clone());
                    tokio::time::sleep(retry_delay(&e, attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    async fn execute_autocomplete_offline_once(
        &self,
        req: &PreparedRequest,
    ) -> Result<AutocompleteOfflineResponse, Error> {
        let mut keys = None;
        let raw = self
            .execute_once_using_field_decode_mode(
                req,
                false,
                self.http()?,
                false,
                Some(&mut keys),
                None,
                None,
                "channel_priority_90d",
            )
            .await?;
        Ok(AutocompleteOfflineResponse { raw, keys })
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
        || url.path() != "/api/search.autocomplete.offlineFeatures"
    {
        return false;
    }
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    fields.len() == 2
        && fields
            .iter()
            .filter(|(k, v)| k == "token" && !v.is_empty())
            .count()
            == 1
        && fields
            .iter()
            .filter(|(k, v)| k == "_x_reason" && v == "search-offline-features")
            .count()
            == 1
}
