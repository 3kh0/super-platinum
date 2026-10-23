use super::*;
mod equality;

#[derive(Default)]
pub(super) struct Delta {
    pub(super) ops: Vec<Value>,
}
impl Delta {
    pub(super) fn insert(&mut self, insert: Value, attributes: Value) {
        if insert.as_str() == Some("") {
            return;
        }
        let attributes = attributes.as_object().filter(|a| !a.is_empty());
        if let Some(text) = insert.as_str() {
            if let Some(previous) = self.ops.last_mut() {
                if previous.get("attributes").and_then(Value::as_object) == attributes {
                    if let Some(prior) = previous
                        .get("insert")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                    {
                        previous["insert"] = json!(prior + text);
                        return;
                    }
                }
            }
        }
        let mut op = json!({"insert":insert});
        if let Some(attributes) = attributes {
            op["attributes"] = Value::Object(attributes.clone());
        }
        self.ops.push(op);
    }
    pub(super) fn text(&mut self, text: &str) {
        self.insert(json!(text), Value::Null);
    }
    pub(super) fn append(&mut self, other: Self) {
        for op in other.ops {
            self.insert(op["insert"].clone(), op["attributes"].clone());
        }
    }
    pub(super) fn ends_newline(&self) -> bool {
        self.ops
            .last()
            .and_then(|op| op["insert"].as_str())
            .is_some_and(|text| text.ends_with('\n'))
    }
    pub(super) fn finish(self) -> Result<Vec<Value>, SeedUnavailable> {
        // Delta.insert omits empty attributes. Delta.push compares attributes
        // with f66B's loose equality and retains the RIGHT complete map when
        // adjacent strings merge. Validate even unmerged maps before export.
        let mut ops: Vec<Value> = Vec::new();
        for mut op in self.ops {
            if let Some(attributes) = op.get("attributes").and_then(Value::as_object) {
                equality::attribute_values(attributes)?;
                if attributes
                    .get("prototype")
                    .is_some_and(|v| v.is_object() || v.is_array())
                {
                    return Err(SeedUnavailable::MalformedMessage);
                }
            }
            if let Some(text) = op.get("insert").and_then(Value::as_str) {
                if let Some(previous) = ops.last_mut() {
                    if let Some(prior) = previous.get("insert").and_then(Value::as_str) {
                        if equality::deep(previous.get("attributes"), op.get("attributes"))? {
                            op["insert"] = json!(prior.to_owned() + text);
                            *previous = op;
                            continue;
                        }
                    }
                }
            }
            ops.push(op);
        }
        if ops.len() > 32768
            || ops
                .iter()
                .map(|op| {
                    op["insert"]
                        .as_str()
                        .map_or(1, |s| s.encode_utf16().count())
                })
                .sum::<usize>()
                > 262144
        {
            return Err(SeedUnavailable::Limit);
        }
        Ok(ops)
    }
}

#[cfg(test)]
mod tests;
