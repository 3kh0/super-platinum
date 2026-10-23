//! Insert-only Delta custody and UTF-16 change application. Intermediate UTF-16
//! units can be split by source indices; only a complete valid document exports.
use super::*;
use serde_json::Map;
mod budget;
mod coalescing;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Insert {
    Unit(u16),
    Embed(Value),
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Unit {
    pub insert: Insert,
    pub attributes: Map<String, Value>,
    // iL+W iterator preserves truthy empty objects but omits null/absent attrs.
    pub attributes_present: bool,
}
impl Unit {
    pub fn new(insert: Insert, attributes: Map<String, Value>) -> Self {
        let attributes_present = !attributes.is_empty();
        Self {
            insert,
            attributes,
            attributes_present,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Document(pub Vec<Unit>);
const LIMIT: usize = 262_144;

impl Document {
    pub fn capture(ops: &[Value]) -> Result<Self> {
        if ops.len() > 32768 {
            return Err(SeedUnavailable::Limit);
        }
        let mut result = Self::default();
        let mut attribute_cost = 0;
        for op in ops {
            let object = op.as_object().ok_or(SeedUnavailable::MalformedMessage)?;
            if object
                .keys()
                .any(|key| !matches!(key.as_str(), "insert" | "attributes"))
            {
                return Err(SeedUnavailable::MalformedMessage);
            }
            let units = match op.get("insert") {
                Some(Value::String(text)) if !text.is_empty() => text.encode_utf16().count(),
                Some(Value::Object(embed)) if !embed.is_empty() => 1,
                _ => return Err(SeedUnavailable::MalformedMessage),
            };
            result
                .len()
                .checked_add(units)
                .filter(|n| *n <= LIMIT)
                .ok_or(SeedUnavailable::Limit)?;
            // Bound repeated attribute clones before cloning even the first map.
            budget::reserve(&mut attribute_cost, op.get("attributes"), units)?;
            let attrs = attributes(op.get("attributes"))?;
            coalescing::validate(&attrs)?;
            let attributes_present = op.get("attributes").is_some_and(Value::is_object);
            match op.get("insert") {
                Some(Value::String(text)) if !text.is_empty() => {
                    result.0.extend(text.encode_utf16().map(|unit| Unit {
                        insert: Insert::Unit(unit),
                        attributes: attrs.clone(),
                        attributes_present,
                    }));
                }
                Some(Value::Object(embed)) if !embed.is_empty() => result.0.push(Unit {
                    insert: Insert::Embed(Value::Object(embed.clone())),
                    attributes: attrs,
                    attributes_present,
                }),
                _ => return Err(SeedUnavailable::MalformedMessage),
            }
            if result.len() > LIMIT {
                return Err(SeedUnavailable::Limit);
            }
        }
        Ok(result)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn slice(&self, start: usize, end: usize) -> Result<Self> {
        Ok(Self(
            self.0
                .get(start..end)
                .ok_or(SeedUnavailable::MalformedMessage)?
                .to_vec(),
        ))
    }
    pub fn ops(&self) -> Result<Vec<Value>> {
        let mut ops = vec![];
        let mut at = 0;
        while at < self.len() {
            let unit = &self.0[at];
            let mut chosen = unit;
            let insert = match &unit.insert {
                Insert::Embed(value) => {
                    at += 1;
                    value.clone()
                }
                Insert::Unit(_) => {
                    let mut text = vec![];
                    while let Some(next) = self.0.get(at) {
                        let Insert::Unit(code) = next.insert else {
                            break;
                        };
                        if !coalescing::same(chosen, next)? {
                            break;
                        }
                        chosen = next;
                        text.push(code);
                        at += 1;
                    }
                    Value::String(
                        String::from_utf16(&text).map_err(|_| SeedUnavailable::InvalidUnicode)?,
                    )
                }
            };
            let mut op = json!({"insert":insert});
            if chosen.attributes_present || !chosen.attributes.is_empty() {
                op["attributes"] = Value::Object(chosen.attributes.clone());
            }
            ops.push(op);
            if ops.len() > 32768 {
                return Err(SeedUnavailable::Limit);
            }
        }
        Ok(ops)
    }
    /// Apply one actual change Delta to this document; remaining original units
    /// are implicitly retained. Null attributes remove, false is retained.
    pub fn apply(&mut self, change: &Change) -> Result<()> {
        let mut at = 0usize;
        let mut output = Vec::new();
        for operation in &change.0 {
            match operation {
                Edit::Insert(document) => output.extend(document.0.iter().cloned()),
                Edit::Delete(count) => {
                    at = at
                        .checked_add(*count)
                        .filter(|n| *n <= self.len())
                        .ok_or(SeedUnavailable::MalformedMessage)?;
                }
                Edit::Retain(count, attributes) => {
                    let end = at
                        .checked_add(*count)
                        .filter(|n| *n <= self.len())
                        .ok_or(SeedUnavailable::MalformedMessage)?;
                    for unit in &self.0[at..end] {
                        let mut unit = unit.clone();
                        unit.attributes = coalescing::compose(&unit.attributes, attributes)?;
                        unit.attributes_present = !unit.attributes.is_empty();
                        output.push(unit);
                    }
                    at = end;
                }
            }
            if output.len() > LIMIT {
                return Err(SeedUnavailable::Limit);
            }
        }
        for unit in &self.0[at..] {
            let mut unit = unit.clone();
            unit.attributes = coalescing::compose(&unit.attributes, &Map::new())?;
            unit.attributes_present = !unit.attributes.is_empty();
            output.push(unit);
        }
        if output.len() > LIMIT {
            return Err(SeedUnavailable::Limit);
        }
        self.0 = output;
        Ok(())
    }
    /// Delta.insert drops empty attributes, unlike iterator + raw push.
    pub fn normalize_insert_attributes(&mut self) {
        for unit in &mut self.0 {
            unit.attributes_present = !unit.attributes.is_empty();
        }
    }
    pub fn ends_newline(&self) -> bool {
        self.0
            .last()
            .is_some_and(|unit| unit.insert == Insert::Unit(10))
    }
    pub fn text(
        &self,
        replace_links: bool,
        replace_code: bool,
        replacement: u16,
    ) -> Result<Vec<u16>> {
        let mut text = Vec::new();
        for unit in &self.0 {
            match &unit.insert {
                Insert::Embed(value) if recognized_embed(value) => text.push(replacement),
                Insert::Unit(_)
                    if replace_links && truthy(unit.attributes.get("link"))
                        || replace_code && truthy(unit.attributes.get("code")) =>
                {
                    text.push(replacement)
                }
                Insert::Embed(_) if replace_code && truthy(unit.attributes.get("code")) => {
                    text.push(replacement)
                }
                Insert::Embed(value) => {
                    // Fks7 concatenates unrecognized objects. A JSON own
                    // toString cannot be callable; ordinary objects use this
                    // inherited tag rather than an invented embed placeholder.
                    if value
                        .as_object()
                        .is_some_and(|v| v.contains_key("toString"))
                    {
                        return Err(SeedUnavailable::MalformedMessage);
                    }
                    text.extend("[object Object]".encode_utf16());
                }
                Insert::Unit(code) => text.push(*code),
            }
        }
        Ok(text)
    }
    pub fn lines(&self) -> Vec<(usize, Self, Map<String, Value>)> {
        let mut lines = vec![];
        let mut start = 0;
        for (at, unit) in self.0.iter().enumerate() {
            if unit.insert == Insert::Unit(10) {
                lines.push((
                    start,
                    Self(self.0[start..at].to_vec()),
                    unit.attributes.clone(),
                ));
                start = at + 1;
            }
        }
        if start < self.len() {
            lines.push((start, Self(self.0[start..].to_vec()), Map::new()));
        }
        lines
    }
}

/// m6Mr recognizes these four families, not every object-valued insertion.
pub(super) fn recognized_embed(value: &Value) -> bool {
    truthy(value.get("slackemoji"))
        || truthy(value.get("workflowtoken"))
        || truthy(value.get("slackhorizontalrule"))
        || value
            .get("slackopaqueblock")
            .and_then(|v| v.get("block"))
            .is_some_and(Value::is_object)
}
fn attributes(value: Option<&Value>) -> Result<Map<String, Value>> {
    match value {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(value)) => Ok(value.clone()),
        _ => Err(SeedUnavailable::MalformedMessage),
    }
}

#[derive(Clone, Debug)]
pub(super) enum Edit {
    Retain(usize, Map<String, Value>),
    Delete(usize),
    Insert(Document),
}
#[derive(Clone, Debug, Default)]
pub(super) struct Change(pub Vec<Edit>);
impl Change {
    pub fn retain(&mut self, count: usize) {
        self.annotate(count, Map::new());
    }
    pub fn annotate(&mut self, count: usize, attributes: Map<String, Value>) {
        if count > 0 {
            self.0.push(Edit::Retain(count, attributes));
        }
    }
    pub fn delete(&mut self, count: usize) {
        if count > 0 {
            self.0.push(Edit::Delete(count));
        }
    }
    pub fn insert(&mut self, text: &str, attributes: Map<String, Value>) {
        if !text.is_empty() {
            self.0.push(Edit::Insert(Document(
                text.encode_utf16()
                    .map(|unit| Unit::new(Insert::Unit(unit), attributes.clone()))
                    .collect(),
            )));
        }
    }
    pub fn insert_document(&mut self, document: Document) {
        if document.len() > 0 {
            self.0.push(Edit::Insert(document));
        }
    }
}
