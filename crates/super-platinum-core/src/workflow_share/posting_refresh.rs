//! Xhlw restricted-send recovery: metadata read only, never a message replay.
use super::*;

pub const REASON: &str = "posting_permission_send_message_error";
pub(crate) fn valid_channel(channel: &str) -> bool {
    destination(channel)
}

pub fn required(raw: Option<&Value>) -> bool {
    let Some(raw) = raw else { return false };
    if raw["ok"] == true {
        return false;
    }
    match raw.get("error") {
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| matches!(v, 6.0 | 7.0 | 8.0)),
        Some(Value::String(s)) => matches!(
            s.as_str(),
            "restricted_action_read_only_channel"
                | "restricted_action_thread_only_channel"
                | "restricted_action_non_threadable_channel"
        ),
        _ => false,
    }
}

pub fn request(
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
) -> Result<PreparedRequest, Error> {
    if !destination(channel) {
        return Err(invalid());
    }
    Ok(client.rest_form(
        session,
        "conversations.info",
        vec![("channel", channel.into()), ("_x_reason", REASON.into())],
    ))
}

pub fn channel(raw: &Value, requested: &str) -> Result<Value, Error> {
    if !destination(requested)
        || raw["ok"] != true
        || raw.get("error").is_some_and(|e| !e.is_null())
    {
        return Err(invalid());
    }
    let value = raw
        .get("channel")
        .filter(|v| v.is_object() && v["id"].as_str() == Some(requested))
        .ok_or_else(invalid)?;
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_six_typed_restricted_errors_only() {
        for code in [
            json!(6),
            json!(7.0),
            json!(8),
            json!("restricted_action_read_only_channel"),
            json!("restricted_action_thread_only_channel"),
            json!("restricted_action_non_threadable_channel"),
        ] {
            assert!(required(Some(&json!({"ok":false,"error":code}))));
            assert!(!required(Some(&json!({"ok":true,"error":code}))));
        }
        for code in [
            json!("6"),
            json!(5),
            json!(9),
            json!("not_allowed"),
            json!("dlp_user_warning"),
            json!(null),
            json!({}),
        ] {
            assert!(!required(Some(&json!({"ok":false,"error":code}))));
        }
        assert!(!required(None));
    }
    #[test]
    fn response_identity_and_opaque_flags_are_preserved() {
        let raw =
            json!({"ok":true,"channel":{"id":"CDEST0001","is_read_only":true,"opaque":[1,"kept"]}});
        assert_eq!(channel(&raw, "CDEST0001").unwrap(), raw["channel"]);
        for bad in [
            json!({"ok":true,"channel":{"id":"COTHER001"}}),
            json!({"ok":false,"channel":raw["channel"]}),
            json!({"ok":true,"error":"no","channel":raw["channel"]}),
            json!({"ok":true}),
        ] {
            assert!(channel(&bad, "CDEST0001").is_err());
        }
    }
}
