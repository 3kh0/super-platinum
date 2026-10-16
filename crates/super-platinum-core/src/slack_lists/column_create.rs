//! Minimal ordinary Text/Number/Checkbox/Date field creation. Request shape is source-backed; the
//! isolated-addition predicate is a conservative observational subset, not a
//! captured server normalization contract or proof that this attempt caused it.
use super::{Column, Document, ListError, PersonalTaskLists, RecordsPage, ViewRename};
use serde_json::{Value, json};

/// Closed creation shapes. This is not a type conversion or arbitrary options API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnCreateKind {
    Text,
    NumberDefault0,
    Checkbox,
    DateSingleDefault,
}
impl ColumnCreateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::NumberDefault0 => "number",
            Self::Checkbox => "checkbox",
            Self::DateSingleDefault => "date",
        }
    }
    /// Exact wire options. Checkbox omits the form member entirely.
    pub fn options(self) -> Option<Value> {
        match self {
            Self::Text => Some(json!({"format":"text"})),
            Self::NumberDefault0 => Some(json!({"precision":0})),
            Self::Checkbox => None,
            Self::DateSingleDefault => Some(json!({"format":"single","date_format":"default"})),
        }
    }
    fn observes_options(self, column: &Column) -> bool {
        match self.options() {
            Some(expected) => column.raw.get("options") == Some(&expected),
            None => column.ordinary_checkbox_without_options(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnCreate {
    document: Document,
    records: RecordsPage,
    view_id: String,
    name: String,
    kind: ColumnCreateKind,
    user_id: String,
}
fn identity(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
impl ColumnCreate {
    /// Borrowed render hint. Fresh parsing, policy and population checks belong
    /// to capture/admit. Owner-only and explicit spare capacity are conservative.
    pub fn supports(document: &Document, view: &str, authenticated_user: &str) -> bool {
        identity(&document.id)
            && identity(view)
            && identity(authenticated_user)
            && (document.raw["access"] == "owner" || document.raw["user"] == authenticated_user)
            && document.raw.pointer("/list_limits/over_column_maximum") == Some(&Value::Bool(false))
            && document
                .views
                .iter()
                .any(|v| v["id"] == view && v["type"] == "table" && ViewRename::supports(v))
    }
    pub fn capture(
        document: &Document,
        records: &RecordsPage,
        view: &str,
        name: &str,
        authenticated_user: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        Self::capture_kind(
            document,
            records,
            view,
            name,
            ColumnCreateKind::Text,
            authenticated_user,
            allow,
            personal,
        )
    }
    /// The explicit type choice is captured once; later UI changes cannot retarget
    /// this intent or a permit already consumed by a request.
    pub fn capture_kind(
        document: &Document,
        records: &RecordsPage,
        view: &str,
        name: &str,
        kind: ColumnCreateKind,
        authenticated_user: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        representations(document, records)?;
        eligible(document, view, authenticated_user, allow, personal)?;
        let name = name.trim();
        if name.is_empty()
            || name.encode_utf16().count() > 256
            || name.chars().any(char::is_control)
        {
            return Err(ListError(match kind {
                ColumnCreateKind::Text => {
                    "Enter a nonempty Text field name on one line, at most 256 UTF-16 units."
                }
                ColumnCreateKind::NumberDefault0 => {
                    "Enter a nonempty Number field name on one line, at most 256 UTF-16 units."
                }
                ColumnCreateKind::Checkbox => {
                    "Enter a nonempty Checkbox field name on one line, at most 256 UTF-16 units."
                }
                ColumnCreateKind::DateSingleDefault => {
                    "Enter a nonempty Date field name on one line, at most 256 UTF-16 units."
                }
            }));
        }
        Ok(Self {
            document: document.clone(),
            records: records.clone(),
            view_id: view.into(),
            name: name.into(),
            kind,
            user_id: authenticated_user.into(),
        })
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn original_records(&self) -> &RecordsPage {
        &self.records
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    pub fn desired_name(&self) -> &str {
        &self.name
    }
    pub fn kind(&self) -> ColumnCreateKind {
        self.kind
    }
    pub fn authenticated_user(&self) -> &str {
        &self.user_id
    }
    pub fn revalidate(
        &self,
        fresh: &Document,
        records: &RecordsPage,
        current_user: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        representations(fresh, records)?;
        if current_user != self.user_id
            || records != &self.records
            || !super::cell_edit::document_matches(&self.document.raw, &fresh.raw)
        {
            return Err(ListError(
                "This List or its loaded items changed. Review a new field before saving.",
            ));
        }
        eligible(fresh, &self.view_id, current_user, allow, personal)
    }
    /// Returns the actual parse-backed identity of one isolated addition.
    /// Fresh files.info ownership and selected-Table projection remain host
    /// responsibilities. No placement, visibility, causation or retry is implied.
    /// An unrelated concurrent row/cursor change deliberately stays unconfirmed.
    pub fn matching_addition<'a>(
        &self,
        fresh: &'a Document,
        records: &RecordsPage,
    ) -> Option<&'a Column> {
        representations(fresh, records).ok()?;
        if fresh.id != self.document.id
            || records != &self.records
            || ["created", "user"]
                .iter()
                .any(|k| fresh.raw.get(*k) != self.document.raw.get(*k))
            || fresh.columns.len() != self.document.columns.len().checked_add(1)?
        {
            return None;
        }
        let mut additions = fresh
            .columns
            .iter()
            .filter(|c| self.document.column(&c.id).is_none());
        let added = additions.next()?;
        if additions.next().is_some()
            || !identity(&added.id)
            || !identity(&added.key)
            || added.name != self.name
            || added.kind != self.kind.as_str()
            || !self.kind.observes_options(added)
            || added
                .raw
                .get("is_primary_column")
                .is_some_and(|v| v != &Value::Bool(false))
            || added.key.eq_ignore_ascii_case("name")
            || fresh.primary_column().map(|c| &c.id)
                != self.document.primary_column().map(|c| &c.id)
        {
            return None;
        }
        // Include property names as well as string values: otherwise an orphan
        // cell, view reference or opaque metadata slot could be revived by reuse.
        let baseline = self.document.raw.get("list_metadata")?;
        if !unreferenced(baseline, &added.id, &added.key)
            || !unreferenced(&self.records.raw, &added.id, &added.key)
        {
            return None;
        }
        let mut metadata = fresh.raw.get("list_metadata")?.clone();
        let schema = metadata.get_mut("schema")?.as_array_mut()?;
        let index = schema.iter().position(|c| c["id"] == added.id)?;
        schema.remove(index);
        let views = metadata.get_mut("views")?.as_array_mut()?;
        // No new view or new columns container normalization is accepted.
        for view in views {
            if let Some(columns) = view.get_mut("columns") {
                let Some(entries) = columns.as_array_mut() else {
                    continue;
                };
                let mut remove = None;
                for (i, entry) in entries.iter().enumerate() {
                    if entry.get("id").and_then(Value::as_str) == Some(&added.id)
                        || entry.get("key").and_then(Value::as_str) == Some(&added.key)
                    {
                        if remove.is_some()
                            || entry.get("id").and_then(Value::as_str) != Some(&added.id)
                            || entry.get("key").and_then(Value::as_str) != Some(&added.key)
                        {
                            return None;
                        }
                        remove = Some(i);
                    }
                }
                if let Some(i) = remove {
                    entries.remove(i);
                }
            }
        }
        (metadata == *baseline).then_some(added)
    }
}
fn representations(document: &Document, records: &RecordsPage) -> Result<(), ListError> {
    if Document::parse(document.raw.clone())? != *document
        || RecordsPage::parse(records.raw.clone())? != *records
    {
        return Err(ListError("List or loaded-item representations disagree."));
    }
    Ok(())
}
fn eligible(
    document: &Document,
    view: &str,
    user: &str,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if let Some(reason) = document.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&document.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    if !ColumnCreate::supports(document, view, user) {
        return Err(ListError(
            "An owned custom Table and available field capacity must be verified.",
        ));
    }
    super::saved_views::catalog(document)
        .map_err(|_| ListError("Saved view identities are unavailable."))?;
    Ok(())
}
/// Iterative, bounded scan. Hitting the budget is uncertainty, never permission.
fn unreferenced(root: &Value, id: &str, key: &str) -> bool {
    let mut pending = vec![root];
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    while let Some(value) = pending.pop() {
        nodes += 1;
        if nodes > 100_000 {
            return false;
        }
        match value {
            Value::String(s) => {
                bytes = bytes.saturating_add(s.len());
                if s == id || s == key {
                    return false;
                }
            }
            Value::Object(map) => {
                if nodes
                    .saturating_add(pending.len())
                    .saturating_add(map.len())
                    > 100_000
                {
                    return false;
                }
                for (k, v) in map {
                    bytes = bytes.saturating_add(k.len());
                    if k == id || k == key {
                        return false;
                    }
                    pending.push(v);
                }
            }
            Value::Array(values) => {
                if nodes
                    .saturating_add(pending.len())
                    .saturating_add(values.len())
                    > 100_000
                {
                    return false;
                }
                pending.extend(values);
            }
            _ => {}
        }
        if bytes > 8 * 1024 * 1024 {
            return false;
        }
    }
    true
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnCreatePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
}
pub struct ColumnCreateAttempt {
    intent: ColumnCreate,
    phase: ColumnCreatePhase,
}
/// Single-use construction capability, deliberately not Clone or Serialize.
pub struct ColumnCreatePermit {
    intent: ColumnCreate,
}
impl ColumnCreatePermit {
    pub(crate) fn into_intent(self) -> ColumnCreate {
        self.intent
    }
}
impl ColumnCreateAttempt {
    pub fn new(intent: ColumnCreate) -> Self {
        Self {
            intent,
            phase: ColumnCreatePhase::Reviewing,
        }
    }
    pub fn intent(&self) -> &ColumnCreate {
        &self.intent
    }
    pub fn phase(&self) -> ColumnCreatePhase {
        self.phase
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        records: &RecordsPage,
        current_user: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ColumnCreatePermit, ListError> {
        if self.phase != ColumnCreatePhase::Reviewing {
            return Err(ListError(
                "This field creation attempt has already been admitted.",
            ));
        }
        self.intent
            .revalidate(fresh, records, current_user, allow, personal)?;
        self.phase = ColumnCreatePhase::InFlight;
        Ok(ColumnCreatePermit {
            intent: self.intent.clone(),
        })
    }
    /// Only typed pre-dispatch evidence may select NotRequested. Neither branch
    /// rearms this attempt; canonical metadata observation is separate evidence.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ColumnCreatePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ColumnCreatePhase::NotRequested
        } else {
            ColumnCreatePhase::Unconfirmed
        };
        true
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod number_tests;

#[cfg(test)]
mod checkbox_tests;

#[cfg(test)]
mod date_tests;
