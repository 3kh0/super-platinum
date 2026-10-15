//! Nu4c users.prefs.get/set shapes; 3l3p serializes frecency exactly once.
use crate::{
    config::WorkspaceSession,
    slack::{
        Error,
        client::{PreparedRequest, SlackClient},
    },
};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn preferences_request(client: &SlackClient, session: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        session,
        "users.prefs.get",
        vec![(
            "prefs",
            "preferred_skin_tone,emoji_mode,emoji_use,frecency".into(),
        )],
    )
}

pub fn parse_preferences(value: &Value) -> Result<BTreeMap<String, Value>, Error> {
    success(value)?;
    value
        .get("prefs")
        .and_then(Value::as_object)
        .map(|prefs| prefs.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .ok_or_else(|| Error::Api("invalid_emoji_preferences_response".into()))
}

/// `wire` is History::to_wire(), not a JSON string containing that JSON text.
pub fn frecency_request(
    client: &SlackClient,
    session: &WorkspaceSession,
    wire: String,
) -> Result<PreparedRequest, Error> {
    super::history::History::from_wire(&wire)
        .map_err(|_| Error::Api("invalid_emoji_frecency".into()))?;
    Ok(client.rest_form(
        session,
        "users.prefs.set",
        vec![("name", "frecency".into()), ("value", wire)],
    ))
}

pub fn success(value: &Value) -> Result<(), Error> {
    if value.get("ok").and_then(Value::as_bool) == Some(true)
        && value.get("error").is_none_or(Value::is_null)
    {
        return Ok(());
    }
    Err(Error::Api(
        value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("invalid_emoji_preferences_response")
            .into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    #[test]
    fn exact_pref_names_and_single_json_encoding_preserve_order() {
        let session = WorkspaceSession {
            team_id: "T".into(),
            user_id: "U".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
            enterprise_id: None,
        };
        let client = SlackClient::default();
        let read = preferences_request(&client, &session);
        assert_eq!(
            url::Url::parse(&read.url).unwrap().path(),
            "/api/users.prefs.get"
        );
        let RequestBody::Form(fields) = read.body else {
            panic!("form");
        };
        assert!(fields.contains(&(
            "prefs".into(),
            "preferred_skin_tone,emoji_mode,emoji_use,frecency".into()
        )));
        let wire =
            r#"{"z":{"id":"Ez","count":1,"visits":[1]},"a":{"id":"Ea","count":1,"visits":[1]}}"#;
        let write = frecency_request(&client, &session, wire.into()).unwrap();
        assert_eq!(
            url::Url::parse(&write.url).unwrap().path(),
            "/api/users.prefs.set"
        );
        let RequestBody::Form(fields) = write.body else {
            panic!("form");
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "synthetic".into()),
                ("name".into(), "frecency".into()),
                ("value".into(), wire.into())
            ]
        );
        assert!(frecency_request(&client, &session, "[]".into()).is_err());
        assert!(parse_preferences(&serde_json::json!({"ok":true})).is_err());
        assert!(parse_preferences(&serde_json::json!({"ok":false,"prefs":{}})).is_err());
    }
}
