//! Bounded, single-attempt read-preference requests. This is not a generic
//! users.prefs.set escape hatch: both boolean fields and no others are required.
use super::*;

impl Transport {
    pub async fn execute_read_preferences(&self, req: PreparedRequest) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_read_preferences_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Read preferences request not sent while offline".into(),
            ));
        }
        let response = self
            .execute_once_using(&req, true, self.single_attempt_client()?)
            .await
            .map_err(|error| match error {
                Error::Offline(_) => {
                    Error::Transport("Read preferences response lost after dispatch".into())
                }
                error => error,
            })?;
        if url::Url::parse(&req.url).is_ok_and(|url| url.path() == "/api/users.prefs.set") {
            write_success(&response)?;
        } else {
            notification_preferences_success(&response)?;
        }
        Ok(response)
    }
}

/// Retain uncertainty when an error is accompanied by possible mutation output.
/// The generic preference reader intentionally loses raw envelope fields when
/// returning Api(code); that is insufficient authority to release a write lock.
fn write_success(response: &Value) -> Result<(), Error> {
    if response.get("ok") == Some(&Value::Bool(true))
        && response.get("error").is_none_or(Value::is_null)
    {
        return Ok(());
    }
    if let Some(code) = definite_rejection(response) {
        return Err(Error::Api(code.to_owned()));
    }
    Err(Error::Transport(
        "Slack did not unambiguously confirm or reject the read preference update".into(),
    ))
}

fn definite_rejection(response: &Value) -> Option<&str> {
    let object = response.as_object()?;
    if object.get("ok") != Some(&Value::Bool(false))
        || object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "ok" | "error" | "needed" | "provided" | "response_metadata"
            )
        })
        || ["needed", "provided"]
            .iter()
            .any(|key| object.get(*key).is_some_and(|value| !value.is_string()))
        || object.get("response_metadata").is_some_and(|value| {
            !value.as_object().is_some_and(|metadata| {
                metadata.iter().all(|(key, value)| {
                    matches!(key.as_str(), "warnings" | "messages")
                        && value
                            .as_array()
                            .is_some_and(|items| items.iter().all(Value::is_string))
                })
            })
        })
    {
        return None;
    }
    let code = object.get("error")?.as_str()?;
    matches!(
        code,
        "invalid_auth"
            | "not_authed"
            | "token_revoked"
            | "token_expired"
            | "account_inactive"
            | "missing_scope"
            | "invalid_arguments"
            | "invalid_arg_name"
    )
    .then_some(code)
}

fn allowed(req: &PreparedRequest) -> bool {
    if req.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&req.url) else {
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
        "/api/users.prefs.get" => {
            fields.len() == 2
                && field("prefs") == Some("mark_msgs_read_immediately,start_scroll_at_oldest")
        }
        "/api/users.prefs.set" => {
            fields.len() == 3
                && field("_x_reason") == Some("prefs-api/setMultipleUserPrefsByApi")
                && field("prefs").is_some_and(|text| {
                    serde_json::from_str::<Value>(text).is_ok_and(|value| {
                        value.as_object().is_some_and(|patch| {
                            patch.len() == 2
                                && crate::state::READ_PREFERENCE_NAMES
                                    .iter()
                                    .all(|key| patch.get(*key).is_some_and(Value::is_boolean))
                        })
                    })
                })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
