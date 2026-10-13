//! Captured hZPA `bots.info` fallback, narrowed to one requested bot.
//! The desktop protocol uses plural `bots`/`app_ids` and returns a `bots` array.

use serde_json::Value;

use crate::config::WorkspaceSession;

use super::super::models::BotProfile;
use super::{Error, PreparedRequest, SlackClient};

/// Prepare a lookup without dispatching. An app ID is only a routing hint;
/// neither it nor the workspace identity is used to fill missing response data.
pub fn bots_info(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    bot_id: &str,
    app_id_hint: Option<&str>,
) -> Result<PreparedRequest, Error> {
    validate_request(bot_id, app_id_hint)?;
    let mut fields = vec![("bots", bot_id.to_owned())];
    if let Some(app_id) = app_id_hint {
        fields.push(("app_ids", app_id.to_owned()));
    }
    Ok(client.rest_form(workspace, "bots.info", fields))
}

/// Decode only the requested identity. A successful empty array is a lookup
/// miss, not a usable profile. Missing app/team metadata stays missing, and a
/// returned app team may differ from the requesting workspace (Slack Connect).
pub fn decode_bots_info(
    value: &Value,
    bot_id: &str,
    app_id_hint: Option<&str>,
) -> Result<Option<BotProfile>, Error> {
    validate_request(bot_id, app_id_hint)?;
    match value.get("ok").and_then(Value::as_bool) {
        Some(true) => {}
        // Do not copy arbitrary endpoint response content into loggable errors.
        Some(false) => return Err(Error::Api("bots_info_rejected".into())),
        None => return Err(invalid()),
    }
    let bots = value
        .get("bots")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let bot = match bots.as_slice() {
        [] => return Ok(None),
        [bot] if bot.is_object() => bot,
        _ => return Err(invalid()),
    };
    if bot.get("id").and_then(Value::as_str) != Some(bot_id) {
        return Err(invalid());
    }
    let app_id = optional_identity(bot, "app_id")?;
    let _team_id = optional_identity(bot, "team_id")?;
    if let Some(app_id) = app_id {
        if !prefixed_identity(app_id, b'A') || app_id_hint.is_some_and(|hint| hint != app_id) {
            return Err(invalid());
        }
    }
    serde_json::from_value(bot.clone())
        .map(Some)
        .map_err(|_| invalid())
}

fn validate_request(bot_id: &str, app_id_hint: Option<&str>) -> Result<(), Error> {
    if !prefixed_identity(bot_id, b'B')
        || app_id_hint.is_some_and(|app_id| !prefixed_identity(app_id, b'A'))
    {
        return Err(Error::Api("bots_info_identity_invalid".into()));
    }
    Ok(())
}

fn identity(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn prefixed_identity(value: &str, prefix: u8) -> bool {
    value.len() > 1 && value.as_bytes()[0] == prefix && identity(value)
}

fn optional_identity<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if identity(id) => Ok(Some(id)),
        _ => Err(invalid()),
    }
}

fn invalid() -> Error {
    Error::Api("bots_info_response_invalid".into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::slack::client::RequestBody;

    fn workspace() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-token".into(),
        }
    }

    #[test]
    fn request_uses_plural_fields_and_omits_absent_app_hint() {
        for app_id in [None, Some("A1")] {
            let request = bots_info(&SlackClient::default(), &workspace(), "B1", app_id).unwrap();
            assert_eq!(request.method, "POST");
            assert!(
                request
                    .url
                    .starts_with("https://fixture.slack.com/api/bots.info?")
            );
            let RequestBody::Form(fields) = request.body else {
                panic!("form expected")
            };
            let fields: BTreeMap<_, _> = fields
                .into_iter()
                .filter(|(key, _)| key != "token")
                .collect();
            let mut expected = BTreeMap::from([("bots".to_owned(), "B1".to_owned())]);
            if let Some(app_id) = app_id {
                expected.insert("app_ids".into(), app_id.into());
            }
            assert_eq!(fields, expected);
        }
        for id in ["", "B", "B1,B2", "B1\n", "@B1", "U1"] {
            assert!(bots_info(&SlackClient::default(), &workspace(), id, None).is_err());
        }
        for hint in ["", " A1", "A1,A2", "B1"] {
            assert!(bots_info(&SlackClient::default(), &workspace(), "B1", Some(hint)).is_err());
        }
    }

    #[test]
    fn decoder_preserves_metadata_and_never_infers_routing() {
        let value = json!({"ok":true,"bots":[{"id":"B1","app_id":"A1","team_id":"TOTHER","updated":123,"custom":{"kept":true}}]});
        let bot = decode_bots_info(&value, "B1", Some("A1")).unwrap().unwrap();
        assert_eq!(bot.id.as_deref(), Some("B1"));
        assert_eq!(bot.extra["app_id"], "A1");
        assert_eq!(bot.extra["team_id"], "TOTHER");
        assert_eq!(bot.extra["updated"], 123);
        assert_eq!(bot.extra["custom"], json!({"kept":true}));
        let bot = decode_bots_info(&json!({"ok":true,"bots":[{"id":"B1"}]}), "B1", Some("A1"))
            .unwrap()
            .unwrap();
        assert!(!bot.extra.contains_key("app_id"));
        assert!(!bot.extra.contains_key("team_id"));
        assert!(
            decode_bots_info(&json!({"ok":true,"bots":[]}), "B1", None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn decoder_requires_explicit_success_and_exact_identity() {
        for value in [
            json!({"bots":[{"id":"B1"}]}),
            json!({"ok":"true","bots":[{"id":"B1"}]}),
            json!({"ok":false,"bots":[{"id":"B1"}]}),
            json!({"ok":true}),
            json!({"ok":true,"bot":{"id":"B1"}}),
            json!({"ok":true,"bots":{}}),
            json!({"ok":true,"bots":[null]}),
            json!({"ok":true,"bots":[{}]}),
            json!({"ok":true,"bots":[{"id":"B2"}]}),
            json!({"ok":true,"bots":[{"id":"B1"},{"id":"B1"}]}),
            json!({"ok":true,"bots":[{"id":"B1"},{"id":"B2"}]}),
            json!({"ok":true,"bots":[{"id":"B1","app_id":"A2"}]}),
            json!({"ok":true,"bots":[{"id":"B1","app_id":42}]}),
            json!({"ok":true,"bots":[{"id":"B1","app_id":""}]}),
            json!({"ok":true,"bots":[{"id":"B1","team_id":" T1"}]}),
            json!({"ok":true,"bots":[{"id":"B1","team_id":{}}]}),
            json!({"ok":true,"bots":[{"id":"B1","name":false}]}),
        ] {
            assert!(decode_bots_info(&value, "B1", Some("A1")).is_err());
        }
    }
}
