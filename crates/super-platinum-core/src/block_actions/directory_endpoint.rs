//! EVxR discovers routing separately from the logical T/E credential context.
pub mod channel_info;
pub mod member_info;
pub mod usergroup_info;
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::{Value, json};
fn invalid() -> Error {
    Error::Api("directory_endpoint_invalid".into())
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 255 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
use std::time::{Duration, Instant};

/// A short-lived endpoint observation, never a selectable-directory permit.
/// No Debug implementation: the exact credential owner is deliberately private.
pub struct DirectoryEndpoint {
    owner: WorkspaceSession,
    url: String,
    expires: Instant,
}
impl DirectoryEndpoint {
    pub fn discovery(client: &SlackClient, w: &WorkspaceSession) -> Result<PreparedRequest, Error> {
        scope(w)?;
        Ok(client.rest_form(
            w,
            "api.getFlannelHttpUrl",
            vec![
                ("include_external_workspaces", "true".into()),
                ("_x_reason", "client-needs-valid-flannel-url".into()),
            ],
        ))
    }
    pub fn decode(w: &WorkspaceSession, raw: &Value, observed: Instant) -> Result<Self, Error> {
        scope(w)?;
        if raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(invalid());
        }
        let ttl = raw["ttl_seconds"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or_else(invalid)?;
        // EVxR retires cached endpoints 2.5 seconds before the server TTL.
        let lifetime = Duration::from_secs(ttl).saturating_sub(Duration::from_millis(2500));
        let expires = observed.checked_add(lifetime).ok_or_else(invalid)?;
        let url = raw["url"].as_str().ok_or_else(invalid)?;
        let team = format!("https://edgeapi.slack.com/cache/{}", w.team_id);
        let enterprise = w
            .enterprise_id
            .as_ref()
            .map(|id| format!("https://edgeapi.slack.com/cache/{id}"));
        if url != team && enterprise.as_deref() != Some(url) {
            return Err(invalid());
        }
        Ok(Self {
            owner: w.clone(),
            url: url.into(),
            expires,
        })
    }
    pub fn current(&self, w: &WorkspaceSession, now: Instant) -> bool {
        now < self.expires
            && self.owner.team_id == w.team_id
            && self.owner.enterprise_id == w.enterprise_id
            && self.owner.user_id == w.user_id
            && self.owner.token == w.token
            && self.owner.url == w.url
    }
    pub fn request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        now: Instant,
        path: &str,
        body: Value,
    ) -> Result<PreparedRequest, Error> {
        if !body.is_object()
            || !self.current(w, now)
            || !matches!(path, "users/search" | "channels/search" | "mpims/search")
        {
            return Err(invalid());
        }
        self.observed_request(client, w, path, body)
    }
    fn observed_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        path: &str,
        mut body: Value,
    ) -> Result<PreparedRequest, Error> {
        // EVxR uses its logical store identity, not enterprise membership, to
        // add enterprise_token. The observed endpoint independently routes it.
        let mut scoped = w.clone();
        scoped.enterprise_id = None;
        if w.team_id.starts_with('E') {
            body["enterprise_token"] = json!(w.token);
        }
        let mut request = client
            .edge_json(&scoped, path, body)
            .map_err(|_| invalid())?;
        let suffix = request
            .url
            .split_once('?')
            .map(|(_, q)| format!("?{q}"))
            .unwrap_or_default();
        request.url = format!("{}/{path}{suffix}", self.url);
        Ok(request)
    }
}
fn scope(w: &WorkspaceSession) -> Result<(), Error> {
    let valid = |v: &str| {
        !v.is_empty() && v.len() <= 256 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    if !valid(&w.team_id)
        || !matches!(w.team_id.as_bytes().first(), Some(b'T' | b'E'))
        || !id(&w.user_id)
        || w.enterprise_id
            .as_ref()
            .is_some_and(|v| !v.starts_with('E') || !valid(v))
        || (w.team_id.starts_with('E') && w.enterprise_id.as_ref().is_some_and(|v| v != &w.team_id))
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace(team: &str) -> WorkspaceSession {
        WorkspaceSession {
            team_id: team.into(),
            enterprise_id: Some("E1".into()),
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.test".into(),
            token: "synthetic".into(),
        }
    }
    #[test]
    fn only_directory_object_requests_use_the_observed_endpoint() {
        let now = Instant::now();
        let w = workspace("T1");
        let endpoint = DirectoryEndpoint::decode(
            &w,
            &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/T1"}),
            now,
        )
        .unwrap();
        for path in ["users/search", "channels/search", "mpims/search"] {
            assert!(
                endpoint
                    .request(
                        &SlackClient::default(),
                        &w,
                        now,
                        path,
                        json!({"query":"Ada"})
                    )
                    .is_ok()
            );
        }
        for path in [
            "permissions/info",
            "conversations/search",
            "users/search?token=other",
            "../write",
        ] {
            assert!(
                endpoint
                    .request(
                        &SlackClient::default(),
                        &w,
                        now,
                        path,
                        json!({"query":"Ada"})
                    )
                    .is_err()
            );
        }
        for body in [json!(null), json!(true), json!([]), json!("text")] {
            assert!(
                endpoint
                    .request(&SlackClient::default(), &w, now, "users/search", body)
                    .is_err()
            );
        }
    }
    #[test]
    fn observed_route_and_logical_credentials_remain_independent() {
        let now = Instant::now();
        for (logical, route) in [("T1", "T1"), ("T1", "E1"), ("E1", "E1")] {
            let w = workspace(logical);
            let e=DirectoryEndpoint::decode(&w,&json!({"ok":true,"ttl_seconds":60,"url":format!("https://edgeapi.slack.com/cache/{route}")}),now).unwrap();
            let request = e
                .request(
                    &SlackClient::default(),
                    &w,
                    now,
                    "users/search",
                    json!({"query":"Ada"}),
                )
                .unwrap();
            assert_eq!(
                url::Url::parse(&request.url).unwrap().path(),
                format!("/cache/{route}/users/search")
            );
            let crate::slack::client::RequestBody::Json(raw) = request.body else {
                panic!("json")
            };
            assert_eq!(raw["token"], "synthetic");
            assert!(raw.get("enterprise_id").is_none());
            let expected_enterprise_token = json!("synthetic");
            assert_eq!(
                raw.get("enterprise_token"),
                if logical == "E1" {
                    Some(&expected_enterprise_token)
                } else {
                    None
                }
            );
        }
    }
    #[test]
    fn expiration_and_credential_replacement_cannot_reuse_discovery() {
        let now = Instant::now();
        let w = workspace("T1");
        let e = DirectoryEndpoint::decode(
            &w,
            &json!({"ok":true,"ttl_seconds":3,"url":"https://edgeapi.slack.com/cache/T1"}),
            now,
        )
        .unwrap();
        assert!(e.current(&w, now));
        assert!(e.current(&w, now + Duration::from_millis(499)));
        assert!(!e.current(&w, now + Duration::from_millis(500)));
        for field in ["team", "user", "token", "url", "enterprise"] {
            let mut changed = w.clone();
            match field {
                "team" => changed.team_id = "T2".into(),
                "user" => changed.user_id = "U2".into(),
                "token" => changed.token = "other".into(),
                "url" => changed.url = "https://other.test".into(),
                _ => changed.enterprise_id = None,
            };
            assert!(!e.current(&changed, now));
        }
        for url in [
            "https://foreign.test/cache/T1",
            "https://edgeapi.slack.com/cache/T2",
            "https://edgeapi.slack.com/cache/T1?redirect=1",
            "http://edgeapi.slack.com/cache/T1",
        ] {
            assert!(
                DirectoryEndpoint::decode(&w, &json!({"ok":true,"ttl_seconds":1,"url":url}), now)
                    .is_err()
            );
        }
    }
}
