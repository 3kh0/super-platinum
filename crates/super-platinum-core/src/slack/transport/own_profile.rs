//! One-shot ordinary self-profile writes. Never follows redirects or retries.
use super::*;

impl Transport {
    pub async fn execute_own_profile_text(&self, request: PreparedRequest) -> Result<(), Error> {
        if !allowed(&request) {
            return Err(Error::Api("invalid_own_profile_text_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Profile edit was not sent while offline".into(),
            ));
        }
        let response = self
            .execute_once_using(&request, true, self.single_attempt_client()?)
            .await
            .map_err(|e| match e {
                Error::Offline(_) => {
                    Error::Transport("Profile response lost after dispatch".into())
                }
                e => e,
            })?;
        if response["ok"] == false
            && let Some(code) = response["error"].as_str().filter(|s| !s.is_empty())
        {
            return Err(Error::Api(code.into()));
        }
        if response["ok"] != true || response.get("error").is_some_and(|e| !e.is_null()) {
            return Err(Error::Transport(
                "Slack did not confirm the profile edit".into(),
            ));
        }
        Ok(())
    }
}
fn allowed(req: &PreparedRequest) -> bool {
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let expected = ["token", "profile", "reason"];
    if fields.len() != expected.len()
        || expected
            .iter()
            .any(|key| fields.iter().filter(|(k, _)| k == key).count() != 1)
    {
        return false;
    }
    let field = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    if field("reason") != Some("EditProfileFormPane") {
        return false;
    }
    let profile = field("profile").and_then(|s| serde_json::from_str::<Value>(s).ok());
    if !profile
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|p| {
            !p.is_empty()
                && p.iter().all(|(k, v)| {
                    matches!(k.as_str(), "real_name" | "display_name" | "pronouns") && v.is_string()
                })
        })
    {
        return false;
    }
    req.method == "POST"
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
                && url.query_pairs().all(|(key, _)| {
                    matches!(
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
                && url.path() == "/api/users.profile.set"
        })
}

#[cfg(test)]
mod tests;
