//! Single-attempt, no-offline-hold boundary for the four emoji preference keys.
//! Full frecency parsing stays in the worker-owned request builder.
use super::*;

impl Transport {
    pub async fn execute_emoji_preferences(&self, req: PreparedRequest) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_emoji_preferences_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Emoji preferences request not sent while offline".into(),
            ));
        }
        self.execute_once_using(&req, true, self.single_attempt_client()?)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => {
                    Error::Transport("Emoji preferences response lost after dispatch".into())
                }
                other => other,
            })
    }
}
fn allowed(req: &PreparedRequest) -> bool {
    if req.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&req.url) else {
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
        || url.query_pairs().any(|(name, _)| {
            !matches!(
                name.as_ref(),
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
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let mut keys = std::collections::BTreeSet::new();
    if fields.iter().any(|(name, _)| !keys.insert(name)) {
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
        "/api/users.prefs.get" => {
            fields.len() == 2
                && field("prefs") == Some("preferred_skin_tone,emoji_mode,emoji_use,frecency")
        }
        "/api/users.prefs.set" => {
            fields.len() == 3
                && field("name") == Some("frecency")
                && field("value").is_some_and(|v| v.starts_with('{') && v.ends_with('}'))
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests;
