//! Exact minimal view-create form, sharing only the established one-attempt HTTP mechanics.
use super::*;
pub type ListViewCreateFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_view_create(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListViewCreateFailure> {
        self.execute_list_rename_once(request, current, allowed)
            .await
    }
}
fn allowed(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&request.url) else {
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
        || url.path() != "/api/lists.views.create"
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
    let grouped = fields.len() == 6;
    let expected: &[&str] = if grouped {
        &["token", "list_id", "name", "type", "grouping", "_x_reason"]
    } else {
        &["token", "list_id", "name", "type", "_x_reason"]
    };
    if fields.len() != expected.len()
        || fields
            .iter()
            .zip(expected)
            .any(|((k, _), e)| k.as_str() != *e)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 16 * 1024)
    {
        return false;
    }
    let value = |i: usize| fields[i].1.as_str();
    !value(0).is_empty()
        && !value(1).trim().is_empty()
        && !value(2).trim().is_empty()
        && value(2).encode_utf16().count() <= 60
        && !value(2).chars().any(char::is_control)
        && matches!(value(3), "table" | "board")
        && value(fields.len() - 1) == "create-view"
        && (!grouped || valid_grouping(value(4)))
}

fn valid_grouping(raw: &str) -> bool {
    // Duplicate JSON keys are refused by the shared strict decoder.
    crate::slash_commands::codec::decode(raw.as_bytes(), 8192).is_ok_and(|v| {
        v.as_object().is_some_and(|o| {
            o.len() == 1
                && o.get("group_by")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|key| {
                        !key.trim().is_empty()
                            && key.len() <= 1024
                            && !key.chars().any(char::is_control)
                    })
        })
    })
}

#[cfg(test)]
mod tests;
