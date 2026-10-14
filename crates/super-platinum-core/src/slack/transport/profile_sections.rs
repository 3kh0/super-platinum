//! Exact structured profile mutation allowlist; no offline holding or replay.
use super::*;
impl Transport {
    pub async fn execute_own_profile_sections(
        &self,
        request: PreparedRequest,
    ) -> Result<(), Error> {
        if !allowed(&request) {
            return Err(Error::Api("invalid_own_profile_sections_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Profile title was not sent while offline".into(),
            ));
        }
        let response = self
            .execute_once_using(&request, true, self.single_attempt_client()?)
            .await
            .map_err(|e| match e {
                Error::Offline(_) => {
                    Error::Transport("Profile title response lost after dispatch".into())
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
                "Slack did not confirm the profile title edit".into(),
            ));
        }
        Ok(())
    }
}
fn allowed(req: &PreparedRequest) -> bool {
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let keys = ["token", "user", "section", "elements", "_x_reason"];
    if fields.len() != keys.len()
        || keys
            .iter()
            .any(|k| fields.iter().filter(|(f, _)| f == k).count() != 1)
    {
        return false;
    }
    let field = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.as_str());
    if field("_x_reason") != Some("edit-profile-form-pane")
        || field("user").is_none_or(str::is_empty)
        || field("section").is_none_or(str::is_empty)
        || !field("elements")
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .is_some_and(|v| {
                v.as_array().is_some_and(|a| {
                    !a.is_empty()
                        && a.iter().all(|e| {
                            e.as_object().is_some_and(|o| {
                                o.len() == 2
                                    && e["element_id"].as_str().is_some_and(|s| !s.is_empty())
                                    && o.keys().all(|k| {
                                        matches!(
                                            k.as_str(),
                                            "element_id"
                                                | "text"
                                                | "currentLocation"
                                                | "tags"
                                                | "date"
                                                | "link"
                                                | "person"
                                                | "long_text"
                                                | "file"
                                        )
                                    })
                            })
                        })
                })
            })
    {
        return false;
    }
    req.method == "POST"
        && url::Url::parse(&req.url).is_ok_and(|u| {
            let origin = u.scheme() == "https"
                && u.host_str().is_some_and(|h| h.ends_with(".slack.com"))
                && u.port_or_known_default() == Some(443);
            #[cfg(test)]
            let origin = origin || (u.scheme() == "http" && u.host_str() == Some("127.0.0.1"));
            origin
                && u.username().is_empty()
                && u.password().is_none()
                && u.fragment().is_none()
                && u.query_pairs().all(|(key, _)| {
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
                && u.path() == "/api/users.profile.setSections"
        })
}
#[cfg(test)]
mod tests;
