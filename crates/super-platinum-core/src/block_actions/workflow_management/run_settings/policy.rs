use super::{Error, Value, strings};
fn list(raw: &Value, key: &str) -> Result<Vec<String>, Error> {
    if raw.get(key).is_some_and(Value::is_null) {
        Ok(vec![])
    } else {
        strings(raw, key)
    }
}

/// Owned original observations. No observation is distinct from an acquired
/// envelope whose trippable preference payload is absent or null.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetPolicy {
    prefs: Option<Value>,
    restriction: Option<(bool, bool)>,
}
impl TargetPolicy {
    pub fn unobserved() -> Self {
        Self {
            prefs: None,
            restriction: None,
        }
    }
    pub fn from_acquired(prefs: &Value, restriction: Option<&Value>) -> Result<Self, Error> {
        fn success(raw: &Value) -> bool {
            raw.is_object() && raw["ok"] == true && raw.get("error").is_none_or(Value::is_null)
        }
        if !success(prefs) {
            return Err(Error::Invalid);
        }
        let payload = prefs
            .get("trippable_by_slack_connected_teams")
            .cloned()
            .unwrap_or(Value::Null);
        if !payload.is_null() && !payload.is_object() {
            return Err(Error::Invalid);
        }
        if let Some(mode) = payload.get("hermes_triggers_trippable_by_slack_connected_teams_mcp") {
            if !mode.is_null() && !mode.is_string() {
                return Err(Error::Invalid);
            }
        }
        for key in [
            "allowed_teams",
            "restricted_teams",
            "allowed_teams_with_3p_connectors",
        ] {
            list(&payload, key)?;
        }
        let restriction = restriction
            .map(|raw| {
                if !success(raw) {
                    return Err(Error::Invalid);
                }
                Ok((
                    raw["is_restricted"].as_bool().ok_or(Error::Invalid)?,
                    raw["restricted_by_connector_steps_only"]
                        .as_bool()
                        .ok_or(Error::Invalid)?,
                ))
            })
            .transpose()?;
        Ok(Self {
            prefs: Some(payload),
            restriction,
        })
    }
    pub fn any_external(&self) -> Result<bool, Error> {
        let prefs = self.prefs.as_ref().ok_or(Error::MissingPolicy)?;
        let mode = prefs["hermes_triggers_trippable_by_slack_connected_teams_mcp"]
            .as_str()
            .unwrap_or("");
        Ok(!mode.is_empty() && mode != "NONE"
            || !list(prefs, "allowed_teams")?.is_empty()
            || !list(prefs, "allowed_teams_with_3p_connectors")?.is_empty())
    }
    pub fn external_checkbox_enabled(&self) -> Result<bool, Error> {
        let prefs = self.prefs.as_ref().ok_or(Error::MissingPolicy)?;
        let (restricted, connectors) = self.restriction.ok_or(Error::MissingPolicy)?;
        let mode = prefs["hermes_triggers_trippable_by_slack_connected_teams_mcp"].as_str();
        Ok(!restricted
            || connectors
                && (mode == Some("SAFE_AND_3P")
                    || !list(prefs, "allowed_teams_with_3p_connectors")?.is_empty()))
    }
    pub fn allows_external(&self, id: &str) -> Result<bool, Error> {
        let prefs = self.prefs.as_ref().ok_or(Error::MissingPolicy)?;
        // Native authority requires the actual restriction read. The reference
        // UI has a nullish-false fallback; this is an explicit stricter boundary.
        let (restricted, connectors) = self.restriction.ok_or(Error::MissingPolicy)?;
        if prefs.is_null() {
            return Ok(true);
        }
        if restricted && !connectors {
            return Ok(false);
        }
        let mode = prefs["hermes_triggers_trippable_by_slack_connected_teams_mcp"].as_str();
        let contains =
            |key: &str| -> Result<bool, Error> { Ok(list(prefs, key)?.iter().any(|v| v == id)) };
        let required_contains = |key: &str| -> Result<bool, Error> {
            if prefs.get(key).is_none_or(Value::is_null) {
                return Err(Error::Invalid);
            }
            contains(key)
        };
        if restricted {
            match mode {
                Some("NONE" | "SAFE") => contains("allowed_teams_with_3p_connectors"),
                Some("SAFE_AND_3P") => {
                    Ok(!contains("allowed_teams")? && !contains("restricted_teams")?)
                }
                _ => Ok(true),
            }
        } else {
            match mode {
                Some("NONE") => Ok(required_contains("allowed_teams")?
                    || contains("allowed_teams_with_3p_connectors")?),
                Some("SAFE" | "SAFE_AND_3P") => Ok(!required_contains("restricted_teams")?),
                _ => Ok(true),
            }
        }
    }
}
