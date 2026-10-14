//! Single-attempt explicit bulk read/Undo and independent confirmation prefs.
//! No retry, redirect following, or offline queue may replay a shortcut.
use super::*;

impl Transport {
    pub async fn execute_read_shortcuts(&self, request: PreparedRequest) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api("invalid_read_shortcuts_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Read shortcut not sent while offline".into(),
            ));
        }
        let response = self
            .execute_once_using(&request, true, self.single_attempt_client()?)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => {
                    Error::Transport("Read shortcut outcome lost after dispatch".into())
                }
                error => error,
            })?;
        match url::Url::parse(&request.url)
            .ok()
            .as_ref()
            .map(url::Url::path)
        {
            Some("/api/users.prefs.get") => notification_preferences_success(&response)?,
            Some(
                "/api/users.prefs.set"
                | "/api/conversations.mark"
                | "/api/subscriptions.thread.mark",
            ) => {
                if response.get("undo_ts").is_some() {
                    return Err(Error::Transport(
                        "Unexpected bulk receipt for a targeted read shortcut".into(),
                    ));
                }
                crate::slack::api::read_shortcuts::bulk_read_success(&response)?;
            }
            _ => crate::slack::api::read_shortcuts::bulk_read_success(&response)?,
        }
        Ok(response)
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
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
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
    let mut seen = std::collections::BTreeSet::new();
    if fields.iter().any(|(name, _)| !seen.insert(name)) {
        return false;
    }
    let field = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    if field("token").is_none_or(str::is_empty) {
        return false;
    }
    match url.path() {
        "/api/conversations.mark" => {
            fields.len() == 3
                && field("channel").is_some_and(valid_channel)
                && field("ts").is_some_and(valid_undo)
        }
        "/api/subscriptions.thread.mark" => {
            fields.len() == 5
                && field("channel").is_some_and(valid_channel)
                && field("thread_ts").is_some_and(valid_undo)
                && field("ts").is_some_and(valid_undo)
                && field("read") == Some("1")
        }
        "/api/users.markAllRead" => {
            (fields.len() == 2 && field("_x_reason") == Some("ShiftEscapeConfirmDialog"))
                || (fields.len() == 3
                    && field("_x_reason") == Some("UndoShiftEscapeMarkAllRead")
                    && field("undo_ts").is_some_and(valid_undo))
        }
        "/api/users.prefs.get" => {
            fields.len() == 2 && field("prefs") == Some("confirm_clear_all_unreads")
        }
        "/api/users.prefs.set" => {
            fields.len() == 4
                && field("name") == Some("confirm_clear_all_unreads")
                && field("value") == Some("false")
                && field("_x_reason") == Some("prefs-api/setUserPrefByApi")
        }
        _ => false,
    }
}

fn valid_channel(id: &str) -> bool {
    id.len() > 1
        && matches!(id.as_bytes()[0], b'C' | b'D' | b'G')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn valid_undo(ts: &str) -> bool {
    let Some((seconds, fraction)) = ts.split_once('.') else {
        return false;
    };
    !seconds.is_empty()
        && seconds.bytes().all(|b| b.is_ascii_digit())
        && seconds.parse::<u64>().is_ok()
        && !fraction.is_empty()
        && fraction.len() <= 6
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests;
