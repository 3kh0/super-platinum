//! Exact minimal Text/Number/Checkbox/Date-column-create form, sharing only the established one-attempt HTTP mechanics.
use super::*;
pub type ListColumnCreateFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_column_create(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnCreateFailure> {
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
        || url.path() != "/api/lists.columns.create"
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
    let kind = match fields.get(4).map(|(_, value)| value.as_str()) {
        Some("text") => crate::slack_lists::ColumnCreateKind::Text,
        Some("number") => crate::slack_lists::ColumnCreateKind::NumberDefault0,
        Some("checkbox") => crate::slack_lists::ColumnCreateKind::Checkbox,
        Some("date") => crate::slack_lists::ColumnCreateKind::DateSingleDefault,
        _ => return false,
    };
    let expected: &[&str] = if kind.options().is_none() {
        &["token", "list_id", "view_id", "name", "type", "_x_reason"]
    } else {
        &[
            "token",
            "list_id",
            "view_id",
            "name",
            "type",
            "options",
            "_x_reason",
        ]
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
        && [value(1), value(2)]
            .iter()
            .all(|s| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control))
        && !value(3).trim().is_empty()
        && value(3) == value(3).trim()
        && value(3).encode_utf16().count() <= 256
        && !value(3).chars().any(char::is_control)
        && kind.options().is_none_or(|expected| {
            crate::slash_commands::codec::decode(value(5).as_bytes(), 8192).ok() == Some(expected)
        })
        && value(fields.len() - 1) == "create-column"
}

#[cfg(test)]
mod tests;
