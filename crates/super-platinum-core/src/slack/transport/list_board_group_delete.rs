//! Nonempty projected-cell clear only. Schema uses the existing choice executor.
use super::*;
impl Transport {
    pub async fn execute_list_board_group_delete_cells(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnRenameFailure> {
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
        || url.path() != "/api/lists.cells.update"
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
    let expected = ["token", "list_id", "cells", "_x_reason"];
    if fields.len() != 4
        || fields.iter().zip(expected).any(|((k, _), want)| k != want)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 1024 * 1024)
    {
        return false;
    }
    let identity =
        |s: &str| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
    if fields[0].1.is_empty() || !identity(&fields[1].1) || fields[3].1 != "bulk-update-cells" {
        return false;
    }
    let Ok(raw) = crate::slash_commands::codec::decode(fields[2].1.as_bytes(), 512 * 1024) else {
        return false;
    };
    if raw.to_string() != fields[2].1 {
        return false;
    }
    let Some(cells) = raw.as_array().filter(|v| !v.is_empty() && v.len() <= 1000) else {
        return false;
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut column = None;
    cells.iter().all(|c| {
        let Some(c) = c.as_object().filter(|c| c.len() == 3) else {
            return false;
        };
        let Some(row) = c
            .get("row_id")
            .and_then(Value::as_str)
            .filter(|s| identity(s))
        else {
            return false;
        };
        let Some(col) = c
            .get("column_id")
            .and_then(Value::as_str)
            .filter(|s| identity(s))
        else {
            return false;
        };
        if column.is_some_and(|v| v != col) || !seen.insert(row) {
            return false;
        }
        column = Some(col);
        c.get("select")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    })
}
#[cfg(test)]
mod tests;
