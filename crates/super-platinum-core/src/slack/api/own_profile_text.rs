//! Ordinary self-profile text edit; status and structured sections are separate.
use super::*;
use crate::profile_text::{Field, normalize_and_validate};
use serde_json::Value;

pub fn own_profile_text_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    profile: Value,
) -> Result<PreparedRequest, Error> {
    let invalid = || Error::Api("invalid_own_profile_text".into());
    let fields = profile
        .as_object()
        .filter(|p| !p.is_empty())
        .ok_or_else(invalid)?;
    let mut normalized = serde_json::Map::new();
    for (name, value) in fields {
        let field = match name.as_str() {
            "real_name" => Field::FullName,
            "display_name" => Field::DisplayName,
            "pronouns" => Field::Pronouns,
            _ => return Err(invalid()),
        };
        let value = normalize_and_validate(field, value.as_str().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
        normalized.insert(name.clone(), Value::String(value));
    }
    Ok(client.rest_form(
        workspace,
        "users.profile.set",
        vec![
            ("profile", Value::Object(normalized).to_string()),
            ("reason", "EditProfileFormPane".into()),
        ],
    ))
}

/// Single attempt only. The owner must fetch canonical profile after this call;
/// `ok` confirms acceptance, not the currently visible profile or its ownership.
pub async fn set_own_profile_text(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    profile: Value,
) -> Result<(), Error> {
    transport
        .execute_own_profile_text(own_profile_text_request(client, workspace, profile)?)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn request_only_changes_explicit_self_text_fields_and_preserves_whitespace() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        };
        let request = own_profile_text_request(
            &SlackClient::default(),
            &workspace,
            json!({"display_name":"  Jose\u{301}  ","pronouns":""}),
        )
        .unwrap();
        let super::super::super::client::RequestBody::Form(fields) = request.body else {
            panic!("form required")
        };
        assert_eq!(
            fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            vec!["token", "profile", "reason"]
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields[1].1).unwrap(),
            json!({"display_name":"  José  ","pronouns":""})
        );
        assert_eq!(fields[2].1, "EditProfileFormPane");
        for bad in [
            json!({}),
            json!({"user":"U2"}),
            json!({"status_text":"bad"}),
            json!({"title":"x"}),
            json!({"real_name":""}),
            json!({"pronouns":null}),
        ] {
            assert!(own_profile_text_request(&SlackClient::default(), &workspace, bad).is_err());
        }
    }
}
