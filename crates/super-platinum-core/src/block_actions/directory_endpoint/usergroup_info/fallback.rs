use super::*;
use std::collections::BTreeMap;

/// D8Wa failed-ID fallback; not a general group-list request.
#[derive(Clone, PartialEq, Eq)]
pub struct Fallback {
    ids: Vec<String>,
    reason: String,
}
impl Fallback {
    pub fn capture(ids: Vec<String>, reason: &str) -> Result<Self, Error> {
        if !valid_ids(&ids, 250) || !valid_reason(reason) {
            return Err(unavailable());
        }
        Ok(Self {
            ids,
            reason: format!("fallback:{reason}"),
        })
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn request(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        super::super::scope(owner)?;
        if owner.token.is_empty() {
            return Err(unavailable());
        }
        Ok(client.rest_form(
            owner,
            "subteams.info",
            vec![
                ("include_users", "false".into()),
                ("include_count", "true".into()),
                ("include_disabled", "true".into()),
                ("usergroups", self.ids.join(",")),
                ("_x_reason", self.reason.clone()),
            ],
        ))
    }
}
fn valid_reason(reason: &str) -> bool {
    !reason.is_empty()
        && reason.len() <= 128
        && !reason.chars().any(|c| c.is_control() || c.is_whitespace())
}
pub(crate) fn valid_fallback_fields(fields: &[(String, String)]) -> bool {
    let map: BTreeMap<_, _> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    if map.len() != fields.len()
        || map.len() != 6
        || !map.get("token").is_some_and(|s| !s.is_empty())
        || map.get("include_users") != Some(&"false")
        || map.get("include_count") != Some(&"true")
        || map.get("include_disabled") != Some(&"true")
    {
        return false;
    }
    let Some(reason) = map
        .get("_x_reason")
        .and_then(|s| s.strip_prefix("fallback:"))
    else {
        return false;
    };
    let Some(ids) = map.get("usergroups") else {
        return false;
    };
    valid_reason(reason) && valid_ids(&ids.split(',').map(str::to_owned).collect::<Vec<_>>(), 250)
}

pub struct FallbackPage {
    request: Fallback,
    raw: Value,
    rows: Vec<Value>,
}
impl FallbackPage {
    pub fn decode(request: &Fallback, raw: Value) -> Result<Self, Error> {
        positive(&raw)?;
        let rows = raw["subteams"].as_array().ok_or_else(unavailable)?.clone();
        requested_rows(&rows, &request.ids)?;
        Ok(Self {
            request: request.clone(),
            raw,
            rows,
        })
    }
    pub fn request(&self) -> &Fallback {
        &self.request
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
