//! Positive object-key order from the SAME bounded replies response bytes.
//! Xc/Q opens delimiters in Object.keys(style) order. serde_json::Value alone
//! cannot reconstruct that order. No global serde feature or second HTTP read.
use super::ListError;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Map, Number, Value};
use std::{collections::BTreeSet, fmt};

const LIMIT: usize = 1024 * 1024;
const SEED_FIELDS: [&str; 11] = [
    "ts",
    "channel",
    "thread_ts",
    "user",
    "subtype",
    "slack_list",
    "list_edit",
    "text",
    "blocks",
    "attachments",
    "files",
];

#[derive(Clone, Debug)]
pub struct ResponseWitness {
    raw: Node,
    value: Value,
}
impl ResponseWitness {
    /// The caller passes the Value decoded by the existing strict HTTP reader.
    /// This constructor independently rejects duplicate keys and binds ALL of
    /// that value to these bytes, before extracting any message/style order.
    pub fn decode(bytes: &[u8], decoded: &Value) -> Result<Self, ListError> {
        if bytes.len() > LIMIT {
            return Err(ListError(
                "List comment ordered response exceeds the size limit.",
            ));
        }
        let raw: Node = serde_json::from_slice(bytes)
            .map_err(|_| ListError("List comment ordered response is malformed."))?;
        let value = raw.value();
        if &value != decoded
            || value.get("ok") != Some(&Value::Bool(true))
            || value.get("error").is_some_and(|error| !error.is_null())
            || !value.get("messages").is_some_and(Value::is_array)
        {
            return Err(ListError(
                "List comment ordered observation is unavailable.",
            ));
        }
        Ok(Self { raw, value })
    }
    pub fn response(&self) -> &Value {
        &self.value
    }
    /// Inert persistence representation of the retained object order and values.
    /// This is not the original HTTP whitespace/number spelling. Deserialization
    /// must decode and correlate it again; it grants no current HTTP authority.
    pub fn ordered_json(&self) -> Result<String, ListError> {
        serde_json::to_string(&self.raw)
            .map_err(|_| ListError("List comment ordered response is malformed."))
    }
    /// Exact replies-array index, never a timestamp search with first-match
    /// ambiguity. Host Page parsing and canonical identity checks remain required.
    pub fn message(&self, index: usize) -> Result<MessageWitness, ListError> {
        let raw = self
            .raw
            .get("messages")
            .and_then(|n| n.index(index))
            .filter(|n| matches!(n, Node::Object(_)))
            .ok_or(ListError(
                "List comment ordered observation is unavailable.",
            ))?
            .clone();
        let value = raw.value();
        if value
            .get("ts")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(ListError("List comment ordered response is malformed."));
        }
        Ok(MessageWitness { raw, value })
    }
}

#[derive(Clone, Debug)]
pub struct MessageWitness {
    raw: Node,
    value: Value,
}
impl MessageWitness {
    pub fn raw_message(&self) -> &Value {
        &self.value
    }
    /// Binds a raw body to this observation; cannot establish fresh object order
    /// from a Value. Save must additionally use same_seed on the freshly decoded
    /// witness. No typed Message reserialization fabricates an original witness.
    pub fn matches_seed(&self, message: &Value) -> bool {
        message.is_object()
            && SEED_FIELDS
                .iter()
                .all(|key| self.value.get(key) == message.get(key))
    }
    /// Full seed subtrees and identity plus ordered style keys, from two real
    /// decoded responses. Reaction/counter-only changes do not reseed an edit.
    pub fn same_seed(&self, other: &Self) -> bool {
        self.matches_seed(&other.value)
            && style_orders(self.raw.get("blocks")) == style_orders(other.raw.get("blocks"))
    }
    /// Path is [block index, element index, nested element index, ...].
    /// Missing style returns None; present empty style returns Some(empty).
    /// Keys follow JSON.parse/Object.keys order, INCLUDING unknown and false-valued
    /// keys. Xc/Q inspects recognized key presence, not the Boolean value.
    pub fn style_keys(&self, path: &[usize]) -> Option<Vec<&str>> {
        let (&first, rest) = path.split_first()?;
        let mut node = self.raw.get("blocks")?.index(first)?;
        for &index in rest {
            node = node.get("elements")?.index(index)?;
        }
        match node.get("style")? {
            Node::Object(entries) => Some(object_keys(entries)),
            _ => None,
        }
    }
}

fn style_orders(node: Option<&Node>) -> Vec<(Vec<usize>, Vec<String>)> {
    fn visit(node: &Node, path: &mut Vec<usize>, out: &mut Vec<(Vec<usize>, Vec<String>)>) {
        if let Some(Node::Object(style)) = node.get("style") {
            out.push((
                path.clone(),
                object_keys(style).into_iter().map(str::to_owned).collect(),
            ));
        }
        if let Some(Node::Array(elements)) = node.get("elements") {
            for (index, item) in elements.iter().enumerate() {
                path.push(index);
                visit(item, path, out);
                path.pop();
            }
        }
    }
    let mut out = vec![];
    if let Some(Node::Array(blocks)) = node {
        for (index, block) in blocks.iter().enumerate() {
            visit(block, &mut vec![index], &mut out);
        }
    }
    out
}

// ECMAScript own string keys enumerate canonical array indices first, in
// ascending numeric order, then other string keys in original insertion order.
fn object_keys(entries: &[(String, Node)]) -> Vec<&str> {
    let index = |key: &str| {
        key.parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == key)
    };
    let mut indexed = entries
        .iter()
        .filter_map(|(key, _)| index(key).map(|n| (n, key.as_str())))
        .collect::<Vec<_>>();
    indexed.sort_by_key(|(n, _)| *n);
    let mut keys = indexed.into_iter().map(|(_, key)| key).collect::<Vec<_>>();
    keys.extend(
        entries
            .iter()
            .filter(|(key, _)| index(key).is_none())
            .map(|(key, _)| key.as_str()),
    );
    keys
}

#[derive(Clone, Debug)]
enum Node {
    Scalar(Value),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
}
impl serde::Serialize for Node {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Self::Scalar(value) => value.serialize(serializer),
            Self::Array(values) => {
                let mut output = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    output.serialize_element(value)?;
                }
                output.end()
            }
            Self::Object(values) => {
                let mut output = serializer.serialize_map(Some(values.len()))?;
                for (key, value) in values {
                    output.serialize_entry(key, value)?;
                }
                output.end()
            }
        }
    }
}
impl Node {
    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn index(&self, index: usize) -> Option<&Self> {
        match self {
            Self::Array(entries) => entries.get(index),
            _ => None,
        }
    }
    fn value(&self) -> Value {
        match self {
            Self::Scalar(v) => v.clone(),
            Self::Array(items) => Value::Array(items.iter().map(Self::value).collect()),
            Self::Object(entries) => Value::Object(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), v.value()))
                    .collect::<Map<_, _>>(),
            ),
        }
    }
}
impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Node;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("duplicate-free bounded JSON preserving object order")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Node, E> {
                Ok(Node::Scalar(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Node, E> {
                Ok(Node::Scalar(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Node, E> {
                Ok(Node::Scalar(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Node, E> {
                Number::from_f64(v)
                    .map(|n| Node::Scalar(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Node, E> {
                Ok(Node::Scalar(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Node, E> {
                Ok(Node::Scalar(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Node, E> {
                Ok(Node::Scalar(Value::Null))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
                let mut items = vec![];
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Node::Array(items))
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
                let mut entries = vec![];
                let mut keys = BTreeSet::<String>::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key.clone()) {
                        return Err(de::Error::custom("duplicate JSON object key"));
                    }
                    entries.push((key, map.next_value()?));
                }
                Ok(Node::Object(entries))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests;
