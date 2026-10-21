//! Original JSON observation of only file channel_map key order. The carrier
//! and sidecar are decoded together; callers cannot construct or replace keys.
use super::*;
use serde::{
    Deserializer,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use std::{collections::BTreeSet, fmt};

#[derive(Clone)]
pub struct ChannelMapOrder {
    carrier: Arc<Value>,
    root: Arc<Node>,
}
#[derive(Default)]
enum Node {
    #[default]
    None,
    Object(BTreeMap<String, Node>),
    Array(Vec<Node>),
    Keys(Vec<String>),
}
impl Node {
    fn field(&self, key: &str) -> &Node {
        match self {
            Self::Object(map) => map.get(key).unwrap_or(&Self::None),
            _ => &Self::None,
        }
    }
    fn at(&self, index: usize) -> &Node {
        match self {
            Self::Array(rows) => rows.get(index).unwrap_or(&Self::None),
            _ => &Self::None,
        }
    }
}
impl ChannelMapOrder {
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_TOTAL_BYTES {
            return Err(malformed());
        }
        let carrier: Value = serde_json::from_slice(bytes).map_err(|_| malformed())?;
        let (mut nodes, mut size) = (MAX_NODES, MAX_TOTAL_BYTES);
        bounded(&carrier, 0, &mut nodes, &mut size)?;
        let mut parser = serde_json::Deserializer::from_slice(bytes);
        let root = Select::Envelope
            .deserialize(&mut parser)
            .map_err(|_| malformed())?;
        parser.end().map_err(|_| malformed())?;
        Ok(Self {
            carrier: Arc::new(carrier),
            root: Arc::new(root),
        })
    }
    pub(crate) fn carrier(&self) -> &Value {
        &self.carrier
    }
    pub(super) fn matches(&self, carrier: &Value) -> bool {
        self.carrier.as_ref() == carrier
    }
    pub(super) fn info_keys(&self) -> Option<&[String]> {
        match self.root.field("file").field("channel_map") {
            Node::Keys(keys) => Some(keys),
            _ => None,
        }
    }
    pub(super) fn keys(&self, root: usize, embedded: Option<usize>) -> Option<&[String]> {
        let row = self.root.field("files").at(root);
        let row = embedded.map_or(row, |index| {
            row.field("canvas_metadata")
                .field("items")
                .at(index)
                .field("file")
        });
        match row.field("channel_map") {
            Node::Keys(keys) => Some(keys),
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
enum Select {
    Envelope,
    Files,
    Root,
    Metadata,
    Items,
    Item,
    Embedded,
    Keys,
}
impl Select {
    fn child(self, key: &str) -> Option<Self> {
        match (self, key) {
            (Self::Envelope, "files") => Some(Self::Files),
            (Self::Envelope, "file") => Some(Self::Root),
            (Self::Root | Self::Embedded, "channel_map") => Some(Self::Keys),
            (Self::Root, "canvas_metadata") => Some(Self::Metadata),
            (Self::Metadata, "items") => Some(Self::Items),
            (Self::Item, "file") => Some(Self::Embedded),
            _ => None,
        }
    }
}
impl<'de> DeserializeSeed<'de> for Select {
    type Value = Node;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Node, D::Error> {
        d.deserialize_any(Observe(self))
    }
}
struct Observe(Select);
impl<'de> Visitor<'de> for Observe {
    type Value = Node;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("original file response JSON")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Node, M::Error> {
        if matches!(self.0, Select::Keys) {
            let mut keys = Vec::new();
            let mut seen = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                map.next_value::<IgnoredAny>()?;
                // JSON.parse keeps the first property position and last value.
                if seen.insert(key.clone()) {
                    keys.push(key);
                }
            }
            // Stable sorting preserves encounter order among non-index keys.
            keys.sort_by_key(|key| index(key).map_or((1, 0), |n| (0, n)));
            return Ok(Node::Keys(keys));
        }
        let mut fields = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            if let Some(child) = self.0.child(&key) {
                fields.insert(key, map.next_value_seed(child)?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(Node::Object(fields))
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Node, S::Error> {
        let child = match self.0 {
            Select::Files => Some(Select::Root),
            Select::Items => Some(Select::Item),
            _ => None,
        };
        if let Some(child) = child {
            let mut rows = Vec::new();
            while let Some(row) = seq.next_element_seed(child)? {
                rows.push(row);
            }
            Ok(Node::Array(rows))
        } else {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(Node::None)
        }
    }
    fn visit_unit<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::None)
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Node, E> {
        Ok(Node::None)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Node, E> {
        Ok(Node::None)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Node, E> {
        Ok(Node::None)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Node, E> {
        Ok(Node::None)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Node, E> {
        Ok(Node::None)
    }
}
fn index(key: &str) -> Option<u32> {
    let n = key.parse::<u32>().ok()?;
    (n != u32::MAX && n.to_string() == key).then_some(n)
}

#[cfg(test)]
mod tests;
