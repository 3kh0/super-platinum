use super::{Error, response};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
static NULL: Value = Value::Null;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Entity {
    Member,
    Channel,
    Mpim,
    Im,
}
impl Entity {
    pub fn key(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Channel => "channel",
            Self::Mpim => "mpim",
            Self::Im => "ims",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Member => "user",
            Self::Channel => "channel",
            Self::Mpim => "mpim",
            Self::Im => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Features {
    pub team_ctr_90dd: f64,
    pub user_ctr_30dd: f64,
}

/// A completed zbK1 observation, including its source-defined cold defaults.
/// This value never means that the owning account/selector snapshot is available.
#[derive(Clone, Debug)]
pub struct Offline {
    other: f64,
    ctr: BTreeMap<Entity, f64>,
    features: BTreeMap<Entity, BTreeMap<String, Features>>,
    dm: Option<Map<String, Value>>,
    mpdm: Option<Map<String, Value>>,
    affinity: BTreeMap<String, f64>,
    priority: BTreeMap<String, f64>,
    // None means the numeric scores were decoded without original property
    // order. Such a snapshot still supplies scores, but cannot supply Texty's
    // stable-tie top-channel request hint.
    processed_channels: Option<Vec<String>>,
}

impl Default for Offline {
    fn default() -> Self {
        Self {
            other: 0.179,
            ctr: BTreeMap::from([
                (Entity::Member, 0.132),
                (Entity::Channel, 0.148),
                (Entity::Mpim, 0.014),
            ]),
            features: BTreeMap::new(),
            dm: None,
            mpdm: None,
            affinity: BTreeMap::new(),
            priority: BTreeMap::new(),
            processed_channels: Some(Vec::new()),
        }
    }
}

fn numeric(v: &Value) -> Result<f64, Error> {
    v.as_f64().filter(|v| v.is_finite()).ok_or(Error::Offline)
}
fn optional_number(v: &Value, fallback: f64) -> Result<f64, Error> {
    if v.is_null() {
        Ok(fallback)
    } else {
        numeric(v)
    }
}
fn names(v: &Value) -> Result<Option<Vec<&str>>, Error> {
    if v.is_null() {
        return Ok(None);
    }
    v.as_array()
        .ok_or(Error::Offline)?
        .iter()
        .map(|v| v.as_str().ok_or(Error::Offline))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}
fn metadata(v: &Value) -> Result<Option<Map<String, Value>>, Error> {
    if v.is_null() {
        return Ok(None);
    }
    let object = v.as_object().ok_or(Error::Offline)?;
    for value in object.values() {
        if value.is_null() {
            continue;
        }
        value.as_object().ok_or(Error::Offline)?;
        for key in ["last_read_abs", "membership_count"] {
            if let Some(v) = value.get(key).filter(|v| !v.is_null()) {
                if !v.is_number() && !v.is_string() {
                    return Err(Error::Offline);
                }
            }
        }
    }
    Ok(Some(object.clone()))
}
fn scores(v: &Value) -> Result<BTreeMap<String, f64>, Error> {
    if v.is_null() {
        return Ok(BTreeMap::new());
    }
    v.as_object()
        .ok_or(Error::Offline)?
        .iter()
        .map(|(k, v)| Ok((k.clone(), optional_number(v, 0.0)?)))
        .collect()
}

impl Offline {
    pub fn decode(value: &Value) -> Result<Self, Error> {
        response(value)?;
        let mut out = Self::default();
        out.other = optional_number(&value["other_ctr"], out.other)?;
        for entity in [Entity::Member, Entity::Channel, Entity::Mpim] {
            let key = format!("{}_ctr", entity.prefix());
            let ctr = optional_number(&value[&key], out.ctr[&entity])?;
            out.ctr.insert(entity, ctr);
        }
        let defaults = names(&value["default_features"])?;
        for entity in [Entity::Member, Entity::Channel, Entity::Mpim] {
            let own = names(&value[format!("{}_features", entity.prefix())])?;
            let names = own.as_ref().or(defaults.as_ref());
            let top_key = match entity {
                Entity::Member => "top_users",
                Entity::Channel => "top_channels",
                _ => "top_mpims",
            };
            let top = &value[top_key];
            if top.is_null() {
                continue;
            }
            let rows = top.as_object().ok_or(Error::Offline)?;
            for (id, row) in rows {
                if row.is_null() {
                    continue;
                }
                let values = row.as_array().ok_or(Error::Offline)?;
                let Some(names) = names else {
                    continue;
                };
                let mut features = out.default_features(entity);
                // Duplicate feature names use the last index, as the source object assignment does.
                for (index, name) in names.iter().enumerate() {
                    if !matches!(*name, "team_ctr_90dd" | "user_ctr_30dd") {
                        continue;
                    }
                    let v = values.get(index).unwrap_or(&Value::Null);
                    let number = optional_number(v, 0.0)?;
                    let number = if number == 0.0 {
                        out.entity_ctr(entity)
                    } else {
                        number
                    };
                    if *name == "team_ctr_90dd" {
                        features.team_ctr_90dd = number;
                    } else {
                        features.user_ctr_30dd = number;
                    }
                }
                out.features
                    .entry(entity)
                    .or_default()
                    .insert(id.clone(), features);
            }
        }
        out.dm = metadata(&value["users_has_dmed"])?;
        out.mpdm = metadata(&value["users_has_mpdmed"])?;
        out.affinity = scores(&value["user_affinity"])?;
        out.priority = scores(&value["channel_priority_90d"])?;
        if !out.priority.is_empty() {
            out.processed_channels = None;
        }
        Ok(out)
    }
    /// Same-response Object.keys order, supplied by the transport decoder.
    /// zbK1 sorts descending, keeps source order for ties and takes fifty IDs.
    pub fn decode_ordered(value: &Value, keys: Option<&[String]>) -> Result<Self, Error> {
        let mut out = Self::decode(value)?;
        let Some(raw) = value.get("channel_priority_90d").filter(|v| !v.is_null()) else {
            return if keys.is_none() {
                Ok(out)
            } else {
                Err(Error::Offline)
            };
        };
        let raw = raw.as_object().ok_or(Error::Offline)?;
        let keys = keys.ok_or(Error::Offline)?;
        let unique = keys.iter().collect::<std::collections::BTreeSet<_>>();
        if unique.len() != keys.len() || unique != raw.keys().collect() {
            return Err(Error::Offline);
        }
        let mut ordered = keys.to_vec();
        ordered.sort_by(|a, b| out.priority[b].partial_cmp(&out.priority[a]).unwrap());
        ordered.truncate(50);
        out.processed_channels = Some(ordered);
        Ok(out)
    }
    /// An empty cold observation is source-defined, not a claim that this
    /// account's cache or query is current. Those guards belong to the caller.
    pub fn processed_top_channels(&self) -> Result<&[String], Error> {
        self.processed_channels.as_deref().ok_or(Error::Offline)
    }
    pub fn entity_ctr(&self, entity: Entity) -> f64 {
        self.ctr
            .get(&entity)
            .copied()
            .filter(|v| *v != 0.0)
            .unwrap_or(self.other)
    }
    fn default_features(&self, entity: Entity) -> Features {
        let ctr = self.entity_ctr(entity);
        Features {
            team_ctr_90dd: ctr,
            user_ctr_30dd: ctr,
        }
    }
    /// zbK1 has no IM feature table; raw IM only receives its other-entity CTR.
    pub fn features(&self, id: &str, entity: Entity) -> Option<Features> {
        if entity == Entity::Im {
            return None;
        }
        Some(
            self.features
                .get(&entity)
                .and_then(|m| m.get(id))
                .copied()
                .unwrap_or_else(|| self.default_features(entity)),
        )
    }
    /// Some(null): source map absent/null. None: present map, missing key (undefined).
    pub fn dm(&self, id: &str) -> Option<&Value> {
        self.dm.as_ref().map_or(Some(&NULL), |m| m.get(id))
    }
    pub fn mpdm(&self, id: &str) -> Option<&Value> {
        self.mpdm.as_ref().map_or(Some(&NULL), |m| m.get(id))
    }
    pub fn affinity(&self, id: &str) -> f64 {
        self.affinity.get(id).copied().unwrap_or(0.0)
    }
    pub fn priority(&self, id: &str) -> f64 {
        self.priority.get(id).copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod priority_tests;
