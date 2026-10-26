//! cdYm/53BY additional team restriction. This is not channel availability,
//! source freshness, membership, or the HTTP account/cache route.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelTeamPolicy {
    allowed: Option<Vec<String>>,
    match_any: bool,
}

impl ChannelTeamPolicy {
    /// The caller must resolve tpNE from owned configuration/page observations.
    /// In particular, missing mode evidence is not `false`. The service team is
    /// the actual Home context-or-installed route, not the directory account.
    pub fn new(enterprise_mode: bool, service_team_id: &str) -> Result<Self, FormError> {
        if service_team_id.is_empty()
            || service_team_id.len() > 255
            || !service_team_id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            || !matches!(service_team_id.as_bytes()[0], b'T' | b'E')
        {
            return Err(FormError::ValueTypeMismatch);
        }
        // The original enterprise test is /^E[A-Z0-9]{8,}$/; a short E ID is
        // not silently promoted to an enterprise-context exemption.
        let enterprise_context = service_team_id.starts_with('E') && service_team_id.len() >= 9;
        let match_any = enterprise_mode && !enterprise_context;
        Ok(Self {
            allowed: match_any.then(|| vec![service_team_id.to_owned()]),
            match_any,
        })
    }

    pub fn allowed_internal_teams(&self) -> Option<&[String]> {
        self.allowed.as_deref()
    }

    pub fn match_any(&self) -> bool {
        self.match_any
    }

    /// Exact additional 53BY fields; never substitute `external_team`.
    pub fn request_fields(&self) -> Value {
        let mut fields = serde_json::Map::new();
        if let Some(allowed) = &self.allowed {
            fields.insert("allowed_internal_teams".into(), json!(allowed));
        }
        if self.match_any {
            fields.insert(
                "should_allowed_internal_teams_match_any".into(),
                json!(true),
            );
        }
        Value::Object(fields)
    }

    /// Extra filter over a separately admitted current canonical public row.
    /// Typed malformed metadata refuses rather than granting a match. An empty
    /// camel-case array wins the raw alias, as in bxA+; connectedTeamIds is not
    /// an alternative to internalTeamIds.
    pub fn allows(&self, row: &Value) -> bool {
        let Some(allowed) = &self.allowed else {
            return true;
        };
        let Some(row) = row.as_object() else {
            return false;
        };
        let flag = |key: &str| match row.get(key) {
            None | Some(Value::Null) => Some(false),
            Some(Value::Bool(value)) => Some(*value),
            _ => None,
        };
        let (Some(org), Some(global), Some(im), Some(mpim)) = (
            flag("is_org_shared"),
            flag("is_global_shared"),
            flag("is_im"),
            flag("is_mpim"),
        ) else {
            return false;
        };
        if self.match_any && org && (global || im || mpim) {
            return true;
        }
        let teams = row
            .get("internalTeamIds")
            .filter(|v| !v.is_null())
            .or_else(|| row.get("internal_team_ids"));
        let Some(teams) = teams.and_then(Value::as_array) else {
            return false;
        };
        if teams.is_empty() || teams.len() > 1000 || teams.iter().any(|v| v.as_str().is_none()) {
            return false;
        }
        let contains = |id: &String| teams.iter().any(|v| v.as_str() == Some(id));
        if self.match_any {
            allowed.iter().any(contains)
        } else {
            allowed.iter().all(contains)
        }
    }
}

#[cfg(test)]
mod tests;
