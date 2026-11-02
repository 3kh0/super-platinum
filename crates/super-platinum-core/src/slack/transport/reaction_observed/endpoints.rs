//! Exact Root/legacy wire compatibility. Independent native policies remain required.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
pub enum RootReactionRead {
    History,
    Channel,
    Membership,
    Preferences,
}
impl Transport {
    /// Exact complete-catalog READ mechanics only. The caller owns the actual
    /// original client/session/team, Root/picker scope and response publication.
    /// Existing RootReactionRead variants and validators remain unchanged.
    pub async fn execute_root_reaction_catalog_read_observed(
        &self,
        r: PreparedRequest,
        current: impl FnMut() -> bool,
        observe: impl FnMut(ReactionWireEvent),
    ) -> Result<Value, ObservedReactionFailure> {
        self.execute_reaction_observed(
            r,
            ReactionRequestKind::Read,
            current,
            valid_catalog_read,
            observe,
        )
        .await
    }

    pub async fn execute_root_reaction_read_observed(
        &self,
        r: PreparedRequest,
        k: RootReactionRead,
        c: &str,
        t: &str,
        current: impl FnMut() -> bool,
        observe: impl FnMut(ReactionWireEvent),
    ) -> Result<Value, ObservedReactionFailure> {
        let path = match k {
            RootReactionRead::History => "/api/conversations.history",
            RootReactionRead::Channel => "/api/conversations.info",
            RootReactionRead::Membership => "/api/reactions.get",
            RootReactionRead::Preferences => "/api/users.prefs.get",
        };
        let Some(f) = fields(&r, path) else {
            return Err(invalid());
        };
        if !channel(c)
            || !timestamp(t)
            || !valid_read(&r)
            || (!matches!(k, RootReactionRead::Preferences) && f.get("channel") != Some(&c))
            || (matches!(k, RootReactionRead::History) && f.get("latest") != Some(&t))
            || (matches!(k, RootReactionRead::Membership) && f.get("timestamp") != Some(&t))
        {
            return Err(invalid());
        }
        self.execute_reaction_observed(r, ReactionRequestKind::Read, current, valid_read, observe)
            .await
    }
    pub async fn execute_root_reaction_once_observed(
        &self,
        r: PreparedRequest,
        c: &str,
        t: &str,
        n: &str,
        adding: bool,
        current: impl FnMut() -> bool,
        observe: impl FnMut(ReactionWireEvent),
    ) -> Result<Value, ObservedReactionFailure> {
        if !channel(c) || !timestamp(t) || !name_ok(n) || !target(&r, c, t, n, adding) {
            return Err(invalid());
        }
        self.execute_reaction_observed(
            r,
            ReactionRequestKind::Write,
            current,
            valid_root_write,
            observe,
        )
        .await
    }
    // This validates the original legacy wire tuple, not Root/Search eligibility.
    pub async fn execute_legacy_reaction_once_observed(
        &self,
        r: PreparedRequest,
        c: &str,
        t: &str,
        n: &str,
        adding: bool,
        current: impl FnMut() -> bool,
        observe: impl FnMut(ReactionWireEvent),
    ) -> Result<Value, ObservedReactionFailure> {
        if !target(&r, c, t, n, adding) {
            return Err(invalid());
        }
        self.execute_reaction_observed(
            r,
            ReactionRequestKind::Write,
            current,
            valid_legacy_write,
            observe,
        )
        .await
    }
}
fn invalid() -> ObservedReactionFailure {
    ObservedReactionFailure::NotRequested(Error::Api("invalid_reaction_request".into()))
}
fn valid_catalog_read(r: &PreparedRequest) -> bool {
    fields(r, "/api/emoji.list").is_some_and(|f| f.len() == 1)
}
fn timestamp(s: &str) -> bool {
    s.len() == 17
        && s.as_bytes()[10] == b'.'
        && s.bytes()
            .enumerate()
            .all(|(i, b)| i == 10 || b.is_ascii_digit())
}
fn channel(s: &str) -> bool {
    (2..=255).contains(&s.len())
        && s.starts_with('C')
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn name_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(|c| c.is_whitespace() || c.is_control())
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
    if fields.get("token").is_none_or(|s| s.is_empty()) {
        return None;
    }
    Some(fields)
}
fn target(r: &PreparedRequest, c: &str, t: &str, n: &str, adding: bool) -> bool {
    let path = if adding {
        "/api/reactions.add"
    } else {
        "/api/reactions.remove"
    };
    fields(r, path).is_some_and(|f| {
        f.len() == 4
            && f.get("channel") == Some(&c)
            && f.get("timestamp") == Some(&t)
            && f.get("name") == Some(&n)
            && !c.is_empty()
            && !t.is_empty()
            && !n.is_empty()
    })
}
fn valid_legacy_write(r: &PreparedRequest) -> bool {
    ["/api/reactions.add", "/api/reactions.remove"]
        .into_iter()
        .any(|p| {
            fields(r, p).is_some_and(|f| {
                f.len() == 4
                    && ["channel", "timestamp", "name"]
                        .into_iter()
                        .all(|k| f.get(k).is_some_and(|s| !s.is_empty()))
            })
        })
}
fn valid_root_write(r: &PreparedRequest) -> bool {
    valid_legacy_write(r)
        && ["/api/reactions.add", "/api/reactions.remove"]
            .into_iter()
            .any(|p| {
                fields(r, p).is_some_and(|f| {
                    channel(f["channel"]) && timestamp(f["timestamp"]) && name_ok(f["name"])
                })
            })
}
fn valid_read(r: &PreparedRequest) -> bool {
    if let Some(f) = fields(r, "/api/users.prefs.get") {
        return f.len() == 1;
    }
    if let Some(f) = fields(r, "/api/conversations.history") {
        return f.len() == 5
            && f.get("channel").is_some_and(|c| channel(c))
            && f.get("latest").is_some_and(|t| timestamp(t))
            && f.get("limit") == Some(&"1")
            && f.get("inclusive") == Some(&"true");
    }
    if let Some(f) = fields(r, "/api/conversations.info") {
        return f.len() == 3
            && f.get("channel").is_some_and(|c| channel(c))
            && f.get("include_num_members") == Some(&"false");
    }
    fields(r, "/api/reactions.get").is_some_and(|f| {
        f.len() == 4
            && f.get("channel").is_some_and(|c| channel(c))
            && f.get("timestamp").is_some_and(|t| timestamp(t))
            && f.get("full") == Some(&"true")
    })
}
