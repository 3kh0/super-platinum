//! File-only selector observations for the shared 4ucY/Ejs0 sorter.
//! Callers supply effective cache rows; this does not establish cache ownership.
use super::{
    Context, Error,
    features::{parse_float, truth},
};
use serde_json::{Value, json};

fn missing(name: &str) -> Error {
    Error::MissingObservation(format!("file {name}"))
}

/// Original 8kfn.$ reads only the first action and JS parseFloat.
pub fn latest_action(raw: &Value) -> Result<Value, Error> {
    if !raw.is_object() {
        return Err(Error::InvalidInput);
    }
    let first = match raw.get("actions") {
        None | Some(Value::Null) => None,
        Some(Value::Array(rows)) => rows.first(),
        _ => return Err(Error::InvalidInput),
    };
    let Some(first) = first.filter(|v| truth(v)) else {
        return Ok(json!(0));
    };
    if first["ts"].is_array() || first["ts"].is_object() {
        return Err(Error::InvalidInput);
    }
    let n = parse_float(&first["ts"]);
    if n.is_nan() {
        Ok(json!(0))
    } else {
        serde_json::Number::from_f64(n)
            .map(Value::Number)
            .ok_or(Error::NonFiniteScore)
    }
}

/// Discard all remote `facts`, including spoofed category/name/action selectors.
/// Explicit observed maps distinguish unavailable inputs from genuine absence.
pub fn item(raw: &Value, context: &Context<'_>) -> Result<Value, Error> {
    let id = raw["id"].as_str().ok_or(Error::InvalidInput)?;
    if !(2..=255).contains(&id.len())
        || !id.starts_with('F')
        || !id.bytes().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(Error::InvalidInput);
    }
    let mut item = raw.as_object().ok_or(Error::InvalidInput)?.clone();
    for key in [
        "name",
        "title",
        "user",
        "filetype",
        "highlight_type",
        "external_url",
        "permalink",
    ] {
        if raw.get(key).is_some_and(|v| !v.is_null() && !v.is_string()) {
            return Err(Error::InvalidInput);
        }
    }
    for key in ["channels", "groups"] {
        if let Some(value) = raw.get(key).filter(|v| !v.is_null()) {
            let ids = value.as_array().ok_or(Error::InvalidInput)?;
            if ids.iter().any(|v| v.as_str().is_none_or(str::is_empty)) {
                return Err(Error::InvalidInput);
            }
        }
    }
    let s = context.state;
    if !context.now_ms.is_finite() {
        return Err(Error::InvalidClock);
    }
    if s["selfId"].as_str().is_none_or(str::is_empty)
        || !s["ims"].is_object()
        || !s["experiments"].is_object()
        || s["experiments"]
            .get("autocomplete_affinity_ml_model")
            .is_none()
        || s["experiments"]
            .get("autocomplete_affinity_score")
            .is_none()
        || s.get("offline").is_none()
        || s.get("teamId").is_none()
    {
        return Err(missing("selector state"));
    }
    if truth(&s["teamId"]) && truth(&s["offline"]) {
        if !s["offline"].is_object()
            || s.get("remoteDm")
                .is_none_or(|v| !v.is_null() && !v.is_object())
        {
            return Err(missing("offline/remote IM observations"));
        }
        for kind in ["member", "channel", "mpim", "file"] {
            if s["entityCtr"][kind].as_f64().is_none_or(|n| !n.is_finite()) {
                return Err(missing("entity CTR"));
            }
            if kind != "file" && !s["offline"][kind].is_object() {
                return Err(missing("per-ID CTR collection"));
            }
        }
        for kind in ["member", "channel", "mpim"] {
            for features in s["offline"][kind].as_object().unwrap().values() {
                if features["user_ctr_30dd"]
                    .as_f64()
                    .is_none_or(|n| !n.is_finite())
                {
                    return Err(missing("per-ID CTR"));
                }
            }
        }
    }
    item.insert(
        "facts".into(),
        json!({"file":true,"latestAction":latest_action(raw)?}),
    );
    Ok(Value::Object(item))
}

#[cfg(test)]
mod tests;
