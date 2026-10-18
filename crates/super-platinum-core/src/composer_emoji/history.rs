//! Ordered Slack `frecency` preference, tyx1 scoring and cGzj usage updates.
//! No IO or implicit clock. The account/catalog owner supplies the snapshot/time.
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct History {
    entries: Vec<(String, Value)>,
}

// Private: a public Deserialize implementation would let from_value::<History>
// silently reconstruct already-lost object order. Only raw wire parsing may use it.
struct WireHistory(History);

impl<'de> Deserialize<'de> for WireHistory {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Ordered;
        impl<'de> Visitor<'de> for Ordered {
            type Value = WireHistory;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an ordered frecency object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<WireHistory, M::Error> {
                let mut result = History::default();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    result.set(key, value);
                }
                result.enumerate_js_keys();
                result.validate().map_err(serde::de::Error::custom)?;
                Ok(WireHistory(result))
            }
        }
        d.deserialize_map(Ordered)
    }
}

impl Serialize for History {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl History {
    /// Parse before an unordered Value map destroys source tie order.
    /// History deliberately does not implement Deserialize: an unordered generic
    /// Value must not bypass this raw-wire boundary or from_pref's refusal.
    /// ```compile_fail
    /// use super_platinum_core::composer_emoji::history::History;
    /// let unordered = serde_json::json!({});
    /// let _: History = serde_json::from_value(unordered).unwrap();
    /// ```
    pub fn from_wire(wire: &str) -> Result<Self, String> {
        serde_json::from_str::<WireHistory>(wire)
            .map(|wire| wire.0)
            .map_err(|e| e.to_string())
    }

    /// Slack's encoded preference string retains object insertion order.
    /// An already decoded Value object cannot prove that order: use from_wire
    /// at the raw boundary or from_entries when an ordered owner already has it.
    pub fn from_pref(value: &Value) -> Result<Self, String> {
        match value {
            Value::String(wire) => Self::from_wire(wire),
            _ => Err("frecency requires its ordered wire object or encoded string".into()),
        }
    }

    pub fn from_entries(entries: Vec<(String, Value)>) -> Result<Self, String> {
        let mut result = Self::default();
        for (key, value) in entries {
            result.set(key, value);
        }
        result.enumerate_js_keys();
        result.validate()?;
        Ok(result)
    }

    pub fn to_wire(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }

    fn validate(&self) -> Result<(), String> {
        for (_, value) in &self.entries {
            for row in records(value) {
                if !row.is_object()
                    || row["id"].as_str().is_none()
                    // Source uses JS truthiness. Only its boolean schema is
                    // admitted here; non-boolean flags are explicitly unsupported.
                    || row.get("_reduced").is_some_and(|v| !v.is_boolean())
                    || row["count"]
                        .as_f64()
                        .is_none_or(|v| !v.is_finite() || v < 0.0)
                    || row.get("visits").is_some_and(|v| {
                        v.as_array().is_none_or(|values| {
                            values
                                .iter()
                                .any(|v| v.as_f64().is_none_or(|v| !v.is_finite()))
                        })
                    })
                {
                    return Err("malformed frecency record".into());
                }
            }
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn set(&mut self, key: String, value: Value) {
        if let Some((_, v)) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            *v = value;
        } else {
            self.entries.push((key, value));
        }
    }

    fn enumerate_js_keys(&mut self) {
        fn index(key: &str) -> Option<u32> {
            key.parse::<u32>()
                .ok()
                .filter(|n| *n != u32::MAX && n.to_string() == key)
        }
        self.entries
            .sort_by(|(a, _), (b, _)| match (index(a), index(b)) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
    }

    /// Complete tyx1 per-item scoring. Ordering is significant for equal-score
    /// duplicate IDs in matching query buckets; exact-query doubling is later.
    pub fn score(&self, query: &str, ids: &[&str], now_ms: f64) -> Vec<Stats> {
        self.score_normalized(&format_query(query), ids, now_ms)
    }

    /// Score an already observed hNDQ/Ojx4 query without a second locale pass.
    /// The owning caller must bind normalization to the same query/account.
    pub fn score_normalized(&self, normalized: &str, ids: &[&str], now_ms: f64) -> Vec<Stats> {
        let mut hits: BTreeMap<&str, (&Value, bool)> = BTreeMap::new();
        for (key, value) in &self.entries {
            if !key.contains(normalized) && !normalized.contains(key) {
                continue;
            }
            for row in records(value) {
                let Some(id) = row["id"].as_str() else {
                    continue;
                };
                let replace = hits.get(id).is_none_or(|(prior, _)| {
                    weight(Some(row), now_ms) * count(Some(row))
                        > weight(Some(prior), now_ms) * count(Some(prior))
                });
                if replace {
                    hits.insert(id, (row, key == normalized));
                }
            }
        }
        let mut result: Vec<_> = ids
            .iter()
            .map(|id| {
                let hit = hits.get(id);
                let h = hit.map(|(v, _)| *v);
                let item = self.get(id);
                let hc = count(h);
                let ic = count(item);
                let hw = weight(h, now_ms);
                let iw = weight(item, now_ms);
                let latest_hit = latest(h);
                let latest_item = latest(item);
                let last_ms = latest_hit.max(latest_item) * 1000.0;
                let raw = (if hit.is_some_and(|(_, exact)| *exact) {
                    2.0 * hc * hw
                } else {
                    hc * hw
                }) + ic * iw;
                Stats {
                    raw: raw
                        * if last_ms != 0.0 {
                            0.9755_f64.powf(((now_ms - last_ms) / 86_400_000.0).floor())
                        } else {
                            1.0
                        },
                    hit_count: hc,
                    item_count: ic,
                    hit_weight: hw,
                    item_weight: iw,
                    log_hit_scaled: hc.ln_1p(),
                    log_item_scaled: ic.ln_1p(),
                    latest_hit,
                    latest_item,
                    hit_buckets: buckets(h, now_ms),
                    item_buckets: buckets(item, now_ms),
                }
            })
            .collect();
        let max = |f: fn(&Stats) -> f64| {
            result
                .iter()
                .map(f)
                .filter(|x| !x.is_nan())
                .reduce(f64::max)
                .unwrap_or(0.0)
        };
        let raw = max(|s| s.raw);
        let hit = max(|s| s.log_hit_scaled);
        let item = max(|s| s.log_item_scaled);
        for row in &mut result {
            if row.raw > 0.0 {
                row.raw = row.raw / raw * 100.0;
            }
            if row.log_hit_scaled > 0.0 {
                row.log_hit_scaled /= hit;
            }
            if row.log_item_scaled > 0.0 {
                row.log_item_scaled /= item;
            }
        }
        result
    }

    /// Pure cGzj update. Call only after a one-use native insertion ACK, then
    /// publish under the same owner. The caller owns 60s leading/trailing throttle.
    pub fn recorded(&self, id: &str, text: &str, now_ms: f64) -> Result<Self, String> {
        if id.is_empty() || !now_ms.is_finite() {
            return Err("invalid frecency event".into());
        }
        let mut next = self.clone();
        if !text.is_empty() {
            let key = format_query(text);
            let value = match next.get(&key) {
                Some(Value::Array(rows)) => {
                    let mut rows = rows.clone();
                    if let Some(i) = rows.iter().position(|v| v["id"].as_str() == Some(id)) {
                        rows[i] = count_visit(&rows[i], now_ms)?;
                    } else {
                        rows.push(new_record(id, now_ms));
                    }
                    Value::Array(rows)
                }
                Some(row) if row["id"].as_str() == Some(id) => count_visit(row, now_ms)?,
                Some(row) => json!([row, new_record(id, now_ms)]),
                None => new_record(id, now_ms),
            };
            next.set(key, value);
        }
        match next.get(id) {
            Some(Value::Array(rows)) => {
                let other = rows.iter().any(|v| v["id"].as_str() != Some(id));
                let mut values = rows
                    .iter()
                    .map(|row| {
                        if row["id"].as_str() == Some(id) {
                            count_id_visit(row, now_ms)
                        } else {
                            Ok(row.clone())
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                // Preserve source's surprising extra append when any other ID exists.
                if other {
                    values.push(new_record(id, now_ms));
                }
                next.set(id.to_owned(), Value::Array(values));
            }
            Some(row) => {
                let value = count_id_visit(row, now_ms)?;
                next.set(id.to_owned(), value);
            }
            None if id != "INF_SEARCH" => {
                let mut value = new_record(id, now_ms);
                value["_reduced"] = Value::Bool(true);
                next.set(id.to_owned(), value);
            }
            None => {}
        }
        next.enumerate_js_keys();
        Ok(next)
    }
}

pub fn format_query(query: &str) -> String {
    let query = match query.chars().next() {
        Some(
            c @ (':' | '@' | '\u{ff20}' | '#' | '\u{ff03}' | '/' | '\u{ff0f}' | '+' | '\u{ff0b}'),
        ) => &query[c.len_utf8()..],
        _ => query,
    };
    super::normalize::normalize(query)
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub raw: f64,
    pub hit_count: f64,
    pub item_count: f64,
    pub hit_weight: f64,
    pub item_weight: f64,
    pub log_hit_scaled: f64,
    pub log_item_scaled: f64,
    pub latest_hit: f64,
    pub latest_item: f64,
    pub hit_buckets: [f64; 6],
    pub item_buckets: [f64; 6],
}

fn records(value: &Value) -> &[Value] {
    match value {
        Value::Array(values) => values,
        value => std::slice::from_ref(value),
    }
}
fn count(value: Option<&Value>) -> f64 {
    value.map_or(0.0, |v| v["count"].as_f64().unwrap_or(f64::NAN))
}
const BUCKETS: [(f64, f64); 6] = [
    (14_400_000.0, 100.0),
    (86_400_000.0, 80.0),
    (259_200_000.0, 60.0),
    (604_800_000.0, 40.0),
    (2_592_000_000.0, 20.0),
    (7_776_000_000.0, 10.0),
];
fn weight(value: Option<&Value>, now: f64) -> f64 {
    let Some(visits) = value.and_then(|v| v["visits"].as_array()) else {
        return 0.0;
    };
    let mut total = 0.0;
    for visit in visits.iter().rev() {
        let age = now - visit.as_f64().unwrap_or(f64::NAN);
        total += BUCKETS
            .iter()
            .find(|(bound, _)| age < *bound)
            .map_or(0.0, |(_, weight)| *weight);
    }
    if value.is_some_and(|v| v["_reduced"].as_bool() == Some(true)) {
        total *= 0.5;
    }
    total / visits.len() as f64
}
fn latest(value: Option<&Value>) -> f64 {
    value.and_then(|v| v["visits"].as_array()).map_or(0.0, |v| {
        v.iter()
            .filter_map(Value::as_f64)
            .fold(f64::NEG_INFINITY, f64::max)
            / 1000.0
    })
}
fn buckets(value: Option<&Value>, now: f64) -> [f64; 6] {
    let mut result = [0.0; 6];
    if let Some(visits) = value.and_then(|v| v["visits"].as_array()) {
        for visit in visits {
            if let Some(i) = BUCKETS
                .iter()
                .position(|(bound, _)| now - visit.as_f64().unwrap_or(f64::NAN) < *bound)
            {
                result[i] += 1.0;
            }
        }
    }
    result
}
fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value < i64::MAX as f64 {
        json!(value as i64)
    } else {
        json!(value)
    }
}
fn new_record(id: &str, now: f64) -> Value {
    json!({"id":id,"count":1,"visits":[number(now)]})
}
fn count_visit(value: &Value, now: f64) -> Result<Value, String> {
    let mut visits = value["visits"]
        .as_array()
        .ok_or("recorded visits unavailable")?
        .clone();
    if visits.len() == 10 {
        visits.remove(0);
    }
    visits.push(number(now));
    Ok(json!({"id":value["id"],"count":number(count(Some(value))+1.0),"visits":visits}))
}
fn count_id_visit(value: &Value, now: f64) -> Result<Value, String> {
    let mut result = count_visit(value, now)?;
    if value["id"].as_str() == Some("INF_SALESFORCE") {
        result["_reduced"] = Value::Bool(false);
    }
    Ok(result)
}
