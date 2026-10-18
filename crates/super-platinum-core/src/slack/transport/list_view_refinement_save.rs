//! Strict Save forms over the existing guarded one-attempt executor.
use super::*;
pub type ListViewRefinementSaveFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_view_refinement_save(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListViewRefinementSaveFailure> {
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
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
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
    let existing = match url.path() {
        "/api/lists.views.update" => true,
        "/api/lists.views.create" => false,
        _ => return false,
    };
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    if fields
        .iter()
        .map(|(k, v)| k.len().saturating_add(v.len()))
        .try_fold(0usize, usize::checked_add)
        .is_none_or(|n| n > 65 * 1024)
    {
        return false;
    }
    let identity =
        |s: &str| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
    let Some((key, token)) = fields.first() else {
        return false;
    };
    if key != "token" || token.is_empty() {
        return false;
    }
    let order = if existing {
        vec![
            "token",
            "list_id",
            "id",
            "type",
            "filters",
            "info_column_filters",
            "sorts",
            "columns",
            "grouping",
            "show_completed_items",
            "_x_reason",
        ]
    } else {
        vec![
            "token",
            "list_id",
            "name",
            "type",
            "grouping",
            "filters",
            "sorts",
            "columns",
            "show_completed_items",
            "_x_reason",
        ]
    };
    let required = if existing {
        vec![
            "token",
            "list_id",
            "id",
            "type",
            "filters",
            "info_column_filters",
            "sorts",
            "show_completed_items",
            "_x_reason",
        ]
    } else {
        vec![
            "token",
            "list_id",
            "name",
            "type",
            "show_completed_items",
            "_x_reason",
        ]
    };
    let mut last = None;
    for (key, value) in fields {
        let Some(index) = order.iter().position(|k| *k == key) else {
            return false;
        };
        if last.is_some_and(|old| index <= old) {
            return false;
        }
        last = Some(index);
        match key.as_str() {
            "token" => (),
            "list_id" | "id" => {
                if !identity(value) {
                    return false;
                }
            }
            "name" => {
                if value.trim().is_empty()
                    || value.encode_utf16().count() > 60
                    || value.chars().any(char::is_control)
                {
                    return false;
                }
            }
            "type" => {
                if !matches!(value.as_str(), "table" | "board") {
                    return false;
                }
            }
            "show_completed_items" => {
                if !matches!(value.as_str(), "true" | "false") {
                    return false;
                }
            }
            "_x_reason" => {
                if value
                    != if existing {
                        "update-view"
                    } else {
                        "create-view"
                    }
                {
                    return false;
                }
            }
            "info_column_filters" => {
                if value != "[]" {
                    return false;
                }
            }
            "grouping" => {
                let Ok(v) = crate::slash_commands::codec::decode(value.as_bytes(), 64 * 1024)
                else {
                    return false;
                };
                if !v.is_object() {
                    return false;
                }
                if v.get("group_by")
                    .is_some_and(|v| v.as_str().is_none_or(|s| !s.is_empty() && !identity(s)))
                {
                    return false;
                }
                if v.get("order").is_some_and(|v| {
                    v.as_array()
                        .is_none_or(|a| a.len() > 1024 || a.iter().any(|v| !v.is_object()))
                }) {
                    return false;
                }
            }
            "filters" | "sorts" | "columns" => {
                let Ok(v) = crate::slash_commands::codec::decode(value.as_bytes(), 64 * 1024)
                else {
                    return false;
                };
                let Some(rows) = v.as_array() else {
                    return false;
                };
                if rows.len() > 1024 {
                    return false;
                }
                for row in rows {
                    let Some(row) = row.as_object() else {
                        return false;
                    };
                    let keys: &[&str] = match key.as_str() {
                        "filters" => &["key", "operator", "values", "column_id"],
                        "sorts" => &["key", "ascending", "column_id"],
                        _ if existing => &["key", "visible", "id"],
                        _ => &["id", "width", "should_wrap_text", "visible", "position"],
                    };
                    if row.keys().any(|k| !keys.contains(&k.as_str())) {
                        return false;
                    }
                    if let Some(v) = row.get("key") {
                        if !v.as_str().is_some_and(identity) {
                            return false;
                        }
                    }
                    if let Some(v) = row.get("column_id") {
                        if !v.as_str().is_some_and(identity) {
                            return false;
                        }
                    }
                    if key == "filters"
                        && (!row.get("key").is_some_and(Value::is_string)
                            || !row.get("operator").is_some_and(Value::is_string)
                            || !row
                                .get("values")
                                .and_then(Value::as_array)
                                .is_some_and(|a| a.iter().all(Value::is_string)))
                    {
                        return false;
                    }
                    if key == "sorts"
                        && (!row.get("key").is_some_and(Value::is_string)
                            || row
                                .get("ascending")
                                .is_some_and(|v| !v.is_null() && !v.is_boolean()))
                    {
                        return false;
                    }
                    if key == "columns"
                        && (!row.get("id").and_then(Value::as_str).is_some_and(identity)
                            || !row.get("visible").is_some_and(Value::is_boolean)
                            || (existing && (row.len() != 3 || !row.contains_key("key"))))
                    {
                        return false;
                    }
                }
            }
            _ => return false,
        }
    }
    required
        .iter()
        .all(|key| fields.iter().any(|(k, _)| k == key))
}
#[cfg(test)]
mod tests;
