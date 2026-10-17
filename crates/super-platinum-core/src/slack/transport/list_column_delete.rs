//! Exact ordinary field-deletion form, sharing only established single-attempt HTTP mechanics.
use super::*;
pub type ListColumnDeleteFailure = ListColumnRenameFailure;
impl Transport {
    /// Delete-only metadata/policy reads, including the authenticated owner-only
    /// assignment. The generic rename reader intentionally keeps its old allowlist.
    pub async fn execute_list_column_delete_read(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnDeleteFailure> {
        self.execute_list_rename_once(request, current, read_allowed)
            .await
    }
    pub async fn execute_list_column_delete(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnDeleteFailure> {
        self.execute_list_rename_once(request, current, allowed)
            .await
    }
}
fn form<'a>(
    request: &'a PreparedRequest,
    path: &str,
    expected: &[&str],
) -> Option<&'a Vec<(String, String)>> {
    if request.method != "POST" {
        return None;
    }
    let Ok(url) = url::Url::parse(&request.url) else {
        return None;
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
        || url.path() != path
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
        return None;
    }
    let RequestBody::Form(fields) = &request.body else {
        return None;
    };
    if fields.len() != expected.len()
        || fields
            .iter()
            .zip(expected)
            .any(|((key, _), want)| key != want)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 16 * 1024)
        || fields[0].1.is_empty()
    {
        return None;
    }
    Some(fields)
}
fn allowed(request: &PreparedRequest) -> bool {
    let Some(fields) = form(
        request,
        "/api/lists.columns.delete",
        &["token", "list_id", "key", "_x_reason"],
    ) else {
        return false;
    };
    !fields[1].1.trim().is_empty()
        && !fields[2].1.trim().is_empty()
        && fields[3].1 == "delete-column"
}
fn read_allowed(request: &PreparedRequest) -> bool {
    if super::list_column_rename_read::allowed(request) {
        return true;
    }
    let Some(fields) = form(
        request,
        "/api/experiments.getByUser",
        &["token", "ssb_platform", "ssb_release_channel"],
    ) else {
        return false;
    };
    matches!(fields[1].1.as_str(), "windows" | "darwin" | "linux") && fields[2].1 == "prod"
}

#[cfg(test)]
mod tests;
