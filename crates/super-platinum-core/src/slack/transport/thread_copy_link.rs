//! Copy-purpose exact raw reads over private single-attempt HTTP mechanics.
//! These methods validate requests; they confer no clipboard/action authority.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub enum ThreadCopyRead {
    History,
    Channel,
}

#[derive(Debug)]
pub enum ThreadCopyReadFailure {
    NotRequested(Error),
    Failed(Error),
}

impl Transport {
    /// No offline hold, retry or redirect. Once dispatched, drains the original
    /// bounded response even after caller retirement; publication stays external.
    pub async fn execute_thread_copy_read(
        &self,
        request: PreparedRequest,
        kind: ThreadCopyRead,
        channel: &str,
        ts: &str,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ThreadCopyReadFailure> {
        let path = match kind {
            ThreadCopyRead::History => "/api/conversations.history",
            ThreadCopyRead::Channel => "/api/conversations.info",
        };
        let Some(fields) = fields(&request, path) else {
            return Err(invalid());
        };
        if !timestamp(ts)
            || !valid_read(&request)
            || fields.get("channel") != Some(&channel)
            || matches!(kind, ThreadCopyRead::History) && fields.get("latest") != Some(&ts)
        {
            return Err(invalid());
        }
        self.execute_list_rename_once_with(request, current, valid_read, |_, value| Ok(value))
            .await
            .map_err(|failure| match failure {
                ListColumnRenameFailure::NotRequested(error) => {
                    ThreadCopyReadFailure::NotRequested(error)
                }
                ListColumnRenameFailure::Unconfirmed(error) => ThreadCopyReadFailure::Failed(error),
            })
    }
}

fn invalid() -> ThreadCopyReadFailure {
    ThreadCopyReadFailure::NotRequested(Error::Api("invalid_thread_copy_read_request".into()))
}
fn timestamp(ts: &str) -> bool {
    ts.len() == 17
        && ts.as_bytes()[10] == b'.'
        && ts
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 10 || b.is_ascii_digit())
}
fn channel(id: &str) -> bool {
    (2..=255).contains(&id.len())
        && id.as_bytes()[0] == b'C'
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn fields<'a>(request: &'a PreparedRequest, path: &str) -> Option<BTreeMap<&'a str, &'a str>> {
    if request.method != "POST" {
        return None;
    }
    let url = url::Url::parse(&request.url).ok()?;
    let origin = url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || url.as_str() != request.url.as_str()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != path
    {
        return None;
    }
    let mut query = BTreeSet::new();
    for (key, value) in url.query_pairs() {
        if !query.insert(key.to_string())
            || !matches!(
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
            || (key == "_x_num_retries" && value != "0")
        {
            return None;
        }
    }
    let allowed = [
        "sec-ch-ua-platform",
        "referer",
        "user-agent",
        "sec-ch-ua",
        "content-type",
        "sec-ch-ua-mobile",
    ];
    let mut headers = BTreeSet::new();
    if request.headers.len() != allowed.len()
        || request.headers.iter().any(|(name, value)| {
            let key = name.to_ascii_lowercase();
            !allowed.contains(&key.as_str())
                || !headers.insert(key.clone())
                || value.is_empty()
                || (key == "content-type" && value != "application/x-www-form-urlencoded")
        })
    {
        return None;
    }
    let RequestBody::Form(rows) = &request.body else {
        return None;
    };
    let mut fields = BTreeMap::new();
    for (key, value) in rows {
        if fields.insert(key.as_str(), value.as_str()).is_some() {
            return None;
        }
    }
    if fields
        .get("token")
        .is_none_or(|s| s.is_empty() || s.chars().any(|c| c.is_whitespace() || c.is_control()))
        || fields.get("channel").is_none_or(|s| !channel(s))
        || rows
            .iter()
            .map(|(key, value)| key.len().saturating_add(value.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|len| len > 16 * 1024)
    {
        return None;
    }
    Some(fields)
}

fn valid_read(request: &PreparedRequest) -> bool {
    if let Some(fields) = fields(request, "/api/conversations.history") {
        fields.len() == 5
            && fields.get("latest").is_some_and(|ts| timestamp(ts))
            && fields.get("inclusive") == Some(&"true")
            && fields.get("limit") == Some(&"1")
    } else if let Some(fields) = fields(request, "/api/conversations.info") {
        fields.len() == 3 && fields.get("include_num_members") == Some(&"false")
    } else {
        false
    }
}

#[cfg(test)]
mod tests;
