use super::*;

/// Source display predicate only; not target identity or navigation authority.
pub fn should_show_file(raw: &Value) -> bool {
    if !raw.is_object() {
        return false;
    }
    if [
        "isNonExistent",
        "is_deleted",
        "is_tombstoned",
        "is_hidden_by_limit",
    ]
    .iter()
    .any(|key| raw.get(*key).is_some_and(truth))
    {
        return false;
    }
    if raw.get("restriction_type").and_then(Value::as_f64) == Some(1.0) {
        return false;
    }
    if matches!(
        raw.get("mode").and_then(Value::as_str),
        Some(
            "tombstone"
                | "not_found"
                | "hidden_by_limit"
                | "access_denied"
                | "canvas_slack_connect_blocked"
                | "lists_slack_connect_blocked"
                | "not_visible"
                | "template_not_visible"
        )
    ) {
        return false;
    }
    !matches!(
        raw.get("file_access").and_then(Value::as_str),
        Some(
            "file_not_found"
                | "access_denied"
                | "not_visible"
                | "template_not_visible"
                | "canvas_globally_disabled"
                | "slack_connect_team_blocked"
                | "slack_connect_canvas_sharing_blocked"
                | "canvas_disabled_user_team"
                | "canvas_disabled_file_team"
                | "slack_connect_lists_team_blocked"
                | "slack_connect_lists_sharing_blocked"
        )
    )
}
fn strict_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        // Separate original JSON objects do not share JS reference identity.
        _ => false,
    }
}
pub(super) fn project(previous: Option<&Value>, raw: &Value) -> Value {
    if !should_show_file(raw) {
        return Value::Object(facts(raw));
    }
    let mut prior = previous.map(facts).unwrap_or_default();
    let tombstone = raw.get("is_tombstoned").is_some_and(truth);
    if tombstone != prior.get("is_tombstoned").is_some_and(truth) {
        prior.clear();
    }
    prior.insert("id".into(), raw["id"].clone());
    prior.insert("is_tombstoned".into(), Value::Bool(tombstone));
    let zero = Value::from(0);
    let incoming = raw
        .get("restriction_type")
        .filter(|v| !v.is_null())
        .unwrap_or(&zero);
    let old = prior
        .get("restriction_type")
        .filter(|v| !v.is_null())
        .unwrap_or(&zero);
    if !strict_equal(incoming, old) {
        prior.clear();
        prior.insert("id".into(), raw["id"].clone());
        if let Some(value) = raw.get("restriction_type") {
            prior.insert("restriction_type".into(), value.clone());
        }
    }
    prior.extend(facts(raw));
    // Source explicitly writes incoming.permalink, even if it is undefined.
    if !raw.as_object().unwrap().contains_key("permalink") {
        prior.remove("permalink");
    }
    if ["updated", "created", "timestamp"]
        .iter()
        .any(|key| raw.get(*key).is_some_and(truth))
    {
        prior.insert("isUnknown".into(), Value::Bool(false));
        prior.insert("isNonExistent".into(), Value::Bool(false));
    }
    Value::Object(prior)
}
pub(super) fn link(row: &Value) -> Option<Link<'_>> {
    if !should_show_file(row) {
        return None;
    }
    Some(Link {
        name: row.get("name")?.as_str()?,
        permalink: row
            .get("permalink")?
            .as_str()
            .filter(|url| !url.is_empty())?,
    })
}
