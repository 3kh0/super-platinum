//! Single dispatch only. A successful HTTP body is evidence to retain, not a
//! canonical schema acknowledgement. Callers require an independent readback.
use super::*;

#[derive(Debug)]
pub enum ListColumnRenameFailure {
    NotRequested(Error),
    Unconfirmed(Error),
}
impl Transport {
    pub async fn execute_list_column_rename(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnRenameFailure> {
        self.execute_list_rename_once(request, current, allowed)
            .await
    }
    // Shared mechanics only. Each public wrapper supplies its exact private validator.
    pub(super) async fn execute_list_rename_once(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
        permitted: fn(&PreparedRequest) -> bool,
    ) -> Result<Value, ListColumnRenameFailure> {
        self.execute_list_rename_once_with(request, current, permitted, |_, value| Ok(value))
            .await
    }
    // The decoder sees the same original bounded bytes only after the shared
    // HTTP, Retry-After, duplicate-key, object and rate-limit checks. It performs
    // no additional request and cannot widen a public wrapper's validator.
    pub(super) async fn execute_list_rename_once_with<T>(
        &self,
        request: PreparedRequest,
        mut current: impl FnMut() -> bool,
        permitted: fn(&PreparedRequest) -> bool,
        decode: impl FnOnce(&[u8], Value) -> Result<T, Error>,
    ) -> Result<T, ListColumnRenameFailure> {
        use ListColumnRenameFailure::{NotRequested, Unconfirmed};
        if !permitted(&request) {
            return Err(NotRequested(Error::Api(
                "unsupported_list_column_rename".into(),
            )));
        }
        if self.health() == Health::Offline {
            return Err(NotRequested(Error::Offline(
                "List rename not sent while offline".into(),
            )));
        }
        if !current() {
            return Err(NotRequested(Error::Transport(
                "List rename was not requested".into(),
            )));
        }
        let http = self.single_attempt_client().map_err(NotRequested)?;
        let mut builder = http
            .post(&request.url)
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none());
        for (name, value) in &request.headers {
            if !name.eq_ignore_ascii_case("content-type") {
                builder = builder.header(name.as_str(), value.as_str());
            }
        }
        let builder = builder.header("Cookie", self.cookie());
        // Public wrappers still supply their exact private validators. The
        // channel-scoped permission read is JSON; existing writes remain forms.
        let builder = match &request.body {
            RequestBody::Form(fields) => builder.form(fields),
            RequestBody::Json(value) => builder.json(value),
        };
        let request = builder
            .build()
            .map_err(|_| NotRequested(Error::Api("invalid_list_column_rename".into())))?;
        // No await separates this exact original-owner fence from dispatch.
        if self.health() == Health::Offline {
            return Err(NotRequested(Error::Offline(
                "List rename not sent while offline".into(),
            )));
        }
        if !current() {
            return Err(NotRequested(Error::Transport(
                "List rename was not requested".into(),
            )));
        }
        let outcome = tokio::time::timeout(API_TIMEOUT, async {
            let response = http.execute(request).await.map_err(|error| {
                if send_error(error).is_offline() {
                    self.set_health(Health::Offline);
                }
                Error::Transport("List rename response was lost after dispatch".into())
            })?;
            self.set_health(Health::Online);
            let status = response.status().as_u16();
            let retry_after_secs = retry_after_secs(response.headers());
            if status == 429 {
                return Err(Error::RateLimited { retry_after_secs });
            }
            // Any Retry-After must reach the stable-owner cooldown ledger even
            // with 2xx, before account/visit projection eligibility is checked.
            if !(200..300).contains(&status) || retry_after_secs.is_some() {
                return Err(Error::HttpStatus {
                    status,
                    retry_after_secs,
                });
            }
            const LIMIT: usize = 1024 * 1024;
            if response
                .content_length()
                .is_some_and(|len| len > LIMIT as u64)
            {
                return Err(Error::Transport("List rename response too large".into()));
            }
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk
                    .map_err(|_| Error::Transport("List rename body was interrupted".into()))?;
                if chunk.len() > LIMIT - bytes.len() {
                    return Err(Error::Transport("List rename response too large".into()));
                }
                bytes.extend_from_slice(&chunk);
            }
            // Existing public pure decoder rejects duplicate keys recursively.
            let value = crate::slash_commands::codec::decode(&bytes, LIMIT)
                .map_err(|_| Error::Transport("Invalid List rename response".into()))?;
            if !value.is_object() {
                return Err(Error::Transport("Invalid List rename response".into()));
            }
            if value["ok"] == false && value["error"] == "ratelimited" {
                return Err(Error::RateLimited {
                    retry_after_secs: None,
                });
            }
            decode(&bytes, value)
        })
        .await
        .map_err(|_| Unconfirmed(Error::Transport("List rename response timed out".into())))?;
        // Preserve the original response after retirement, for receipt only.
        outcome.map_err(Unconfirmed)
    }
}
fn allowed(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != "/api/lists.columns.update"
        || url.query_pairs().any(|(key, _)| {
            !matches!(
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
        })
    {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    let expected = [
        "token",
        "list_id",
        "key",
        "name",
        "options",
        "backfill_missing_cells_with_default",
        "_x_reason",
    ];
    if fields.len() != expected.len()
        || fields
            .iter()
            .zip(expected)
            .any(|((key, _), want)| key != want)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 16 * 1024)
    {
        return false;
    }
    let value = |i: usize| fields[i].1.as_str();
    !value(0).is_empty()
        && !value(1).trim().is_empty()
        && !value(2).trim().is_empty()
        && !value(3).is_empty()
        && value(3).trim() == value(3)
        && value(3).encode_utf16().count() <= 256
        && !value(3).chars().any(char::is_control)
        && matches!(
            value(4),
            "{\"format\":\"text\"}"
                | "{\"precision\":0}"
                | "{\"precision\":1}"
                | "{\"precision\":2}"
                | "{\"precision\":3}"
                | "{\"precision\":4}"
                | "{\"precision\":5}"
        )
        && value(5) == "true"
        && value(6) == "update-column"
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod number_tests;
