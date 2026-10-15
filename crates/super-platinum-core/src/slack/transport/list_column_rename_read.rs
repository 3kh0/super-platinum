//! Rename-only positive metadata/policy reads. No offline hold or implicit retry.
use super::*;

#[derive(Debug)]
pub enum ListColumnRenameReadFailure {
    NotRequested(Error),
    Unconfirmed(Error),
}
impl Transport {
    pub async fn execute_list_column_rename_read(
        &self,
        request: PreparedRequest,
        mut current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnRenameReadFailure> {
        use ListColumnRenameReadFailure::{NotRequested, Unconfirmed};
        if !allowed(&request) {
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
        let RequestBody::Form(fields) = &request.body else {
            unreachable!()
        };
        let mut builder = http
            .post(&request.url)
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none());
        for (name, value) in &request.headers {
            if !name.eq_ignore_ascii_case("content-type") {
                builder = builder.header(name.as_str(), value.as_str());
            }
        }
        let request = builder
            .header("Cookie", self.cookie())
            .form(fields)
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
            Ok(value)
        })
        .await
        .map_err(|_| Unconfirmed(Error::Transport("List rename response timed out".into())))?;
        // Preserve the original response after retirement, for receipt only.
        outcome.map_err(Unconfirmed)
    }
}
pub(super) fn allowed(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some()
    {
        return false;
    }
    if url.query_pairs().any(|(k, _)| {
        !matches!(
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
    }) {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    if fields
        .iter()
        .map(|(k, v)| k.len().saturating_add(v.len()))
        .try_fold(0usize, usize::checked_add)
        .is_none_or(|n| n > 16384)
    {
        return false;
    }
    let keys: Vec<_> = fields.iter().map(|(k, _)| k.as_str()).collect();
    let val = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    if val("token").is_none_or(str::is_empty) {
        return false;
    }
    match url.path() {
        "/api/files.info" => {
            keys == ["token", "file"] && val("file").is_some_and(|v| !v.trim().is_empty())
        }
        "/api/team.prefs.get" => {
            keys == ["token", "target_team", "_x_reason"]
                && val("target_team").is_some_and(|v| !v.trim().is_empty())
                && val("_x_reason") == Some("native-list-permission")
        }
        "/api/enterprise.prefs.get" => {
            keys == ["token", "_x_reason"] && val("_x_reason") == Some("native-list-permission")
        }
        "/api/users.prefs.get" => {
            keys == ["token", "prefs", "_x_reason"]
                && val("prefs") == Some("task_lists,assigned_to_list")
                && val("_x_reason") == Some("native-list-completion-permission")
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests;
