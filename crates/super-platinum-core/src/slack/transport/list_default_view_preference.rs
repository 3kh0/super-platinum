//! Two exact forms, one shared guarded executor; no offline persistence/replay.
use super::*;
pub type ListDefaultViewPreferenceFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_default_view_preference(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListDefaultViewPreferenceFailure> {
        self.execute_list_rename_once(request, current, |r| allowed(r, false))
            .await
    }
    pub async fn execute_list_default_view_preference_read(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListDefaultViewPreferenceFailure> {
        self.execute_list_rename_once(request, current, |r| allowed(r, true))
            .await
    }
}
fn allowed(request: &PreparedRequest, read: bool) -> bool {
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
        || url.path()
            != if read {
                "/api/users.prefs.get"
            } else {
                "/api/users.prefs.set"
            }
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
    let keys = if read {
        vec!["token", "prefs", "_x_reason"]
    } else {
        vec!["token", "name", "value", "_x_reason"]
    };
    if fields.len() != keys.len()
        || fields
            .iter()
            .zip(keys)
            .any(|((k, _), expected)| k != expected)
        || fields[0].1.is_empty()
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 65 * 1024)
    {
        return false;
    }
    if fields[1].1 != "lists_default_view" {
        return false;
    }
    if read {
        return fields[2].1 == "native-list-default-view-read";
    }
    if fields[3].1 != "prefs-api/setUserPrefByApi" {
        return false;
    }
    let Ok(value) = crate::slash_commands::codec::decode(fields[2].1.as_bytes(), 64 * 1024) else {
        return false;
    };
    let Some(map) = value.as_object() else {
        return false;
    };
    let identity =
        |s: &str| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
    !map.is_empty()
        && map.len() <= 4096
        && map
            .iter()
            .all(|(k, v)| identity(k) && v.as_str().is_some_and(identity))
}
#[cfg(test)]
mod tests;
