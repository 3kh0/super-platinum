//! Ordinary Select filter leaves; values are ordered option IDs, never labels.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectFilterOperator {
    Contains,
    DoesNotContain,
    IsEmpty,
    IsNotEmpty,
}
impl SelectFilterOperator {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "contains" => Self::Contains,
            "does_not_contain" => Self::DoesNotContain,
            "is_empty" => Self::IsEmpty,
            "is_not_empty" => Self::IsNotEmpty,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::DoesNotContain => "does_not_contain",
            Self::IsEmpty => "is_empty",
            Self::IsNotEmpty => "is_not_empty",
        }
    }
    pub fn requires_values(self) -> bool {
        matches!(self, Self::Contains | Self::DoesNotContain)
    }
    pub fn valid_values(self, values: &[String]) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        (self.requires_values() || values.is_empty())
            && values.iter().all(|id| identity(id) && seen.insert(id))
    }
}
/// A presentation list, not permission or submission authority. Unknown options
/// and ambiguous IDs stay unsupported. No local choice-count quota is introduced.
pub(super) fn choices(column: &Column) -> Option<Vec<(String, String)>> {
    if column.kind != "select"
        || ["is_deleted", "deleted", "suggested", "is_suggested"]
            .iter()
            .any(|k| column.raw.get(*k).is_some_and(|v| v != &Value::Bool(false)))
    {
        return None;
    }
    let options = column.raw.get("options")?.as_object()?;
    if !options
        .keys()
        .all(|k| matches!(k.as_str(), "format" | "choices"))
        || !options
            .get("format")
            .is_none_or(|v| matches!(v.as_str(), Some("single_select" | "multi_select")))
    {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    options
        .get("choices")?
        .as_array()?
        .iter()
        .map(|v| {
            let o = v.as_object()?;
            let id = o.get("value")?.as_str()?;
            let label = o.get("label")?.as_str()?;
            if !identity(id)
                || id.contains('|')
                || !seen.insert(id)
                || !o
                    .keys()
                    .all(|k| matches!(k.as_str(), "value" | "label" | "color"))
                || o.get("color").is_some_and(|v| !v.is_string())
            {
                return None;
            }
            Some((id.into(), label.into()))
        })
        .collect()
}
pub(super) fn wire(column: &Column, op: SelectFilterOperator, values: &[String]) -> Option<Value> {
    let choices = choices(column)?;
    (op.valid_values(values)
        && values
            .iter()
            .all(|id| choices.iter().any(|(known, _)| known == id)))
    .then(|| json!({"key":column.key,"operator":op.as_str(),"values":values}))
}
#[cfg(test)]
mod tests;
