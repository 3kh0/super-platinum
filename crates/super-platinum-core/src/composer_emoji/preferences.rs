//! Retained emoji presentation and usage prefs. Preserve frecency's original
//! wire string: record order is observable in equal-scoring query history.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

const KEYS: [&str; 4] = ["preferred_skin_tone", "emoji_mode", "emoji_use", "frecency"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    loaded: bool,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    raw: BTreeMap<String, Value>,
}

impl Preferences {
    pub fn loaded(&self) -> bool {
        self.loaded
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn raw(&self) -> &BTreeMap<String, Value> {
        &self.raw
    }
    pub fn frecency(&self) -> Option<&Value> {
        self.raw.get("frecency")
    }
    pub fn emoji_use(&self) -> Option<&Value> {
        self.raw.get("emoji_use")
    }

    /// Absence in a complete boot/GET replaces earlier values; a missing whole
    /// boot prefs container must not call this method.
    pub fn replace(&mut self, values: &BTreeMap<String, Value>) -> bool {
        let raw = values
            .iter()
            .filter(|(key, _)| KEYS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if self.loaded && self.raw == raw {
            return false;
        }
        self.loaded = true;
        self.raw = raw;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Partial socket changes neither erase other prefs nor establish that the
    /// full history has been loaded.
    pub fn apply_change(&mut self, name: &str, value: Value) -> bool {
        if !KEYS.contains(&name) || self.raw.get(name) == Some(&value) {
            return false;
        }
        self.raw.insert(name.to_owned(), value);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn as_text(&self) -> bool {
        self.raw.get("emoji_mode").and_then(Value::as_str) == Some("as_text")
    }

    /// H0x0's regex accepts either the string or numeric scalar 2..6, but not
    /// multi-tone strings, padded forms, arrays or booleans at this typed boundary.
    pub fn preferred_tone(&self) -> Option<&'static str> {
        let raw = self.raw.get("preferred_skin_tone")?;
        let tone = raw
            .as_str()
            .and_then(|s| match s {
                "2" => Some(2),
                "3" => Some(3),
                "4" => Some(4),
                "5" => Some(5),
                "6" => Some(6),
                _ => None,
            })
            .or_else(|| raw.as_u64());
        match tone {
            Some(2) => Some("2"),
            Some(3) => Some("3"),
            Some(4) => Some("4"),
            Some(5) => Some("5"),
            Some(6) => Some("6"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_partial_and_warm_wire_identity() {
        let mut p = Preferences::default();
        p.apply_change("preferred_skin_tone", Value::from(4));
        assert!(!p.loaded());
        assert_eq!(p.preferred_tone(), Some("4"));
        let wire =
            r#"{"z":{"id":"Ez","count":1,"visits":[1]},"a":{"id":"Ea","count":1,"visits":[1]}}"#;
        p.replace(&BTreeMap::from([
            ("frecency".into(), Value::String(wire.into())),
            ("emoji_mode".into(), Value::String("as_text".into())),
        ]));
        assert!(p.as_text());
        assert_eq!(p.preferred_tone(), None);
        let roundtrip: Preferences =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, roundtrip);
        assert_eq!(p.frecency().and_then(Value::as_str), Some(wire));
        let revision = p.revision();
        assert!(!p.apply_change("unrelated", Value::Null));
        assert_eq!(p.revision(), revision);
        p.apply_change("preferred_skin_tone", Value::String("04".into()));
        assert_eq!(p.preferred_tone(), None);
    }
}
