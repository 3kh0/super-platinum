//! Search-purpose exact requests over shared single-attempt HTTP mechanics.
//! These methods validate wire data; they confer no renderer/action authority.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub enum SearchReactionRead {
    History,
    Channel,
    Membership,
}
#[derive(Debug)]
pub enum SearchReactionFailure {
    NotRequested(Error),
    Unknown(Error),
}

impl Transport {
    pub async fn execute_search_reaction_read(
        &self,
        request: PreparedRequest,
        kind: SearchReactionRead,
        channel: &str,
        ts: &str,
        current: impl FnMut() -> bool,
    ) -> Result<Value, SearchReactionFailure> {
        let path = match kind {
            SearchReactionRead::History => "/api/conversations.history",
            SearchReactionRead::Channel => "/api/conversations.info",
            SearchReactionRead::Membership => "/api/reactions.get",
        };
        let Some(fields) = fields(&request, path) else {
            return Err(invalid());
        };
        if !valid_read(&request)
            || fields.get("channel") != Some(&channel)
            || match kind {
                SearchReactionRead::History => fields.get("latest") != Some(&ts),
                SearchReactionRead::Membership => fields.get("timestamp") != Some(&ts),
                SearchReactionRead::Channel => false,
            }
        {
            return Err(invalid());
        }
        self.execute_list_rename_once_with(request, current, valid_read, |_, value| Ok(value))
            .await
            .map_err(map_failure)
    }
    /// Once dispatched this drains its bounded original response, even if its
    /// caller's UI retires. It never retries, redirects, holds offline or replays.
    pub async fn execute_search_reaction_once(
        &self,
        request: PreparedRequest,
        channel: &str,
        ts: &str,
        name: &str,
        adding: bool,
        current: impl FnMut() -> bool,
    ) -> Result<Value, SearchReactionFailure> {
        let path = if adding {
            "/api/reactions.add"
        } else {
            "/api/reactions.remove"
        };
        let Some(fields) = fields(&request, path) else {
            return Err(invalid());
        };
        if !valid_write(&request)
            || fields.get("channel") != Some(&channel)
            || fields.get("timestamp") != Some(&ts)
            || fields.get("name") != Some(&name)
        {
            return Err(invalid());
        }
        self.execute_list_rename_once_with(request, current, valid_write, |_, value| Ok(value))
            .await
            .map_err(map_failure)
    }
}
fn invalid() -> SearchReactionFailure {
    SearchReactionFailure::NotRequested(Error::Api("invalid_search_reaction_request".into()))
}
fn map_failure(f: ListColumnRenameFailure) -> SearchReactionFailure {
    match f {
        ListColumnRenameFailure::NotRequested(e) => SearchReactionFailure::NotRequested(e),
        ListColumnRenameFailure::Unconfirmed(e) => SearchReactionFailure::Unknown(e),
    }
}
fn timestamp(s: &str) -> bool {
    s.len() <= 48
        && s.split_once('.').is_some_and(|(a, b)| {
            !a.is_empty() && !b.is_empty() && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
        })
}
fn channel(s: &str) -> bool {
    s.starts_with('C')
        && s.len() > 1
        && s.len() <= 64
        && s.bytes().all(|c| c.is_ascii_alphanumeric())
}
fn fields<'a>(request: &'a PreparedRequest, path: &str) -> Option<BTreeMap<&'a str, &'a str>> {
    if request.method != "POST" {
        return None;
    }
    let url = url::Url::parse(&request.url).ok()?;
    let origin = url.scheme() == "https"
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != path
    {
        return None;
    }
    let mut query = BTreeSet::new();
    for (k, v) in url.query_pairs() {
        if !query.insert(k.to_string())
            || !matches!(
                k.as_ref(),
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
            || (k == "_x_num_retries" && v != "0")
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
        || request.headers.iter().any(|(k, v)| {
            let key = k.to_ascii_lowercase();
            !allowed.contains(&key.as_str())
                || !headers.insert(key.clone())
                || v.is_empty()
                || (key == "content-type" && v != "application/x-www-form-urlencoded")
        })
    {
        return None;
    }
    let RequestBody::Form(rows) = &request.body else {
        return None;
    };
    let mut fields = BTreeMap::new();
    for (k, v) in rows {
        if fields.insert(k.as_str(), v.as_str()).is_some() {
            return None;
        }
    }
    if fields.get("token").is_none_or(|s| s.is_empty())
        || fields.get("channel").is_none_or(|s| !channel(s))
    {
        return None;
    }
    Some(fields)
}
fn valid_read(request: &PreparedRequest) -> bool {
    if let Some(f) = fields(request, "/api/conversations.history") {
        f.len() == 5
            && f.get("latest").is_some_and(|s| timestamp(s))
            && f.get("limit") == Some(&"1")
            && f.get("inclusive") == Some(&"true")
    } else if let Some(f) = fields(request, "/api/conversations.info") {
        f.len() == 3 && f.get("include_num_members") == Some(&"false")
    } else if let Some(f) = fields(request, "/api/reactions.get") {
        f.len() == 4
            && f.get("timestamp").is_some_and(|s| timestamp(s))
            && f.get("full") == Some(&"true")
    } else {
        false
    }
}
fn valid_write(request: &PreparedRequest) -> bool {
    ["/api/reactions.add", "/api/reactions.remove"]
        .into_iter()
        .any(|path| {
            fields(request, path).is_some_and(|f| {
                f.len() == 4
                    && f.get("timestamp").is_some_and(|s| timestamp(s))
                    && f.get("name").is_some_and(|s| {
                        !s.is_empty()
                            && s.len() <= 256
                            && !s.chars().any(|c| c.is_whitespace() || c.is_control())
                    })
            })
        })
}

#[cfg(test)]
mod tests;
