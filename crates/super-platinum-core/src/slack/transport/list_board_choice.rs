//! Exact Board choice form; the established one-attempt executor owns HTTP.
use super::*;
pub type ListBoardChoiceFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_board_choice(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListBoardChoiceFailure> {
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
        || url.path() != "/api/lists.columns.update"
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
    let expected = [
        "token",
        "list_id",
        "key",
        "options",
        "backfill_missing_cells_with_default",
        "_x_reason",
    ];
    if fields.len() != expected.len()
        || fields
            .iter()
            .zip(expected)
            .any(|((key, _), want)| key != want)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 64 * 1024)
    {
        return false;
    }
    let value = |i: usize| fields[i].1.as_str();
    !value(0).is_empty()
        && !value(1).trim().is_empty()
        && !value(2).trim().is_empty()
        && serde_json::from_str::<Value>(value(3)).is_ok_and(|options| {
            options.to_string() == value(3)
                && crate::slack_lists::BoardChoiceChange::supported_wire_options(&options)
        })
        && value(4) == "true"
        && value(5) == "update-column"
}

#[cfg(test)]
mod tests;

impl Transport {
    /// Exact loaded-page readback, without guessed cursor/limit or a new executor.
    pub async fn execute_list_board_choice_read(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListBoardChoiceFailure> {
        self.execute_list_rename_once(request, current, |r| {
            super::list_column_rename_read::allowed(r) || allowed_records(r)
        })
        .await
    }
}
pub(super) fn allowed_records(request: &PreparedRequest) -> bool {
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
        || url.path() != "/api/lists.records.list"
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
    let expected = [
        "token",
        "list_id",
        "include_subtasks",
        "archived",
        "include_suggested",
        "_x_reason",
    ];
    if fields.len() != expected.len()
        || fields
            .iter()
            .zip(expected)
            .any(|((key, _), want)| key != want)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 64 * 1024)
    {
        return false;
    }
    let value = |i: usize| fields[i].1.as_str();
    !value(0).is_empty()
        && !value(1).trim().is_empty()
        && value(2) == "true"
        && value(3) == "false"
        && value(4) == "false"
        && value(5) == "get-list-records"
}

#[cfg(test)]
mod read_tests;
