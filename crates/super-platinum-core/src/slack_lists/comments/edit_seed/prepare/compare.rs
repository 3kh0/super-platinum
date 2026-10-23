//! IHC0's no-op predicate using AGzE's document comparison. Inputs are owned
//! JSON observations, not a claim of shared JavaScript object identity.
use super::*;
use document::Unit;
pub(super) mod equality;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct EditComparison {
    original: Vec<Value>,
    edited: Vec<Value>,
    processed: Vec<Value>,
    unfurled: Value,
    unchanged: bool,
}
impl EditComparison {
    /// `unfurled` must be the positively observed edit attachment value. An
    /// empty array/object is truthy and prevents the source no-op shortcut.
    pub fn capture(
        original: &[Value],
        edited: &[Value],
        processed: &[Value],
        unfurled: &Value,
    ) -> Result<Self> {
        let unchanged =
            (equal(original, edited)? || equal(original, processed)?) && !truthy(Some(unfurled));
        Ok(Self {
            original: original.to_vec(),
            edited: edited.to_vec(),
            processed: processed.to_vec(),
            unfurled: unfurled.clone(),
            unchanged,
        })
    }
    pub fn unchanged(&self) -> bool {
        self.unchanged
    }
    pub fn matches_inputs(
        &self,
        original: &[Value],
        edited: &[Value],
        processed: &[Value],
        unfurled: &Value,
    ) -> bool {
        self.original == original
            && self.edited == edited
            && self.processed == processed
            && self.unfurled == *unfurled
    }
}

fn equal(left: &[Value], right: &[Value]) -> Result<bool> {
    let left = with_newline(left)?;
    let right = with_newline(right)?;
    if left.len() != right.len() {
        return Ok(false);
    }
    for (a, b) in left.0.iter().zip(&right.0) {
        let same = match (&a.insert, &b.insert) {
            (Insert::Unit(a), Insert::Unit(b)) => a == b,
            (Insert::Embed(a), Insert::Embed(b)) => equality::deep(Some(a), Some(b))?,
            _ => false,
        };
        if !same {
            return Ok(false);
        }
        // AttributeMap.diff compares the union of keys individually. Missing
        // and explicit null compare equal except inherited prototype properties.
        for key in a.attributes.keys().chain(b.attributes.keys()) {
            if !equality::attribute(key, a.attributes.get(key), b.attributes.get(key))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn with_newline(ops: &[Value]) -> Result<Document> {
    for op in ops {
        // Delta.diff may compare later embeds after an earlier text difference.
        // Preflight all insert subtrees before the zipped early-exit comparator.
        if let Some(insert @ Value::Object(_)) = op.get("insert") {
            equality::attribute_value(insert, 0)?;
        }
        if let Some(attributes) = op.get("attributes").and_then(Value::as_object) {
            equality::attribute_values(attributes)?;
        }
    }
    let mut document = Document::capture(ops)?;
    let terminal = ops.last().is_some_and(|op| {
        op["insert"].as_str().is_some_and(|s| s.ends_with('\n'))
            && !["slackmention", "slackslug", "slacksearchfilter"]
                .iter()
                .any(|key| truthy(op.get("attributes").and_then(|a| a.get(*key))))
    });
    if !terminal {
        if document.len() >= 262_144 {
            return Err(SeedUnavailable::Limit);
        }
        document
            .0
            .push(Unit::new(Insert::Unit(10), Default::default()));
    }
    Ok(document)
}
