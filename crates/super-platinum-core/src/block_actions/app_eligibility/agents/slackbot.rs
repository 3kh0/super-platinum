//! Closed top-level permission read used by YwqI.isSlackbotAiNextEnabled.
//! The native owning workspace is the source store's teamOrEnterpriseId;
//! the actual Flannel endpoint is discovered within that credential context.
use super::{
    Error, PreparedRequest, SlackClient, User, Value, WorkspaceSession, id, invalid, success,
};
use serde_json::json;

pub const PERMISSION: &str = "ACCESS_SLACK_AI_SLACKBOT";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    team: String,
    user: String,
    allowed: bool,
}
impl Observation {
    pub fn decode(workspace: &WorkspaceSession, raw: &Value) -> Result<Self, Error> {
        scope(workspace)?;
        success(raw)?;
        let allowed = raw
            .get("allowed")
            .and_then(Value::as_array)
            .filter(|r| r.len() <= 4096)
            .ok_or_else(invalid)?;
        if !allowed.iter().all(|v| v.as_str().is_some_and(|v| id(v))) {
            return Err(invalid());
        }
        Ok(Self {
            team: workspace.team_id.clone(),
            user: workspace.user_id.clone(),
            allowed: allowed.iter().any(|v| v.as_str() == Some(PERMISSION)),
        })
    }
    pub fn allows(&self, workspace: &WorkspaceSession) -> bool {
        self.allowed && self.team == workspace.team_id && self.user == workspace.user_id
    }
}
fn scope(workspace: &WorkspaceSession) -> Result<(), Error> {
    let team_id = |id: &str| {
        !id.is_empty()
            && id.len() <= 256
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    if !team_id(&workspace.team_id)
        || !matches!(workspace.team_id.as_bytes().first(), Some(b'T' | b'E'))
        || !id(&workspace.user_id)
        || workspace
            .enterprise_id
            .as_ref()
            .is_some_and(|id| !id.starts_with('E') || !team_id(id))
        || (workspace.team_id.starts_with('E')
            && workspace
                .enterprise_id
                .as_ref()
                .is_some_and(|id| id != &workspace.team_id))
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn is_slackbot(user: &User) -> bool {
    user.id == "USLACKBOT" || user.extra.get("is_slackbot") == Some(&Value::Bool(true))
}
pub fn discovery(
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<PreparedRequest, Error> {
    scope(workspace)?;
    Ok(client.rest_form(
        workspace,
        "api.getFlannelHttpUrl",
        vec![
            ("include_external_workspaces", "true".into()),
            ("_x_reason", "client-needs-valid-flannel-url".into()),
        ],
    ))
}
pub fn request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    discovered: &Value,
) -> Result<PreparedRequest, Error> {
    scope(workspace)?;
    success(discovered)?;
    if discovered.get("error").is_some_and(|v| !v.is_null())
        || discovered
            .get("ttl_seconds")
            .and_then(Value::as_u64)
            .is_none_or(|ttl| ttl == 0)
    {
        return Err(invalid());
    }
    let endpoint = discovered
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let team_url = format!("https://edgeapi.slack.com/cache/{}", workspace.team_id);
    let enterprise_url = workspace
        .enterprise_id
        .as_ref()
        .map(|id| format!("https://edgeapi.slack.com/cache/{id}"));
    // Credential boundary: accept only the exact observed normal Slack edge
    // origin and an authenticated T/E identity, never arbitrary response URLs.
    if endpoint != team_url && enterprise_url.as_deref() != Some(endpoint) {
        return Err(invalid());
    }
    // EVxR's logical store context controls token fields. The server's observed
    // endpoint controls routing; enterprise membership does not fabricate it.
    let mut scoped = workspace.clone();
    scoped.enterprise_id = None;
    let mut body = json!({"user_id":workspace.user_id,"permissions":[PERMISSION],"as_admin":true});
    if workspace.team_id.starts_with('E') {
        body["enterprise_token"] = Value::String(workspace.token.clone());
    }
    let mut request = client
        .edge_json(&scoped, "permissions/info", body)
        .map_err(|_| invalid())?;
    let suffix = request
        .url
        .split_once('?')
        .map(|(_, query)| format!("?{query}"))
        .unwrap_or_default();
    request.url = format!("{endpoint}/permissions/info{suffix}");
    Ok(request)
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
            url: "https://example.test".into(),
            token: "synthetic".into(),
        }
    }
    #[test]
    fn actual_prepared_permission_request_preserves_t_scope_and_explicit_e_scope() {
        for (team, endpoint_team) in [("T1", "T1"), ("T1", "E1"), ("E1", "E1")] {
            let w = workspace(team);
            let r = request(&SlackClient::default(), &w,&json!({"ok":true,"ttl_seconds":3600,"url":format!("https://edgeapi.slack.com/cache/{endpoint_team}")})).unwrap();
            let url = url::Url::parse(&r.url).unwrap();
            assert_eq!(
                url.path(),
                format!("/cache/{endpoint_team}/permissions/info")
            );
            let crate::slack::client::RequestBody::Json(actual) = r.body else {
                panic!("json")
            };
            let mut expected = json!({"user_id":"U1","permissions":[PERMISSION],"as_admin":true,"token":"synthetic"});
            if team == "E1" {
                expected["enterprise_token"] = json!("synthetic");
            }
            assert_eq!(actual, expected);
        }
    }
    #[test]
    fn permission_is_exact_observed_scope_not_membership_or_missing_result() {
        let w = workspace("T1");
        let yes = Observation::decode(&w, &json!({"ok":true,"allowed":[PERMISSION]})).unwrap();
        assert!(yes.allows(&w));
        assert!(!yes.allows(&workspace("E1")));
        let mut other = w.clone();
        other.user_id = "U2".into();
        assert!(!yes.allows(&other));
        assert!(
            !Observation::decode(&w, &json!({"ok":true,"allowed":[]}))
                .unwrap()
                .allows(&w)
        );
        for raw in [
            json!({"ok":false,"allowed":[PERMISSION]}),
            json!({"ok":true}),
            json!({"ok":true,"allowed":[true]}),
            json!({"ok":true,"allowed":PERMISSION}),
        ] {
            assert!(Observation::decode(&w, &raw).is_err());
        }
    }
    #[test]
    fn actual_discovery_form_and_refusal_of_unobserved_or_foreign_endpoints() {
        let w = workspace("T1");
        let client = SlackClient::default();
        let r = discovery(&client, &w).unwrap();
        assert_eq!(
            url::Url::parse(&r.url).unwrap().path(),
            "/api/api.getFlannelHttpUrl"
        );
        let crate::slack::client::RequestBody::Form(form) = r.body else {
            panic!("form")
        };
        assert_eq!(
            form,
            vec![
                ("token".into(), "synthetic".into()),
                ("include_external_workspaces".into(), "true".into()),
                ("_x_reason".into(), "client-needs-valid-flannel-url".into())
            ]
        );
        for endpoint in [
            "https://evil.test/cache/T1",
            "https://edgeapi.slack.com.evil.test/cache/T1",
            "https://edgeapi.slack.com/cache/TOTHER",
            "http://edgeapi.slack.com/cache/T1",
            "https://edgeapi.slack.com/cache/EOTHER",
            "https://edgeapi.slack.com/cache/T1?redirect=1",
        ] {
            assert!(
                request(
                    &client,
                    &w,
                    &json!({"ok":true,"ttl_seconds":3600,"url":endpoint})
                )
                .is_err()
            );
        }
        for bad in [
            json!({"ok":false,"ttl_seconds":3600}),
            json!({"ok":true,"ttl_seconds":0}),
            json!({"ok":true,"ttl_seconds":"3600"}),
            json!({"ok":true,"ttl_seconds":3600,"error":"bad"}),
            json!({"ok":true}),
        ] {
            let mut bad = bad;
            bad["url"] = json!("https://edgeapi.slack.com/cache/T1");
            assert!(request(&client, &w, &bad).is_err());
        }
    }
}
