//! File-view presence for an explicitly opened List, separate from lists.open
//! read tracking. Captured B:6906374/13676740/13677466; wire conversion LYxw.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct FileViewing {
    pub should_subscribe_and_ping: bool,
    /// Preserve the service's unmodeled viewer data without inventing a schema.
    pub viewers: Option<Value>,
}

pub fn files_open(
    c: &SlackClient,
    w: &WorkspaceSession,
    id: &str,
    last_updated_ms: u64,
) -> PreparedRequest {
    c.rest_form(
        w,
        "files.open",
        vec![
            ("file_id", id.into()),
            ("supports_read_only_check", "true".into()),
            ("last_updated", last_updated_ms.to_string()),
            ("_x_reason", "open-file".into()),
        ],
    )
}

pub fn files_ping(
    c: &SlackClient,
    w: &WorkspaceSession,
    id: &str,
    is_focused: bool,
) -> PreparedRequest {
    c.rest_form(
        w,
        "files.ping",
        vec![
            ("file_id", id.into()),
            ("is_focused", is_focused.to_string()),
            ("_x_reason", "ping-file".into()),
        ],
    )
}

pub fn files_close(
    c: &SlackClient,
    w: &WorkspaceSession,
    id: &str,
    last_updated_ms: u64,
) -> PreparedRequest {
    c.rest_form(
        w,
        "files.close",
        vec![
            ("file_id", id.into()),
            ("last_updated", last_updated_ms.to_string()),
            ("_x_reason", "close-file".into()),
        ],
    )
}

/// Missing/malformed permission is not an implicit subscription grant. Keep
/// explicit false distinct so the caller can stop an existing viewing lease.
pub fn parse_file_viewing(response: &Value) -> Result<FileViewing, Error> {
    if response.get("ok") != Some(&Value::Bool(true)) {
        return Err(Error::Api(
            "File viewing request was not successful.".into(),
        ));
    }
    let should_subscribe_and_ping = response
        .get("should_subscribe_and_ping")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            Error::Api("File viewing subscription result is missing or malformed.".into())
        })?;
    Ok(FileViewing {
        should_subscribe_and_ping,
        viewers: response.get("viewers").cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    use serde_json::json;

    #[test]
    fn viewing_requests_preserve_captured_fields_and_do_not_retry_presence_changes() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "T_VIEW".into(),
            enterprise_id: None,
            user_id: "U_VIEW".into(),
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "fixture-not-a-token".into(),
        };
        let timestamp = 1_800_000_000_123;
        let cases = [
            (
                files_open(&client, &workspace, "F_VIEW", timestamp),
                "files.open",
                json!({
                    "file_id":"F_VIEW", "supports_read_only_check":"true", "last_updated":"1800000000123", "_x_reason":"open-file"
                }),
            ),
            (
                files_ping(&client, &workspace, "F_VIEW", false),
                "files.ping",
                json!({
                    "file_id":"F_VIEW", "is_focused":"false", "_x_reason":"ping-file"
                }),
            ),
            (
                files_close(&client, &workspace, "F_VIEW", timestamp),
                "files.close",
                json!({
                    "file_id":"F_VIEW", "last_updated":"1800000000123", "_x_reason":"close-file"
                }),
            ),
        ];
        for (request, endpoint, expected) in cases {
            assert!(request.url.contains(&format!("/api/{endpoint}?")));
            assert!(!request.retry_safe());
            let RequestBody::Form(fields) = request.body else {
                panic!("expected form")
            };
            for (key, expected) in expected.as_object().unwrap() {
                assert_eq!(
                    fields
                        .iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, value)| value.as_str()),
                    expected.as_str()
                );
            }
            assert!(!fields.iter().any(|(key, _)| matches!(
                key.as_str(),
                "list_id" | "cursor" | "isFocused" | "lastUpdated"
            )));
        }
    }

    #[test]
    fn subscription_result_requires_exact_boolean_and_preserves_viewer_payload() {
        for permission in [true, false] {
            let viewers = json!({"future_viewer_shape":[{"id":"U_VIEW"}]});
            let result = parse_file_viewing(
                &json!({"ok":true,"should_subscribe_and_ping":permission,"viewers":viewers}),
            )
            .unwrap();
            assert_eq!(result.should_subscribe_and_ping, permission);
            assert_eq!(result.viewers, Some(viewers));
        }
        for response in [
            json!({"ok":false,"should_subscribe_and_ping":true}),
            json!({"ok":true}),
            json!({"ok":true,"should_subscribe_and_ping":null}),
            json!({"ok":true,"should_subscribe_and_ping":"true"}),
            json!({"ok":true,"should_subscribe_and_ping":1}),
        ] {
            assert!(parse_file_viewing(&response).is_err());
        }
        assert_eq!(
            parse_file_viewing(&json!({"ok":true,"should_subscribe_and_ping":true}))
                .unwrap()
                .viewers,
            None
        );
    }
}
