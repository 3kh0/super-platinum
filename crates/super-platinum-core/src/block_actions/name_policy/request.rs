//! Exact captured sQEb/PAdb/mabH/0WGR reads. The caller must supply the
//! correctly routed actual session for this request and preserve its owner.
use crate::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient},
};
pub fn user(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "users.prefs.get", vec![])
}
pub fn team(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "team.prefs.get", vec![])
}
pub fn enterprise(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "enterprise.prefs.get", vec![])
}

/// Actual mL3A/PAdb/IlU1 routing: target_team selects the preference map;
/// credentials, URL and Slack route remain those of the original workspace.
/// The caller must bind this target to its complete acquired membership.
pub fn team_for(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    target_team: &str,
) -> Result<PreparedRequest, super::Error> {
    if !target_team.starts_with('T') || !super::id(target_team) {
        return Err(super::Error::Membership);
    }
    Ok(client.rest_form(
        workspace,
        "team.prefs.get",
        vec![("target_team".into(), target_team.into())],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    // Each request generates three fresh UUIDs. Validate those fields rather
    // than demanding the same random bytes; compare every other URL component.
    fn request_identity(value: &str) -> (url::Url, Vec<(String, String)>) {
        let mut url = url::Url::parse(value).unwrap();
        let mut counts = [0; 3];
        let pairs = url
            .query_pairs()
            .map(|(key, value)| {
                let value = if let Some(index) = ["_x_id", "_x_csid", "fp"]
                    .iter()
                    .position(|name| *name == key)
                {
                    counts[index] += 1;
                    let id = uuid::Uuid::parse_str(&value).unwrap();
                    assert_eq!(id.get_version_num(), 4);
                    assert_eq!(id.get_variant(), uuid::Variant::RFC4122);
                    assert_eq!(id.simple().to_string(), value);
                    "per-request-uuid".to_owned()
                } else {
                    value.into_owned()
                };
                (key.into_owned(), value)
            })
            .collect();
        assert_eq!(counts, [1, 1, 1]);
        url.set_query(None);
        (url, pairs)
    }

    #[test]
    fn foreign_preferences_keep_original_request_identity() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "TCURRENT".into(),
            enterprise_id: Some("EOWN".into()),
            user_id: "U1".into(),
            name: "Synthetic".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic-current-token".into(),
        };
        let plain = team(&client, &workspace);
        for target in ["TCURRENT", "TOTHER"] {
            let actual = team_for(&client, &workspace, target).unwrap();
            assert_eq!(request_identity(&actual.url), request_identity(&plain.url));
            assert_eq!(actual.headers, plain.headers);
            assert_eq!(actual.method, plain.method);
            let RequestBody::Form(mut fields) = actual.body else {
                panic!("form required")
            };
            let at = fields
                .iter()
                .position(|(key, _)| key == "target_team")
                .unwrap();
            assert_eq!(fields.remove(at).1, target);
            assert_eq!(RequestBody::Form(fields), plain.body);
        }
    }
    #[test]
    fn malformed_target_is_not_an_alternative_route() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Synthetic".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        };
        for target in ["", "E1", "T1&token=other", "T1/other", "T 1"] {
            assert_eq!(
                team_for(&client, &workspace, target),
                Err(super::super::Error::Membership)
            );
        }
    }
}
