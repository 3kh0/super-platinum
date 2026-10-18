//! No retry, offline hold, redirect, or replay for assignee continuation calls.
use super::*;
pub type ListAssigneeFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_assignee(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListAssigneeFailure> {
        self.execute_list_rename_once(request, current, allowed)
            .await
    }
}
fn allowed(request: &PreparedRequest) -> bool {
    if list_column_rename_read::allowed(request) {
        return true;
    }
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
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    if fields
        .iter()
        .map(|(k, v)| k.len().saturating_add(v.len()))
        .try_fold(0usize, usize::checked_add)
        .is_none_or(|n| n > 16384)
    {
        return false;
    }
    let keys: Vec<_> = fields.iter().map(|(k, _)| k.as_str()).collect();
    let val = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let id = |key: &str, prefix: &[u8]| {
        val(key).is_some_and(|v| {
            v.len() >= 9
                && v.len() <= 128
                && prefix.contains(&v.as_bytes()[0])
                && v.bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
    };
    if val("token").is_none_or(str::is_empty) {
        return false;
    }
    match url.path() {
        "/api/files.canSee" | "/api/files.canShare" => {
            let target = if url.path().ends_with("canSee") {
                "user_id"
            } else {
                "channel_id"
            };
            keys == ["token", "file_id", target, "_x_reason"]
                && id("file_id", b"F")
                && id(target, if target == "user_id" { b"UW" } else { b"D" })
                && val("_x_reason") == Some("canShare")
        }
        "/api/conversations.open" => {
            keys == ["token", "users", "quiet", "return_im", "_x_reason"]
                && id("users", b"UW")
                && val("quiet") == Some("0")
                && val("return_im") == Some("1")
                && val("_x_reason") == Some("get_channel_id_for_sharing")
        }
        "/api/conversations.info" | "/api/conversations.listPrefs" => {
            keys == ["token", "channel", "_x_reason"]
                && id("channel", b"D")
                && val("_x_reason")
                    == Some(if url.path().ends_with("listPrefs") {
                        "client_redux_store"
                    } else {
                        "channels-api/fetchConversationInfo"
                    })
        }
        "/api/experiments.getByUser" => {
            keys == ["token", "ssb_platform", "ssb_release_channel"]
                && matches!(val("ssb_platform"), Some("windows" | "darwin" | "linux"))
                && val("ssb_release_channel") == Some("prod")
        }
        "/api/lists.records.list" => {
            keys == [
                "token",
                "list_id",
                "include_subtasks",
                "archived",
                "include_suggested",
                "_x_reason",
            ] && id("list_id", b"F")
                && val("include_subtasks") == Some("true")
                && val("archived") == Some("false")
                && val("include_suggested") == Some("false")
                && val("_x_reason") == Some("get-list-records")
        }
        "/api/files.share" => {
            let mut expected = vec![
                "token",
                "files",
                "channel",
                "resharing_aware",
                "permissions",
                "skip_dlp_user_warning",
                "from_share_modal",
            ];
            if val("client_context_team_id").is_some() {
                expected.push("client_context_team_id");
                if !id("client_context_team_id", b"T") {
                    return false;
                }
            }
            expected.push("_x_reason");
            if keys != expected
                || !id("files", b"F")
                || !id("channel", b"D")
                || val("resharing_aware") != Some("true")
                || val("skip_dlp_user_warning") != Some("false")
                || val("from_share_modal") != Some("true")
                || val("_x_reason") != Some("share_dialog_permission_share_to_user")
            {
                return false;
            }
            let Some(raw) = val("permissions") else {
                return false;
            };
            let Ok(value) = crate::slash_commands::codec::decode(raw.as_bytes(), 1024) else {
                return false;
            };
            let Some(items) = value.as_array().filter(|a| a.len() == 1) else {
                return false;
            };
            items[0].as_object().is_some_and(|o| o.len() == 2)
                && items[0]["file_id"].as_str() == val("files")
                && matches!(items[0]["grant"].as_str(), Some("read" | "write"))
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests;
