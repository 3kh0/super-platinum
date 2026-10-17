//! Closed ordinary People/Channel CREATION options, not cell cardinality rules.
//! Source: reviewed v2 brief manifest 77069b59b46292cd66354181e5f192ac36c91e4bb36163a438113c3c8b1d3893.
//! B EntityColumnEditor callbacks [18247162,18247779), Cs2K defaults in S
//! [4027415,4031229); exact asset/slice hashes are in candidate provenance.json.
use crate::slack_lists::ListError;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityCreateDomain {
    People,
    Channel,
}
impl EntityCreateDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::People => "user",
            Self::Channel => "channel",
        }
    }
    /// Matches existing cell entity syntax (U/W or C/G/D/Z, >=9 ASCII bytes).
    /// Directory eligibility and ownership are separate desktop responsibilities.
    pub fn valid_id(self, id: &str) -> bool {
        let bytes = id.as_bytes();
        let prefixes: &[u8] = match self {
            Self::People => b"UW",
            Self::Channel => b"CGDZ",
        };
        bytes.len() >= 9
            && prefixes.contains(&bytes[0])
            && bytes[1..]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntityCreateFormat {
    #[default]
    Single,
    Multiple,
}
impl EntityCreateFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single_entity",
            Self::Multiple => "multi_entity",
        }
    }
}
/// Empty default shapes are distinct source outputs. Selected must be nonempty;
/// capture validates rather than normalizing an ambiguous empty vector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityCreateDefaults {
    Null,
    Omitted,
    Selected(Vec<String>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityCreateOptions {
    domain: EntityCreateDomain,
    format: EntityCreateFormat,
    show_names: bool,
    defaults: EntityCreateDefaults,
}
impl EntityCreateOptions {
    /// Existing local options JSON budget, not a Slack default-selection quota.
    pub const MAX_OPTIONS_BYTES: usize = 8192;
    pub fn new(domain: EntityCreateDomain) -> Self {
        Self {
            domain,
            format: EntityCreateFormat::Single,
            show_names: true,
            defaults: EntityCreateDefaults::Null,
        }
    }
    pub fn capture(
        domain: EntityCreateDomain,
        format: EntityCreateFormat,
        show_names: bool,
        defaults: EntityCreateDefaults,
    ) -> Result<Self, ListError> {
        if domain == EntityCreateDomain::Channel && !show_names {
            return Err(ListError(
                "Channel creation does not expose name visibility.",
            ));
        }
        if let EntityCreateDefaults::Selected(ids) = &defaults {
            let mut seen = HashSet::new();
            let mut bytes = 0usize;
            if ids.is_empty() {
                return Err(ListError("Choose an explicit empty default shape."));
            }
            for id in ids {
                bytes = bytes.saturating_add(id.len());
                if bytes > Self::MAX_OPTIONS_BYTES {
                    return Err(ListError(
                        "These entity defaults exceed the supported request size.",
                    ));
                }
                if !domain.valid_id(id) || !seen.insert(id.as_str()) {
                    return Err(ListError(
                        "Choose distinct valid references for this field.",
                    ));
                }
            }
        }
        // Deliberately NO single-format cardinality refusal: source preserves
        // the whole vector through multi -> single and submits it unchanged.
        let result = Self {
            domain,
            format,
            show_names,
            defaults,
        };
        if result.wire().to_string().len() > Self::MAX_OPTIONS_BYTES {
            return Err(ListError(
                "These entity defaults exceed the supported request size.",
            ));
        }
        Ok(result)
    }
    /// Use at a byte boundary to reject oversize JSON and duplicate keys before
    /// constructing Value. from_wire alone cannot detect already-erased keys.
    pub fn decode(domain: EntityCreateDomain, bytes: &[u8]) -> Result<Self, ListError> {
        let raw = crate::slash_commands::codec::decode(bytes, Self::MAX_OPTIONS_BYTES)
            .map_err(|_| ListError("Unsupported entity creation options."))?;
        Self::from_wire(domain, &raw)
    }
    /// Exact closed authoring shape. Never repairs aliases, empty arrays, scalar
    /// order, typed order, duplicated IDs, cross-domain keys or unknown options.
    pub fn from_wire(domain: EntityCreateDomain, raw: &Value) -> Result<Self, ListError> {
        let error = || ListError("Unsupported entity creation options.");
        let map = raw.as_object().ok_or_else(error)?;
        if map.keys().any(|key| {
            !matches!(
                key.as_str(),
                "format" | "show_member_name" | "default_value" | "default_value_typed"
            )
        }) || raw.to_string().len() > Self::MAX_OPTIONS_BYTES
        {
            return Err(error());
        }
        let format = match raw.get("format").and_then(Value::as_str) {
            Some("single_entity") => EntityCreateFormat::Single,
            Some("multi_entity") => EntityCreateFormat::Multiple,
            _ => return Err(error()),
        };
        let show_names = raw
            .get("show_member_name")
            .and_then(Value::as_bool)
            .ok_or_else(error)?;
        let defaults = match (raw.get("default_value"), raw.get("default_value_typed")) {
            (None, None) => EntityCreateDefaults::Omitted,
            (Some(Value::Null), None) => EntityCreateDefaults::Null,
            (Some(Value::String(scalar)), Some(Value::Object(typed))) if typed.len() == 1 => {
                let values = typed
                    .get(domain.as_str())
                    .and_then(Value::as_array)
                    .ok_or_else(error)?;
                let ids = values
                    .iter()
                    .map(|value| value.as_str().map(str::to_owned).ok_or_else(error))
                    .collect::<Result<Vec<_>, _>>()?;
                if scalar.is_empty() || ids.join("|") != *scalar {
                    return Err(error());
                }
                EntityCreateDefaults::Selected(ids)
            }
            _ => return Err(error()),
        };
        let result = Self::capture(domain, format, show_names, defaults)?;
        if result.wire() != *raw {
            return Err(error());
        }
        Ok(result)
    }
    pub fn domain(&self) -> EntityCreateDomain {
        self.domain
    }
    pub fn format(&self) -> EntityCreateFormat {
        self.format
    }
    pub fn show_names(&self) -> bool {
        self.show_names
    }
    pub fn defaults(&self) -> &EntityCreateDefaults {
        &self.defaults
    }
    pub fn selected_ids(&self) -> &[String] {
        match &self.defaults {
            EntityCreateDefaults::Selected(ids) => ids,
            _ => &[],
        }
    }
    /// Single control display only. Does not change or truncate submission data.
    pub fn single_display_default(&self) -> Option<&str> {
        self.selected_ids().first().map(String::as_str)
    }
    /// Same Type keeps configuration; changing ordinary entity Type resets it.
    /// Transitions to/from other column types belong to the desktop Type owner.
    pub fn select_domain(&self, domain: EntityCreateDomain) -> Self {
        if self.domain == domain {
            self.clone()
        } else {
            Self::new(domain)
        }
    }
    pub fn with_format(&self, format: EntityCreateFormat) -> Result<Self, ListError> {
        Self::capture(self.domain, format, self.show_names, self.defaults.clone())
    }
    pub fn with_show_names(&self, show_names: bool) -> Result<Self, ListError> {
        Self::capture(self.domain, self.format, show_names, self.defaults.clone())
    }
    /// Actual single callback: reselecting a sole default yields null. With a
    /// retained multi default string, picking its first visible ID replaces the
    /// vector with that one ID; it does not compare only the display projection.
    pub fn select_single(&self, id: &str) -> Result<Self, ListError> {
        if self.format != EntityCreateFormat::Single || !self.domain.valid_id(id) {
            return Err(ListError("This default selection is no longer current."));
        }
        let defaults = if self.selected_ids().len() == 1 && self.selected_ids()[0] == id {
            EntityCreateDefaults::Null
        } else {
            EntityCreateDefaults::Selected(vec![id.into()])
        };
        Self::capture(self.domain, self.format, self.show_names, defaults)
    }
    /// [] is the multi callback's undefined result: omit BOTH default members.
    pub fn select_multiple(&self, ids: Vec<String>) -> Result<Self, ListError> {
        if self.format != EntityCreateFormat::Multiple {
            return Err(ListError("This default selection is no longer current."));
        }
        let defaults = if ids.is_empty() {
            EntityCreateDefaults::Omitted
        } else {
            EntityCreateDefaults::Selected(ids)
        };
        Self::capture(self.domain, self.format, self.show_names, defaults)
    }
    pub fn wire(&self) -> Value {
        let mut wire = json!({"format":self.format.as_str(),"show_member_name":self.show_names});
        match &self.defaults {
            EntityCreateDefaults::Null => wire["default_value"] = Value::Null,
            EntityCreateDefaults::Omitted => {}
            EntityCreateDefaults::Selected(ids) => {
                wire["default_value"] = Value::String(ids.join("|"));
                wire["default_value_typed"] = Value::Object(
                    [(self.domain.as_str().into(), json!(ids))]
                        .into_iter()
                        .collect(),
                );
            }
        }
        wire
    }
}
#[cfg(test)]
mod tests;
