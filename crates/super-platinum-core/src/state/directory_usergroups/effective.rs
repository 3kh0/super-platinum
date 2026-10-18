//! Exact JJAw merge; observed locale lower is supplied by the renderer.
use super::projection::{id, known, truth};
use super::{Error, Result};
use serde_json::{Value, json};
fn string<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match raw.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        _ => Err(Error::Shape),
    }
}
/// `locale_lower` is an observed toLocaleLowerCase of the exact host name.
/// None means the actual thunk skipped this update, not an empty replacement.
pub fn effective(
    raw: &Value,
    previous: Option<&Value>,
    locale_lower: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<Value>> {
    let identifier = id(raw)?;
    if previous.is_some_and(|p| p["id"] != identifier) {
        return Err(Error::Shape);
    }
    string(raw, "name")?;
    string(raw, "handle")?;
    if known(raw) && !truth(&raw["name"]) && !truth(&raw["handle"]) {
        return Ok(None);
    }
    let mut result = previous
        .map(|v| v.as_object().cloned().ok_or(Error::Shape))
        .transpose()?
        .unwrap_or_default();
    result.extend(raw.as_object().ok_or(Error::Shape)?.clone());
    if truth(&raw["channels"]) {
        result.insert("channel_ids".into(), raw["channels"].clone());
    }
    result.insert("user_count".into(), json!(count(raw)?));
    match raw.get("name") {
        None => {
            result.remove("_name_lc");
        }
        Some(Value::Null) => {
            result.insert("_name_lc".into(), Value::Null);
        }
        Some(Value::String(s)) => {
            result.insert(
                "_name_lc".into(),
                json!(if s.is_empty() {
                    String::new()
                } else {
                    locale_lower(s).ok_or(Error::Shape)?
                }),
            );
        }
        _ => return Err(Error::Shape),
    }
    result.insert("is_usergroup".into(), json!(true));
    let value = Value::Object(result.clone());
    // The source asks getName on u BEFORE destructuring out isUnknown.
    // A prior isNonExistent flag is not implicitly cured by a display name.
    if known(&value) && truth(&value["name"]) {
        result.insert("isNonExistent".into(), json!(false));
    }
    result.remove("isUnknown");
    Ok(Some(Value::Object(result)))
}
fn count(raw: &Value) -> Result<i64> {
    if !known(raw) {
        return Ok(0);
    }
    match raw.get("user_count") {
        None | Some(Value::Null | Value::Bool(_)) => Ok(0),
        Some(Value::Number(n)) => {
            let n = n
                .as_f64()
                .filter(|n| n.is_finite() && n.abs() <= 9_007_199_254_740_991.0)
                .ok_or(Error::Shape)?;
            // Within this exact-integer work bound JavaScript prints values
            // below 1e-6 in exponent notation, so parseInt reads its mantissa.
            if n != 0.0 && n.abs() < 1e-6 {
                let scientific = format!("{n:e}");
                let prefix = scientific.split(['.', 'e']).next().ok_or(Error::Shape)?;
                prefix.parse().map_err(|_| Error::Shape)
            } else {
                Ok(n.trunc() as i64)
            }
        }
        Some(Value::String(s)) => {
            let s=s.trim_start_matches(|c|matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'));
            let (negative, s) = if let Some(v) = s.strip_prefix('-') {
                (true, v)
            } else {
                (false, s.strip_prefix('+').unwrap_or(s))
            };
            let digits = s.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return Ok(0);
            }
            let n = s[..digits].parse::<u64>().map_err(|_| Error::Shape)?;
            if n > 9_007_199_254_740_991 {
                return Err(Error::Shape);
            }
            Ok(if negative { -(n as i64) } else { n as i64 })
        }
        _ => Err(Error::Shape),
    }
}
