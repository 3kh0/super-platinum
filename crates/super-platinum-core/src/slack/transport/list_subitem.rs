//! Exact root-subitem creation and request-associated parent reads.
use super::*;
pub type ListSubitemFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_subitem_create(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListSubitemFailure> {
        self.execute_list_rename_once(request, current, create_allowed)
            .await
    }
    pub async fn execute_list_subitem_read(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListSubitemFailure> {
        self.execute_list_rename_once(request, current, read_allowed)
            .await
    }
}
fn form_origin(request: &PreparedRequest) -> bool {
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
    true
}

fn form<'a>(
    request: &'a PreparedRequest,
    path: &str,
    expected: &[&str],
) -> Option<&'a [(String, String)]> {
    if !form_origin(request) || url::Url::parse(&request.url).ok()?.path() != path {
        return None;
    }
    let RequestBody::Form(fields) = &request.body else {
        return None;
    };
    if fields.len() != expected.len()
        || fields.iter().zip(expected).any(|((k, _), e)| k != e)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 16384)
        || fields[0].1.is_empty()
    {
        return None;
    }
    Some(fields)
}
fn identity(v: &str) -> bool {
    !v.is_empty() && v.trim() == v && !v.chars().any(char::is_control)
}
fn timestamp(v: &str) -> bool {
    let p = v.split('-').collect::<Vec<_>>();
    p.len() == 3
        && !p[0].is_empty()
        && p[0].bytes().all(|b| b.is_ascii_digit())
        && p[0]
            .parse::<u64>()
            .is_ok_and(|n| n <= 8_640_000_000_000_000)
        && p[1].len() == 4
        && p[1]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
        && p[2].len() == 16
        && p[2]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn create_allowed(request: &PreparedRequest) -> bool {
    let Some(f) = form(
        request,
        "/api/lists.records.create",
        &[
            "token",
            "list_id",
            "position",
            "updated_timestamp",
            "_x_reason",
            "parent_record_id",
        ],
    ) else {
        return false;
    };
    identity(&f[1].1)
        && f[2].1.is_empty()
        && timestamp(&f[3].1)
        && f[4].1 == "create-record"
        && identity(&f[5].1)
}
fn read_allowed(request: &PreparedRequest) -> bool {
    if super::list_column_rename_read::allowed(request) {
        return true;
    }
    if let Some(f) = form(
        request,
        "/api/lists.records.info",
        &[
            "token",
            "list_id",
            "id",
            "include_is_subscribed",
            "_x_reason",
        ],
    ) {
        return identity(&f[1].1)
            && identity(&f[2].1)
            && f[3].1 == "true"
            && f[4].1 == "get-record-subtasks";
    }
    if let Some(f) = form(
        request,
        "/api/experiments.getByUser",
        &["token", "ssb_platform", "ssb_release_channel"],
    ) {
        return matches!(f[1].1.as_str(), "windows" | "darwin" | "linux") && f[2].1 == "prod";
    }
    false
}
#[cfg(test)]
mod tests;
