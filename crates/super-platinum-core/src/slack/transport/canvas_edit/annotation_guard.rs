//! Exact bounded shape emitted by the existing REST client and Qpc builder.
use serde_json::Value;
pub(super) fn query(url: &url::Url) -> bool {
    // SlackClient::rest_form always emits these ten XParams. Rejecting all query
    // strings would reject every ordinary built request before dispatch.
    let mut keys = std::collections::HashSet::new();
    for (key, value) in url.query_pairs() {
        if !keys.insert(key.to_string()) {
            return false;
        }
        let good = match key.as_ref() {
            "_x_id" | "_x_csid" | "fp" => {
                value.len() == 32
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }
            "_x_desktop_ia" | "_x_foreground" => value == "true",
            "_x_num_retries" => value == "0",
            "slack_route" => value == "default",
            "_x_frontend_build_type" | "_x_gantry" | "_x_version_ts" => {
                !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
            }
            _ => false,
        };
        if !good {
            return false;
        }
    }
    keys.len() == 10
}
fn id(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
}
pub(super) fn snippet(value: &Value, control: &str) -> bool {
    if control.len() != 35
        || !control.starts_with("temp:C:")
        || !control.as_bytes()[7..10]
            .iter()
            .all(u8::is_ascii_alphanumeric)
        || !control.as_bytes()[10..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return false;
    }
    let Some(map) = value.as_object() else {
        return false;
    };
    if map.keys().any(|k| {
        !matches!(
            k.as_str(),
            "mrkdwn"
                | "rich_text"
                | "section_users"
                | "is_archived"
                | "control_id"
                | "created_usec"
                | "last_edited_usec"
        )
    }) || map.get("control_id").and_then(Value::as_str) != Some(control)
        || map.get("is_archived") != Some(&Value::Bool(false))
        || map.get("mrkdwn").and_then(Value::as_str).is_none()
    {
        return false;
    }
    for key in ["created_usec", "last_edited_usec"] {
        if map
            .get(key)
            .is_some_and(|v| v.as_u64().is_none_or(|n| n > 9_007_199_254_740_991))
        {
            return false;
        }
    }
    let Some(rich) = map
        .get("rich_text")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
    else {
        return false;
    };
    if rich.as_object().is_none_or(|o| o.len() != 1)
        || rich
            .get("blocks")
            .and_then(Value::as_array)
            .is_none_or(|rows| {
                rows.len() != 1 || rows[0].get("type").and_then(Value::as_str) != Some("rich_text")
            })
    {
        return false;
    }
    let Some(users) = map.get("section_users").and_then(Value::as_array) else {
        return false;
    };
    !users.is_empty()
        && users.len() <= 257
        && users.iter().all(|v| {
            let Some(user) = v.as_object() else {
                return false;
            };
            !user.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "quip_user_id"
                        | "slack_user_id"
                        | "slack_team_id"
                        | "is_author"
                        | "is_recent_editor"
                        | "is_most_recent_editor"
                        | "is_mentioned"
                )
            }) && id(user.get("quip_user_id"))
                && ["slack_user_id", "slack_team_id"]
                    .iter()
                    .all(|k| !user.contains_key(*k) || id(user.get(*k)))
                && [
                    "is_author",
                    "is_recent_editor",
                    "is_most_recent_editor",
                    "is_mentioned",
                ]
                .iter()
                .all(|k| user.get(*k).is_some_and(Value::is_boolean))
        })
}
#[cfg(test)]
mod tests;
