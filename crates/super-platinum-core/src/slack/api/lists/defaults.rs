//! Captured List opening/read state, separate from file-view presence.
//! B7297948/yY35 and B18029040/X4ex; see slack-lists-protocol.md.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListOpen {
    /// Prior read time, in Unix seconds (captured Dayjs.unix consumer).
    pub last_read_timestamp: Option<u64>,
    pub resolved_default_view: Option<String>,
}
pub fn lists_open(c: &SlackClient, w: &WorkspaceSession, id: &str) -> PreparedRequest {
    c.rest_form(
        w,
        "lists.open",
        vec![("list_id", id.into()), ("_x_reason", "open-list".into())],
    )
}
pub fn files_mark_read(
    c: &SlackClient,
    w: &WorkspaceSession,
    id: &str,
    timestamp_ms: u64,
) -> PreparedRequest {
    c.rest_form(
        w,
        "files.markRead",
        vec![
            ("file_id", id.into()),
            ("timestamp", timestamp_ms.to_string()),
            ("_x_reason", "mark-list-read".into()),
        ],
    )
}
pub fn list_default_view(c: &SlackClient, w: &WorkspaceSession, id: &str) -> PreparedRequest {
    c.rest_form(
        w,
        "lists.getDefaultView",
        vec![
            ("list_id", id.into()),
            ("_x_reason", "fetch-personal-default-view".into()),
        ],
    )
}
pub fn parse_list_default_view(response: &Value) -> Result<Option<String>, Error> {
    if response.get("ok") == Some(&Value::Bool(false))
        && response.get("error").and_then(Value::as_str) == Some("default_view_not_found")
    {
        return Ok(None);
    }
    if response.get("ok") != Some(&Value::Bool(true)) {
        return Err(Error::Api(
            "Personal List default lookup was not successful.".into(),
        ));
    }
    response
        .get("view_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.trim() == *id)
        .map(|id| Some(id.into()))
        .ok_or_else(|| Error::Api("Personal List default identity is missing or malformed.".into()))
}
pub async fn fetch_list_default_view(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    id: &str,
) -> Result<Option<String>, Error> {
    match t.execute_list_read_state(list_default_view(c, w, id)).await {
        Ok(response) => parse_list_default_view(&response),
        Err(Error::Api(code)) if code == "default_view_not_found" => Ok(None),
        Err(error) => Err(error),
    }
}
pub fn parse_list_open(response: &Value) -> Result<ListOpen, Error> {
    if response.get("ok") != Some(&Value::Bool(true)) {
        return Err(Error::Api("List open was not successful.".into()));
    }
    let last_read_timestamp = match response.get("last_read_timestamp") {
        None | Some(Value::Null) => None,
        Some(Value::String(text))
            if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Some(
                text.parse()
                    .map_err(|_| Error::Api("List prior read timestamp is malformed.".into()))?,
            )
        }
        Some(value) => Some(
            value
                .as_u64()
                .ok_or_else(|| Error::Api("List prior read timestamp is malformed.".into()))?,
        ),
    };
    let resolved_default_view = match response.get("resolved_default_view") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) if value.trim() == value => Some(value.clone()),
        _ => return Err(Error::Api("Resolved List view is malformed.".into())),
    };
    Ok(ListOpen {
        last_read_timestamp,
        resolved_default_view,
    })
}
pub fn parse_file_mark_read(response: &Value) -> Result<(), Error> {
    if response.get("ok") == Some(&Value::Bool(true)) {
        Ok(())
    } else {
        Err(Error::Api("List read marking was not successful.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    use serde_json::json;
    #[test]
    fn personal_default_lookup_only_treats_named_absence_as_empty() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "T_TEST".into(),
            enterprise_id: None,
            user_id: "U_TEST".into(),
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "fixture-not-a-token".into(),
        };
        let req = list_default_view(&client, &workspace, "F_TEST");
        assert!(req.url.contains("/api/lists.getDefaultView?"));
        let RequestBody::Form(fields) = req.body else {
            panic!()
        };
        assert!(fields.contains(&("list_id".into(), "F_TEST".into())));
        assert!(fields.contains(&("_x_reason".into(), "fetch-personal-default-view".into())));
        assert_eq!(
            parse_list_default_view(&json!({"ok":true,"view_id":"V_TEST"})).unwrap(),
            Some("V_TEST".into())
        );
        assert_eq!(
            parse_list_default_view(&json!({"ok":false,"error":"default_view_not_found"})).unwrap(),
            None
        );
        for error in ["permission_denied", "list_not_found", "invalid_args"] {
            assert!(parse_list_default_view(&json!({"ok":false,"error":error})).is_err());
        }
        for response in [
            json!({"ok":true}),
            json!({"ok":true,"view_id":""}),
            json!({"ok":true,"view_id":null}),
            json!({"ok":true,"view_id":false}),
        ] {
            assert!(parse_list_default_view(&response).is_err());
        }
    }
    #[test]
    fn open_and_read_marking_are_distinct_nonretrying_requests_with_captured_units() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "T_TEST".into(),
            enterprise_id: None,
            user_id: "U_TEST".into(),
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "fixture-not-a-token".into(),
        };
        let open = lists_open(&client, &workspace, "F_TEST");
        assert!(!open.retry_safe());
        assert!(open.url.contains("/api/lists.open?"));
        let RequestBody::Form(fields) = open.body else {
            panic!()
        };
        assert!(fields.contains(&("list_id".into(), "F_TEST".into())));
        assert!(fields.contains(&("_x_reason".into(), "open-list".into())));
        assert!(
            !fields
                .iter()
                .any(|(k, _)| matches!(k.as_str(), "view_id" | "timestamp" | "file_id" | "cursor"))
        );
        let mark = files_mark_read(&client, &workspace, "F_TEST", 1_800_000_000_123);
        assert!(!mark.retry_safe());
        assert!(mark.url.contains("/api/files.markRead?"));
        let RequestBody::Form(fields) = mark.body else {
            panic!()
        };
        assert!(fields.contains(&("timestamp".into(), "1800000000123".into())));
        assert!(fields.contains(&("file_id".into(), "F_TEST".into())));
        assert!(fields.contains(&("_x_reason".into(), "mark-list-read".into())));
    }
    #[test]
    fn open_result_preserves_prior_seconds_and_absent_default_without_guessing() {
        for timestamp in [json!("1800000000"), json!(1800000000u64)] {
            let parsed = parse_list_open(
                &json!({"ok":true,"last_read_timestamp":timestamp,"resolved_default_view":"VIEW"}),
            )
            .unwrap();
            assert_eq!(parsed.last_read_timestamp, Some(1800000000));
            assert_eq!(parsed.resolved_default_view.as_deref(), Some("VIEW"));
        }
        for default in [Value::Null, json!("")] {
            assert_eq!(
                parse_list_open(&json!({"ok":true,"resolved_default_view":default}))
                    .unwrap()
                    .resolved_default_view,
                None
            );
        }
        for bad in [
            json!(true),
            json!(-1),
            json!(1.25),
            json!("123junk"),
            json!("18446744073709551616"),
        ] {
            assert!(parse_list_open(&json!({"ok":true,"last_read_timestamp":bad})).is_err());
        }
        for bad in [json!(true), json!([]), json!(" ")] {
            assert!(parse_list_open(&json!({"ok":true,"resolved_default_view":bad})).is_err());
        }
        assert!(parse_list_open(&json!({"ok":false})).is_err());
        assert!(parse_file_mark_read(&json!({"ok":true})).is_ok());
        assert!(parse_file_mark_read(&json!({"ok":false})).is_err());
    }
}
