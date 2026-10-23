//! Retained Delta.push/AttributeMap.compose semantics within the supported
//! JSON comparison boundary; original raw input remains owned by callers.
use super::super::compare::equality;
use super::*;

pub(super) fn validate(attributes: &Map<String, Value>) -> Result<()> {
    equality::attribute_values(attributes)?;
    if attributes
        .get("prototype")
        .is_some_and(|v| v.is_object() || v.is_array())
    {
        return Err(SeedUnavailable::MalformedMessage);
    }
    Ok(())
}
pub(super) fn same(left: &Unit, right: &Unit) -> Result<bool> {
    let left_present = left.attributes_present || !left.attributes.is_empty();
    let right_present = right.attributes_present || !right.attributes.is_empty();
    if left_present == right_present && left.attributes == right.attributes {
        return Ok(true);
    }
    let left = left_present.then(|| Value::Object(left.attributes.clone()));
    let right = right_present.then(|| Value::Object(right.attributes.clone()));
    equality::deep(left.as_ref(), right.as_ref())
}
fn inherited(key: &str) -> bool {
    matches!(
        key,
        "constructor"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
            | "__proto__"
    )
}
pub(super) fn compose(
    original: &Map<String, Value>,
    update: &Map<String, Value>,
) -> Result<Map<String, Value>> {
    validate(original)?;
    validate(update)?;
    let mut result: Map<_, _> = update
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    // iL+W reads A[key], including inherited properties, before copying the
    // original. An absent inherited function is therefore not undefined.
    for (key, value) in original {
        if !update.contains_key(key) && !inherited(key) {
            result.insert(key.clone(), value.clone());
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests;
