//! Dedicated global notification admission; no channel patches or generic prefs.
use super::*;
impl Transport {
    pub async fn execute_global_notifications(&self, req: PreparedRequest) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_global_notification_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Global notification request not sent while offline".into(),
            ));
        }
        let response = self
            .execute_once_using(&req, true, self.single_attempt_client()?)
            .await
            .map_err(|e| match e {
                Error::Offline(_) => {
                    Error::Transport("Global notification response lost after dispatch".into())
                }
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
        || url.path() != "/api/users.prefs.setNotifications"
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
    if map.len() != fields.len() || map.get("global") != Some(&"true") {
        return false;
    }
    if let Some(prefs) = map.get("prefs") {
        if map
            .keys()
            .any(|k| !matches!(*k, "token" | "global" | "prefs" | "_x_reason"))
            || map.get("_x_reason") != Some(&"NotificationsChannelDefaults")
        {
            return false;
        }
        let Ok(Value::Array(prefs)) = serde_json::from_str::<Value>(prefs) else {
            return false;
        };
        let mut seen = std::collections::BTreeSet::new();
        !prefs.is_empty()
            && prefs.len() <= 2
            && prefs.iter().all(|p| {
                let Some(p) = p.as_object() else {
                    return false;
                };
                let key = p.get("name").and_then(Value::as_str);
                p.len() == 2
                    && matches!(key, Some("global_desktop" | "global_mobile"))
                    && seen.insert(key)
                    && p.get("value")
                        .and_then(Value::as_str)
                        .is_some_and(|v| matches!(v, "everything" | "mentions_dms"))
            })
    } else {
        if map.keys().any(|k| {
            !matches!(
                *k,
                "token" | "global" | "name" | "value" | "sync" | "_x_reason"
            )
        }) || map.get("sync") != Some(&"false")
        {
            return false;
        }
        let (Some(name), Some(value), Some(reason)) =
            (map.get("name"), map.get("value"), map.get("_x_reason"))
        else {
            return false;
        };
        match *name {
            "global_desktop_push_enabled" | "threads_everything" => {
                *reason == "NotificationsGlobalPrefCheckbox" && matches!(*value, "true" | "false")
            }
            "global_mobile" => {
                matches!(
                    *reason,
                    "NotificationsGlobalPrefCheckbox" | "NotificationsChannelDefaults"
                ) && matches!(*value, "" | "everything" | "mentions_dms" | "nothing")
            }
            "global_keywords" => {
                *reason == "NotificationsKeywordsInput"
                    && value.bytes().filter(|b| *b == b',').count() < 100
            }
            _ => false,
        }
    }
}
#[cfg(test)]
mod tests;
