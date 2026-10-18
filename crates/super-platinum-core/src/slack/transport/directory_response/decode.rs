//! Preserve the normal serde_json Value and independently observe only the
//! selected object's key order from the same bytes. No sidecar wire keys.
use super::DirectoryResponse;
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{collections::BTreeSet, fmt};

pub(super) fn decode(bytes: &[u8]) -> Result<DirectoryResponse, serde_json::Error> {
    decode_field(bytes, "results")
}

pub(super) fn decode_field(
    bytes: &[u8],
    field: &str,
) -> Result<DirectoryResponse, serde_json::Error> {
    let raw: Value = serde_json::from_slice(bytes)?;
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let keys = Envelope(field).deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(DirectoryResponse { raw, keys })
}

struct Envelope<'a>(&'a str);
impl<'de> DeserializeSeed<'de> for Envelope<'_> {
    type Value = Option<Vec<String>>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        struct E<'a>(&'a str);
        impl<'de> Visitor<'de> for E<'_> {
            type Value = Option<Vec<String>>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an ordered response object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut keys = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == self.0 {
                        keys = map.next_value::<Groups>()?.0;
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(keys)
            }
        }
        d.deserialize_map(E(self.0))
    }
}

struct Groups(Option<Vec<String>>);
impl<'de> Deserialize<'de> for Groups {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct G;
        impl<'de> Visitor<'de> for G {
            type Value = Groups;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("directory results")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Groups, M::Error> {
                let mut seen = BTreeSet::new();
                let mut keys = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    map.next_value::<IgnoredAny>()?;
                    // JSON.parse keeps a duplicate property's first position,
                    // while the ordinary Value above keeps its final value.
                    if seen.insert(key.clone()) {
                        keys.push(key);
                    }
                }
                // Enumerable array-index properties precede other strings.
                // 2^32-1 and noncanonical spellings are ordinary string keys.
                keys.sort_by_key(|key| index(key).map_or((1, 0), |n| (0, n)));
                Ok(Groups(Some(keys)))
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Groups, S::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Groups(None))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Groups, E> {
                Ok(Groups(None))
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Groups, E> {
                Ok(Groups(None))
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Groups, E> {
                Ok(Groups(None))
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Groups, E> {
                Ok(Groups(None))
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Groups, E> {
                Ok(Groups(None))
            }
            fn visit_str<E: de::Error>(self, _: &str) -> Result<Groups, E> {
                Ok(Groups(None))
            }
        }
        d.deserialize_any(G)
    }
}
fn index(key: &str) -> Option<u32> {
    let n = key.parse::<u32>().ok()?;
    (n != u32::MAX && n.to_string() == key).then_some(n)
}
