//! Local response order only. Named-field reads do not grant order authority
//! to serde_json::Map; extras workspace preference dispatch uses actual entries.
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::fmt;
#[derive(Clone)]
pub(super) enum Node {
    Atom(Value),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
}
impl Node {
    pub(super) fn value(&self) -> Value {
        match self {
            Self::Atom(v) => v.clone(),
            Self::Array(a) => Value::Array(a.iter().map(Self::value).collect()),
            Self::Object(o) => {
                Value::Object(o.iter().map(|(k, v)| (k.clone(), v.value())).collect())
            }
        }
    }
    pub(super) fn get(&self, k: &str) -> Option<&Self> {
        match self {
            Self::Object(o) => o.iter().find(|(n, _)| n == k).map(|(_, v)| v),
            _ => None,
        }
    }
    pub(super) fn entries(&self) -> Option<Vec<(&str, &Self)>> {
        match self {
            Self::Object(o) => {
                let mut entries: Vec<_> = o.iter().map(|(k, v)| (k.as_str(), v)).collect();
                // ECMAScript own-key enumeration: array-index keys before other keys.
                entries.sort_by_key(|(k, _)| index(k).map_or((1, 0), |n| (0, n)));
                Some(entries)
            }
            _ => None,
        }
    }
}
fn index(s: &str) -> Option<u32> {
    let n = s.parse::<u32>().ok()?;
    (n != u32::MAX && n.to_string() == s).then_some(n)
}
impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Node;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON response")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Node, E> {
                Ok(Node::Atom(v.into()))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Node, E> {
                Ok(Node::Atom(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Node, E> {
                Ok(Node::Atom(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Node, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Node::Atom(n.into()))
                    .ok_or_else(|| E::custom("nonfinite"))
            }
            fn visit_str<E>(self, v: &str) -> Result<Node, E> {
                Ok(Node::Atom(v.into()))
            }
            fn visit_string<E>(self, v: String) -> Result<Node, E> {
                Ok(Node::Atom(v.into()))
            }
            fn visit_unit<E>(self) -> Result<Node, E> {
                Ok(Node::Atom(Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Node, E> {
                self.visit_unit()
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Node, A::Error> {
                let mut v = vec![];
                while let Some(x) = a.next_element()? {
                    v.push(x)
                }
                Ok(Node::Array(v))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Node, A::Error> {
                let mut v: Vec<(String, Node)> = vec![];
                while let Some((k, n)) = a.next_entry::<String, Node>()? {
                    if let Some((_, old)) = v.iter_mut().find(|(key, _)| key == &k) {
                        *old = n
                    } else {
                        v.push((k, n))
                    }
                }
                Ok(Node::Object(v))
            }
        }
        d.deserialize_any(V)
    }
}
