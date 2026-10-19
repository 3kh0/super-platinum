//! Supported 4OY3 selector facts, separate from the accepted title-only view.
//! No caller-supplied facts, localUpdated, preview, or full File assertion.
use super::*;
use serde_json::json;

const INHERITED: &[&str] = &[
    "external_url",
    "user",
    "user_team",
    "created",
    "last_read",
    "shares",
    "groups",
    "ims",
    "comments",
    "channel_map",
    "highlight_type",
    "isTopEngagedFile",
    "is_ai_suggested",
    "pretty_type",
];

pub(super) fn project(
    incoming: &Map<String, Value>,
    previous: Option<&Value>,
    team: Option<&str>,
    channel_map_order: Option<&[String]>,
) -> Result<Value, Error> {
    // canvas_metadata is handled explicitly, including embedded file references.
    // Other conservative title/status/reset gates remain.
    let mut ordinary_input = incoming.clone();
    ordinary_input.remove("canvas_metadata");
    let previous_title = previous.map(|row| {
        let mut title = row.as_object().expect("candidate object").clone();
        title.remove("canvas_metadata");
        Value::Object(title)
    });
    let title = super::project(&ordinary_input, previous_title.as_ref())?;
    let mut output = Map::new();
    for key in FIELDS {
        if let Some(value) = title.get(*key) {
            output.insert((*key).into(), value.clone());
        }
    }
    for key in INHERITED {
        if let Some(value) = incoming
            .get(*key)
            .or_else(|| previous.and_then(|p| p.get(*key)))
        {
            output.insert((*key).into(), value.clone());
        }
    }
    let raw = Value::Object(incoming.clone());
    let absent = json!({});
    let prior = previous.unwrap_or(&absent);
    // processFile defaults on a genuinely new cache row, not made-up wire facts.
    for key in ["comments", "ims", "groups"] {
        output.entry(key).or_insert_with(|| json!([]));
    }
    output.insert("actions".into(), actions(prior, &raw)?);
    output.insert(
        "channels".into(),
        channels(prior, &raw, team, channel_map_order)?,
    );
    output.insert(
        "is_starred".into(),
        json!(truth(
            raw.get("is_starred")
                .filter(|v| !v.is_null())
                .unwrap_or(&prior["is_starred"])
        )),
    );
    let shared = if truth(&raw["channels"]) || truth(&raw["groups"]) {
        size(&raw["channels"])? > 0 || size(&raw["groups"])? > 0
    } else {
        truth(&prior["is_shared"])
    };
    output.insert("is_shared".into(), json!(shared));
    let count = if truth(&raw["comments_count"]) {
        &raw["comments_count"]
    } else if truth(&prior["comments_count"]) {
        &prior["comments_count"]
    } else {
        &Value::Null
    };
    let count = if count.is_null() {
        0.0
    } else {
        count
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(unavailable)?
    };
    let comments = if truth(&raw["comments"]) {
        &raw["comments"]
    } else {
        &prior["comments"]
    };
    let count = count.max(size(comments)? as f64);
    output.insert(
        "comments_count".into(),
        if count.fract() == 0.0 && count >= 0.0 && count < (u64::MAX as f64) {
            json!(count as u64)
        } else {
            json!(count)
        },
    );
    if truth(&raw["canvas_metadata"]) {
        let mut metadata = raw["canvas_metadata"]
            .as_object()
            .ok_or_else(unavailable)?
            .clone();
        if truth(&raw["canvas_metadata"]["items"]) {
            let items = raw["canvas_metadata"]["items"]
                .as_array()
                .ok_or_else(unavailable)?;
            let mut mapped = Vec::new();
            for item in items {
                let mut item = item.as_object().ok_or_else(unavailable)?.clone();
                if item.get("file").is_some_and(truth) {
                    let id = item["file"]["id"]
                        .as_str()
                        .filter(|id| valid_id(id))
                        .ok_or_else(unavailable)?;
                    item.insert("file".into(), json!({"id":id}));
                }
                mapped.push(Value::Object(item));
            }
            metadata.insert("items".into(), Value::Array(mapped));
        }
        output.insert("canvas_metadata".into(), Value::Object(metadata));
    }
    // Incoming metadata omission removes it; it is not shallow inherited.
    Ok(Value::Object(output))
}

fn size(value: &Value) -> Result<usize, Error> {
    match value {
        Value::Null => Ok(0),
        Value::Array(v) => Ok(v.len()),
        Value::Object(v) => Ok(v.len()),
        Value::String(v) => Ok(v.encode_utf16().count()),
        _ => Err(unavailable()),
    }
}
fn action_ts(value: &Value) -> Result<f64, Error> {
    match value {
        Value::Null => Ok(f64::NAN),
        Value::Number(n) => Ok(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => {
            // JS parseFloat decimal prefix, including a partial exponent.
            // Exotic non-string coercions remain explicitly unavailable.
            let s=s.trim_start_matches(|c:char| matches!(c,'\u{0009}'..='\u{000d}'|' '|'\u{00a0}'|'\u{1680}'
                |'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'));
            if s.starts_with("Infinity") || s.starts_with("+Infinity") {
                return Ok(f64::INFINITY);
            }
            if s.starts_with("-Infinity") {
                return Ok(f64::NEG_INFINITY);
            }
            let bytes = s.as_bytes();
            let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
            let mut digits = 0;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
                digits += 1;
            }
            if bytes.get(end) == Some(&b'.') {
                end += 1;
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                    digits += 1;
                }
            }
            if digits == 0 {
                return Ok(f64::NAN);
            }
            if matches!(bytes.get(end), Some(b'e' | b'E')) {
                let exponent = end;
                end += 1;
                if matches!(bytes.get(end), Some(b'+' | b'-')) {
                    end += 1;
                }
                let start = end;
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                if start == end {
                    end = exponent;
                }
            }
            return Ok(s[..end].parse::<f64>().unwrap_or(f64::NAN));
        }
        _ => Err(unavailable()),
    }
}
fn array_or_empty(value: &Value) -> Result<Vec<Value>, Error> {
    if !truth(value) {
        Ok(Vec::new())
    } else {
        value.as_array().cloned().ok_or_else(unavailable)
    }
}
fn actions(prior: &Value, raw: &Value) -> Result<Value, Error> {
    let incoming = array_or_empty(&raw["actions"])?;
    let previous = array_or_empty(&prior["actions"])?;
    if incoming.is_empty() {
        return Ok(Value::Array(previous));
    }
    if previous.is_empty() {
        return Ok(Value::Array(incoming));
    }
    let selected = if action_ts(&incoming[0]["ts"])? > action_ts(&previous[0]["ts"])? {
        &incoming[0]
    } else {
        &previous[0]
    };
    Ok(json!([selected]))
}
fn channels(
    prior: &Value,
    raw: &Value,
    team: Option<&str>,
    order: Option<&[String]>,
) -> Result<Value, Error> {
    if !truth(&raw["channels"]) {
        return Ok(Value::Array(array_or_empty(&prior["channels"])?));
    }
    let incoming = raw["channels"].as_array().ok_or_else(unavailable)?;
    if !truth(&raw["channel_map"]) {
        return Ok(Value::Array(incoming.clone()));
    }
    let team = team.filter(|t| !t.is_empty()).ok_or_else(unavailable)?;
    let map = raw["channel_map"].as_object().ok_or_else(unavailable)?;
    let mut result = Vec::new();
    let mut selected: Vec<(&String, &Value)> = if let Some(order) = order {
        if order.len() != map.len()
            || order
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != order.len()
        {
            return Err(unavailable());
        }
        order
            .iter()
            .map(|key| {
                map.get(key)
                    .map(|entry| (key, entry))
                    .ok_or_else(unavailable)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        map.iter().collect()
    };
    selected.retain(|(_, entry)| truth(&entry["is_org_shared"]) || entry["team"] == team);
    // JSON Value has lost string-key insertion order. Numeric JS index keys
    // have an independently established order; multiple other selected keys
    // need an original-byte sidecar before this branch can be authoritative.
    let index = |key: &str| {
        key.parse::<u32>()
            .ok()
            .filter(|i| *i != u32::MAX && i.to_string() == key)
    };
    if order.is_none() && selected.len() > 1 {
        if selected.iter().any(|(key, _)| index(key).is_none()) {
            return Err(unavailable());
        }
        selected.sort_by_key(|(key, _)| index(key).unwrap());
    }
    for (_, entry) in selected {
        if truth(&entry["is_org_shared"]) || entry["team"] == team {
            let id = entry["id"].as_str().ok_or_else(unavailable)?;
            let id = json!(id);
            if !result.contains(&id) {
                result.push(id)
            }
        }
    }
    for id in incoming {
        if !result.contains(id) {
            result.push(id.clone())
        }
    }
    Ok(Value::Array(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_prefix_is_linear_and_keeps_partial_exponent_semantics() {
        for (text, expected) in [
            ("1e+oops", 1.0),
            ("-.5e2tail", -50.0),
            ("0x20", 0.0),
            ("+2.3.4", 2.3),
        ] {
            assert_eq!(action_ts(&json!(text)).unwrap(), expected);
        }
        for text in ["+.", "e2", "--1"] {
            assert!(action_ts(&json!(text)).unwrap().is_nan());
        }
        let long = format!("1{}e+suffix", "0".repeat(200_000));
        assert_eq!(action_ts(&json!(long)).unwrap(), f64::INFINITY);
        let bad = format!("{}1", "x".repeat(200_000));
        assert!(action_ts(&json!(bad)).unwrap().is_nan());
    }
}
