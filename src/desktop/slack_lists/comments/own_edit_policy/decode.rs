use super::*;
fn success(raw: &Value) -> Result<(), Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    Ok(())
}
pub(super) fn channel(raw: &Value, id: &str) -> Result<Value, Error> {
    success(raw)?;
    let rows = raw["channels"].as_array().ok_or_else(unavailable)?;
    // The request names exactly one channel; no unrelated response can grant it.
    if rows.len() != 1 || rows[0]["id"] != id {
        return Err(unavailable());
    }
    Ok(rows[0].clone())
}
pub(super) fn mode(user: &Value, assignments: &Value) -> Result<(bool, bool), Error> {
    success(user)?;
    success(assignments)?;
    // Supported observed boolean preference only. Missing is never a fabricated
    // false value, even though the web caller can coerce undefined.
    let plain = user["prefs"]["plain_text_mode"]
        .as_bool()
        .ok_or_else(unavailable)?;
    let group = assignments["assignments"]["portable_content_composer"]["group"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(unavailable)?;
    // Exact retained abzA.Wt selector, not generic on/off inference.
    Ok((plain, group == "treatment"))
}
pub(super) fn windows(
    raw: &Value,
    top_level: &str,
    relevant: &[String],
) -> Result<Vec<i64>, Error> {
    success(raw)?;
    if top_level.is_empty() || relevant.is_empty() || relevant.iter().any(|s| s.is_empty()) {
        return Err(unavailable());
    }
    if raw.get("failed_extras").is_some_and(|v| {
        v.as_array()
            .is_none_or(|a| a.iter().any(|v| v == "team_prefs"))
    }) {
        return Err(unavailable());
    }
    let prefs = raw["team_prefs"].as_object().ok_or_else(unavailable)?;
    let workspaces = match prefs.get("workspace_prefs") {
        None => None,
        Some(Value::Object(v)) => Some(v),
        _ => return Err(unavailable()),
    };
    relevant
        .iter()
        .map(|team| {
            // RgmF top-level publication precedes per-workspace publication.
            let value = workspaces
                .and_then(|w| w.get(team))
                .or_else(|| {
                    (team == top_level)
                        .then(|| prefs.get("toplevel_prefs"))
                        .flatten()
                })
                .ok_or_else(unavailable)?;
            value
                .get("msg_edit_window_mins")
                .and_then(Value::as_i64)
                .filter(|n| *n >= -1 && *n <= 9_007_199_254_740_991)
                .ok_or_else(unavailable)
        })
        .collect()
}
