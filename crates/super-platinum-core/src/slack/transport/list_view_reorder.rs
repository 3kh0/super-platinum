//! Exact position-only saved-view move form, sharing only the established one-attempt HTTP mechanics.
use super::*;
pub type ListViewReorderFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_view_reorder(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListViewReorderFailure> {
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
        || url.path() != "/api/lists.views.update"
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
    let expected = ["token", "list_id", "id", "position", "_x_reason"];
    if fields.len() != expected.len()
        || fields.iter().zip(expected).any(|((k, _), e)| k != e)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 16 * 1024)
    {
        return false;
    }
    let text = |i: usize| fields[i].1.as_str();
    let identity =
        |s: &str| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
    if text(0).is_empty() || !identity(text(1)) || !identity(text(2)) || text(4) != "update-view" {
        return false;
    }
    crate::slack_lists::ViewReorder::supports_position(text(3))
}
#[cfg(test)]
mod tests;
