//! Bounded notification reads and writes: no offline hold or automatic replay.
use super::*;

impl Transport {
    pub async fn execute_notification_preferences(
        &self,
        req: PreparedRequest,
    ) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api(
                "invalid_notification_preferences_request".into(),
            ));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Notification preferences request not sent while offline".into(),
            ));
        }
        let http = self.single_attempt_client()?;
        let response =
            self.execute_once_using(&req, true, http)
                .await
                .map_err(|error| match error {
                    Error::Offline(_) => Error::Transport(
                        "Notification preferences response lost after dispatch".into(),
                    ),
                    error => error,
                })?;
        notification_preferences_success(&response)?;
        Ok(response)
    }
}
pub(in crate::slack) fn notification_preferences_success(value: &Value) -> Result<(), Error> {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        if let Some(code) = value
            .get("error")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            return Err(Error::Api(code.into()));
        }
    }
    if value.get("ok").and_then(Value::as_bool) != Some(true)
        || value.get("error").is_some_and(|e| !e.is_null())
    {
        return Err(Error::Transport(
            "Slack did not confirm the notification preference request".into(),
        ));
    }
    Ok(())
}
fn allowed(req: &PreparedRequest) -> bool {
    req.method == "POST"
        && matches!(req.body, RequestBody::Form(_))
        && url::Url::parse(&req.url).is_ok_and(|url| {
            let origin = url.scheme() == "https"
                && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
                && url.port_or_known_default() == Some(443);
            #[cfg(test)]
            let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
            origin
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && matches!(
                    url.path(),
                    "/api/users.prefs.setNotifications" | "/api/users.prefs.get"
                )
        })
}
#[cfg(test)]
mod tests;
