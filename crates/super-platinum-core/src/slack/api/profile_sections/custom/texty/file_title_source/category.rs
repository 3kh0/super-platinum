//! Effective category evidence only: it never grants title/rank authority.
use super::*;

const KEYS: &[&str] = &[
    "id",
    "mode",
    "filetype",
    "subtype",
    "file_access",
    "isUnknown",
    "isNonExistent",
    "is_tombstoned",
    "restriction_type",
    "canvas_template_mode",
    "list_template_mode",
    "is_deleted",
    "is_hidden_by_limit",
    "is_external",
    "external_type",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exclusion {
    NotVisible,
    NonNative,
}
#[derive(Clone)]
pub struct CategoryObservation {
    identity: Arc<()>,
    id: String,
    row: Arc<Row>,
}
impl CategoryObservation {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn source(&self) -> &Value {
        &self.row.source
    }
    pub fn effective(&self) -> &Value {
        self.row.category.as_ref().expect("category observation")
    }
    pub fn exclusion(&self) -> Option<Exclusion> {
        exclusion(self.effective())
    }
    pub fn current(&self, store: &Store) -> bool {
        Arc::ptr_eq(&self.identity, &store.identity)
            && store
                .rows
                .get(&self.id)
                .is_some_and(|row| Arc::ptr_eq(row, &self.row))
    }
}
impl Store {
    pub fn category(&self, id: &str) -> Result<CategoryObservation, Error> {
        let row = self
            .rows
            .get(id)
            .filter(|row| row.category.is_some())
            .ok_or_else(unavailable)?;
        Ok(CategoryObservation {
            identity: self.identity.clone(),
            id: id.into(),
            row: row.clone(),
        })
    }
}
fn not_visible(row: &Value) -> bool {
    ["not_visible", "template_not_visible"]
        .iter()
        .any(|value| row["mode"] == *value || row["file_access"] == *value)
}
fn fields(row: &Map<String, Value>) -> Map<String, Value> {
    KEYS.iter()
        .filter_map(|key| row.get(*key).map(|value| ((*key).into(), value.clone())))
        .collect()
}
/// 4OY3 does not merge a not-visible incoming row. This proof survives prior
/// poison only for exclusion; no prior title/category is reconstructed.
pub(super) fn project(incoming: &Map<String, Value>, previous: Option<&Row>) -> Option<Value> {
    let raw = Value::Object(incoming.clone());
    if not_visible(&raw) {
        return Some(Value::Object(fields(incoming)));
    }
    let previous = match previous {
        Some(row) => Some(row.category.as_ref()?),
        None => None,
    };
    let mut effective = previous
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // These branches either bypass normal merging or reset predecessor facts.
    // Unsupported history remains unknown, never a fresh empty predecessor.
    for value in previous.into_iter().chain(std::iter::once(&raw)) {
        if [
            "canvas_template_mode",
            "list_template_mode",
            "is_tombstoned",
            "isNonExistent",
            "is_deleted",
            "is_hidden_by_limit",
        ]
        .iter()
        .any(|key| truth(&value[*key]))
            || value
                .get("restriction_type")
                .is_some_and(|v| !v.is_null() && v.as_f64() != Some(0.0))
            || truth(&value["file_access"])
        {
            return None;
        }
        if let Some(mode) = value.get("mode").filter(|v| !v.is_null()) {
            let mode = mode.as_str()?;
            // Source ordinary modes needed by this bounded category adapter;
            // unknown status modes are not interpreted as ordinary files.
            if !matches!(
                mode,
                "" | "hosted"
                    | "external"
                    | "snippet"
                    | "post"
                    | "space"
                    | "docs"
                    | "quip"
                    | "list"
            ) {
                return None;
            }
        }
    }
    effective.extend(fields(incoming));
    effective.insert("is_tombstoned".into(), Value::Bool(false));
    for key in ["mode", "filetype", "subtype", "file_access"] {
        if effective
            .get(key)
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            return None;
        }
    }
    // Only source timestamp truthiness clears isUnknown; isNonExistent input
    // already took the unsupported passthrough branch above.
    if ["updated", "created", "timestamp"]
        .iter()
        .any(|key| truth(&raw[*key]))
    {
        effective.insert("isUnknown".into(), Value::Bool(false));
        effective.insert("isNonExistent".into(), Value::Bool(false));
    }
    if effective.get("isUnknown").is_some_and(truth) {
        return None;
    }
    Some(Value::Object(effective))
}
fn exclusion(row: &Value) -> Option<Exclusion> {
    if not_visible(row) {
        return Some(Exclusion::NotVisible);
    }
    let canvas = row["mode"] == "quip" || row["filetype"] == "quip";
    let list = (row["mode"] == "list" || row["filetype"] == "list") && row["subtype"] != "poll";
    // Fresh id-only rows remain unresolved, not a successful empty category.
    let observed = ["mode", "filetype"]
        .iter()
        .any(|key| row[*key].as_str().is_some_and(|s| !s.is_empty()));
    (observed && !canvas && !list).then_some(Exclusion::NonNative)
}

#[cfg(test)]
mod tests;
