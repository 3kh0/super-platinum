//! Presentation preferences only: named read and distinct global/user scalar writes.
use super::*;
impl Transport {
    pub async fn execute_presentation_notifications(
        &self,
        req: PreparedRequest,
    ) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api(
                "invalid_presentation_notification_request".into(),
            ));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Presentation notification request not sent while offline".into(),
            ));
        }
        let response = self
            .execute_once_using(&req, true, self.single_attempt_client()?)
            .await
            .map_err(|e| match e {
                Error::Offline(_) => Error::Transport(
                    "Presentation notification response lost after dispatch".into(),
                ),
                e => e,
            })?;
        notification_preferences_success(&response)?;
        Ok(response)
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
        || !matches!(
            url.path(),
            "/api/users.prefs.setNotifications" | "/api/users.prefs.set" | "/api/users.prefs.get"
        )
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
    let map: std::collections::BTreeMap<_, _> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    if map.len() != fields.len() {
        return false;
    }
    match url.path() {
        "/api/users.prefs.get" => {
            map.keys().all(|k| matches!(*k, "token" | "prefs"))
                && map.get("prefs") == Some(&"all_notifications_prefs,mute_sounds,mac_ssb_bounce")
        }
        "/api/users.prefs.set" => {
            map.keys()
                .all(|k| matches!(*k, "token" | "name" | "value" | "_x_reason"))
                && match map.get("name").copied() {
                    Some("mute_sounds") => matches!(map.get("value"), Some(&"true" | &"false")),
                    Some("mac_ssb_bounce") => {
                        matches!(map.get("value"), Some(&"" | &"short" | &"long"))
                    }
                    _ => false,
                }
                && map.get("_x_reason") == Some(&"prefs-api/setUserPrefByApi")
        }
        "/api/users.prefs.setNotifications" => {
            if !map.keys().all(|k| {
                matches!(
                    *k,
                    "token" | "name" | "value" | "_x_reason" | "global" | "sync"
                )
            }) || map.get("global") != Some(&"true")
                || map.get("sync") != Some(&"false")
            {
                return false;
            }
            let (Some(name), Some(value), Some(reason)) =
                (map.get("name"), map.get("value"), map.get("_x_reason"))
            else {
                return false;
            };
            match *name {
                "desktop_sound" => {
                    *reason == "" && super::super::api::valid_notification_sound(value)
                }
                "priority_desktop_sound" => {
                    *reason == "select-new-vip-notification-sound"
                        && super::super::api::valid_notification_sound(value)
                }
                "no_text_in_notifications" => *reason == "" && matches!(*value, "true" | "false"),
                _ => false,
            }
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests;
