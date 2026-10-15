//! Strict envelope/nested recovery decoding without a raw_value dependency.
use super::*;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::fmt;

// Reject duplicate keys recursively BEFORE constructing a Value. In particular,
// a later null must not erase an unresolved nested check_ticket.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON without duplicate fields")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Unique, E> {
                self.visit_unit()
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut out = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    out.push(v);
                }
                Ok(Unique(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some((k, Unique(v))) = a.next_entry::<String, Unique>()? {
                    if out.insert(k.clone(), v).is_some() {
                        return Err(de::Error::custom(format!("duplicate field {k}")));
                    }
                }
                Ok(Unique(Value::Object(out)))
            }
        }
        d.deserialize_any(V)
    }
}
pub(super) fn strict<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, AppError> {
    let Unique(value) = serde_json::from_str(raw)?;
    Ok(serde_json::from_value(value)?)
}
fn strict_state<'de, D: serde::Deserializer<'de>>(d: D) -> Result<DraftSync, D::Error> {
    let Unique(raw) = Unique::deserialize(d)?;
    if raw.get("check_ticket").is_none() || raw.get("check_deadline").is_none() {
        return Err(de::Error::custom("missing nested v5 recovery guard"));
    }
    serde_json::from_value(raw).map_err(de::Error::custom)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    #[serde(deserialize_with = "strict_state")]
    state: DraftSync,
    enrollment_id: String,
    verified_epoch: u64,
    basis: Basis,
    origin: Origin,
}
pub(super) fn decode(
    scope: &Scope,
    revision: i64,
    format: i64,
    content: &str,
) -> Result<ProtectedRow, AppError> {
    if format != PROTECTED_FORMAT || revision <= 0 {
        return Err(invalid("unprotected or unsupported row"));
    }
    let e: Envelope = serde_json::from_str(content)?;
    if !valid_uuid(&e.enrollment_id) {
        return Err(invalid("invalid enrollment identity"));
    }
    sql_number(e.verified_epoch)?;
    if (e.verified_epoch == 0) != (e.basis == Basis::Unverified)
        || matches!(&e.origin, Origin::LocalIssued{client_msg_id} if !valid_uuid(client_msg_id) || client_msg_id != e.state.client_msg_id())
        || (e.basis == Basis::Local && !matches!(e.origin, Origin::LocalIssued { .. }))
    {
        return Err(invalid("invalid verification basis/origin"));
    }
    validate(scope, &e.state)?;
    Ok(ProtectedRow {
        stored: Stored {
            revision: revision as u64,
            state: e.state,
        },
        ownership: Ownership {
            enrollment_id: e.enrollment_id,
            verified_epoch: e.verified_epoch,
            basis: e.basis,
            origin: e.origin,
        },
    })
}
pub(super) fn encode(row: &ProtectedRow) -> Result<String, AppError> {
    if !valid_uuid(&row.ownership.enrollment_id) {
        return Err(invalid("invalid enrollment identity"));
    }
    sql_number(row.ownership.verified_epoch)?;
    validate(row.stored.state.scope(), &row.stored.state)?;
    let encoded = serde_json::to_string(&Envelope {
        state: row.stored.state.clone(),
        enrollment_id: row.ownership.enrollment_id.clone(),
        verified_epoch: row.ownership.verified_epoch,
        basis: row.ownership.basis.clone(),
        origin: row.ownership.origin.clone(),
    })?;
    // Validate derived metadata on write as well as load; no invalid combination
    // can become a later startup-only failure.
    decode(
        row.stored.state.scope(),
        sql_number(row.stored.revision)?,
        PROTECTED_FORMAT,
        &encoded,
    )?;
    Ok(encoded)
}
