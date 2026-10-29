//! One typed self preference, never a generic setter or a confirmation of its
//! canonical value. Caller retains the genuine original user-intent authority.
//! Native metadata/routing remain explicitly non-JavaScript-equivalent policy;
//! Source mode/sonic/app wrapper fields are not inferred from missing state.
use super::file_pages::Sort;
use super::*;

pub struct Request {
    pub(in crate::slack) inner: PreparedRequest,
    pub(in crate::slack) destination: String,
    pub(in crate::slack) sort: Sort,
}
impl Request {
    /// Nonsecret original submitted choice; never a canonical preference value.
    pub fn submitted_sort(&self) -> Sort {
        self.sort
    }
}
pub fn request(
    client: &SlackClient,
    session: &WorkspaceSession,
    sort: Sort,
) -> Result<Request, Error> {
    let id =
        |s: &str| !s.is_empty() && s.len() <= 255 && s.bytes().all(|b| b.is_ascii_alphanumeric());
    if session.token.is_empty()
        || session.token.len() > 16 * 1024
        || !id(&session.team_id)
        || !id(&session.user_id)
    {
        return Err(invalid());
    }
    let inner = client.file_sort_preference_multipart(session, sort.raw());
    let url = url::Url::parse(&inner.url).map_err(|_| invalid())?;
    let host = url.host_str().ok_or_else(invalid)?;
    if url.scheme() != "https"
        || !host.ends_with(".slack.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
        || url.path() != "/api/users.prefs.set"
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(Request {
        destination: host.into(),
        inner,
        sort,
    })
}
/// Data/error validation only. A successful operation is not a canonical
/// preference readback, and supplies no current account or producer authority.
pub fn validate_response(value: &serde_json::Value) -> Result<(), Error> {
    if value.get("ok") == Some(&serde_json::Value::Bool(true))
        && value.get("error").is_none_or(serde_json::Value::is_null)
    {
        return Ok(());
    }
    let code = value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("file_sort_save_failed");
    if matches!(
        code,
        "ratelimited"
            | "rate_limited"
            | "rate_limit_exceeded"
            | "rate_limit_reached"
            | "ratelimit"
            | "rate_limit"
    ) {
        return Err(Error::RateLimited {
            retry_after_secs: None,
        });
    }
    Err(Error::Api(
        if code.len() <= 64 && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            code.into()
        } else {
            "file_sort_save_failed".into()
        },
    ))
}
fn invalid() -> Error {
    Error::Api("file_sort_preference_not_admitted".into())
}

#[cfg(test)]
mod tests;
