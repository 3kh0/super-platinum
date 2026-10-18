use super::*;

pub(super) fn validate(raw: &Value) -> Result<(), Error> {
    raw["profile"].as_object().ok_or_else(invalid)?;
    for name in ["is_profile_only_user", "is_stranger"] {
        if raw.get(name).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid());
        }
    }
    if raw
        .get("updated")
        .is_some_and(|v| !v.is_null() && v.as_u64().is_none())
    {
        return Err(invalid());
    }
    for name in ["name"] {
        if raw
            .get(name)
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(invalid());
        }
    }
    for name in [
        "real_name",
        "display_name",
        "email",
        "first_name",
        "last_name",
        "real_name_normalized",
        "display_name_normalized",
    ] {
        if raw["profile"]
            .get(name)
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(invalid());
        }
    }
    if raw["profile"]
        .get("fields")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn normalize(raw: &Value, enabled: bool) -> Result<Value, Error> {
    validate(raw)?;
    let mut value = raw.clone();
    if enabled && raw["is_profile_only_user"] != true {
        value["is_stranger"] = Value::Bool(false);
    }
    Ok(value)
}
pub(super) fn effective(
    raw: &Value,
    previous: Option<&Value>,
    self_id: &str,
    preserve_email: bool,
) -> Result<Value, Error> {
    let mut value = raw.clone();
    let profile = value["profile"].as_object_mut().ok_or_else(invalid)?;
    if raw["id"] == self_id
        && preserve_email
        && raw["profile"]["email"].is_null()
        && let Some(email) = previous
            .and_then(|r| r.pointer("/profile/email"))
            .filter(|v| !v.is_null())
    {
        if !email.is_string() {
            return Err(invalid());
        }
        profile.insert("email".into(), email.clone());
    }
    if raw["profile"]["fields"].is_null()
        && let Some(fields) = previous
            .and_then(|r| r.pointer("/profile/fields"))
            .and_then(Value::as_object)
            .filter(|v| !v.is_empty())
    {
        profile.insert("fields".into(), Value::Object(fields.clone()));
    }
    for name in ["first_name", "last_name"] {
        if profile.get(name).is_none_or(Value::is_null) {
            profile.insert(name.into(), Value::String(String::new()));
        }
    }
    if raw["id"] == self_id {
        value["is_self"] = Value::Bool(true);
    }
    Ok(value)
}
pub(super) fn preserve_native(
    previous: Option<&Value>,
    wire: &Value,
    effective: &Value,
) -> Result<Value, Error> {
    let mut native = previous
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let old_profile = native
        .get("profile")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    native.extend(wire.as_object().ok_or_else(invalid)?.clone());
    let mut profile = old_profile;
    profile.extend(wire["profile"].as_object().ok_or_else(invalid)?.clone());
    // Only the source's two preservation exceptions amend this native patch.
    for name in ["email", "fields"] {
        if effective["profile"].get(name) != wire["profile"].get(name)
            && let Some(value) = effective["profile"].get(name)
        {
            profile.insert(name.into(), value.clone());
        }
    }
    native.insert("profile".into(), Value::Object(profile));
    Ok(Value::Object(native))
}
fn string<'a>(raw: &'a Value, name: &str) -> &'a str {
    raw.get(name).and_then(Value::as_str).unwrap_or("")
}
pub(super) fn name_inputs(raw: &Value) -> Result<Vec<String>, Error> {
    validate(raw)?;
    Ok(std::iter::once(string(raw, "name"))
        .chain(
            [
                "first_name",
                "last_name",
                "real_name",
                "display_name",
                "email",
            ]
            .map(|k| string(&raw["profile"], k)),
        )
        .map(str::to_owned)
        .collect())
}
pub(super) fn derive(raw: &Value, lower: &BTreeMap<String, String>) -> Result<Value, Error> {
    for input in name_inputs(raw)? {
        if !lower.contains_key(&input) {
            return Err(invalid());
        }
    }
    let mut value = derive_member_names(raw, |s| lower[s].clone()).ok_or_else(invalid)?;
    let map = value.as_object_mut().ok_or_else(invalid)?;
    for (field, input) in [
        ("_name_lc", string(raw, "name")),
        ("_first_name_lc", string(&raw["profile"], "first_name")),
        ("_last_name_lc", string(&raw["profile"], "last_name")),
        ("_real_name_lc", string(&raw["profile"], "real_name")),
        ("_display_name_lc", string(&raw["profile"], "display_name")),
    ] {
        map.insert(field.into(), Value::String(lower[input].clone()));
    }
    // The same retained J3F transform without lowercase supplies profile names.
    let direct = derive_member_names(raw, str::to_owned).ok_or_else(invalid)?;
    for (field, derived) in [
        ("real_name_normalized", "_real_name_normalized_lc"),
        ("display_name_normalized", "_display_name_normalized_lc"),
    ] {
        if value["profile"][field].is_null() {
            value["profile"][field] = direct[derived].clone();
        }
    }
    Ok(value)
}
