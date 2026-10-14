//! Narrow recurring-schedule pref reads/writes; no generic users.prefs.set access.
use super::*;
impl Transport {
    pub async fn execute_notification_schedule(
        &self,
        req: PreparedRequest,
    ) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_notification_schedule_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Notification schedule request not sent while offline".into(),
            ));
        }
        let result = self
            .execute_once_using(&req, true, self.single_attempt_client()?)
            .await
            .map_err(|e| match e {
                Error::Offline(_) => {
                    Error::Transport("Notification schedule response lost after dispatch".into())
                }
                e => e,
            })?;
        notification_preferences_success(&result)?;
        Ok(result)
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
        || url.query_pairs().any(|(k, _)| {
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
        })
    {
        return false;
    }
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let mut seen = std::collections::BTreeSet::new();
    if fields
        .iter()
        .any(|(k, _)| !matches!(k.as_str(), "token" | "prefs" | "_x_reason") || !seen.insert(k))
    {
        return false;
    }
    let Some((_, prefs)) = fields.iter().find(|(k, _)| k == "prefs") else {
        return false;
    };
    match url.path() {
        "/api/users.prefs.get" => {
            !prefs.is_empty()
                && prefs
                    .split(',')
                    .all(super::super::api::is_notification_schedule_pref)
        }
        "/api/users.prefs.set" => {
            if fields
                .iter()
                .find(|(k, _)| k == "_x_reason")
                .map(|(_, v)| v.as_str())
                != Some("prefs-api/setMultipleUserPrefsByApi")
            {
                return false;
            }
            serde_json::from_str::<Value>(prefs).is_ok_and(|v| valid_patch(&v))
        }
        _ => false,
    }
}
fn valid_patch(value: &Value) -> bool {
    let Some(patch) = value.as_object() else {
        return false;
    };
    patch.get("dnd_enabled") == Some(&Value::Bool(true))
        && patch.keys().any(|k| k != "dnd_enabled")
        && patch.iter().all(|(k, v)| {
            if k == "dnd_enabled" {
                return v == true;
            }
            if k == "dnd_days" {
                return v
                    .as_str()
                    .is_some_and(|s| matches!(s, "every_day" | "weekdays" | "custom"));
            }
            if !super::super::api::is_notification_schedule_pref(k)
                || matches!(k.as_str(), "time24" | "tz" | "set_tz_automatically")
            {
                return false;
            }
            if k.starts_with("dnd_enabled_") {
                return v
                    .as_str()
                    .is_some_and(|s| matches!(s, "partial" | "all_day"));
            }
            let Some(s) = v.as_str() else {
                return false;
            };
            let b = s.as_bytes();
            b.len() == 5
                && b[2] == b':'
                && [b[0], b[1], b[3], b[4]].iter().all(u8::is_ascii_digit)
                && s[..2].parse::<u8>().is_ok_and(|h| h < 24)
                && s[3..].parse::<u8>().is_ok_and(|m| m < 60)
        })
}
#[cfg(test)]
mod tests;
