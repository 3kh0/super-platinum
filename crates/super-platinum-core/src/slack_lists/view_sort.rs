//! Complete ordered supported-field sort replacement for an ordinary saved Table.
//! This authoring boundary is deliberately narrower than saved-view read projection.
use super::{Column, ColumnRename, Document, ListError, PersonalTaskLists, ViewRename};
use serde_json::{Value, json};

mod choice;
pub use choice::ViewSortChoice;
// Existing one-attempt transport bounds, not an inferred Slack rule-count limit.
const MAX_SORT_BYTES: usize = 8192;
#[derive(Clone, Debug, PartialEq)]
pub struct ViewSort {
    document: Document,
    view_id: String,
    desired: ViewSortChoice,
    sorts: Value,
    expected_metadata: Value,
}
fn identity(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
impl ViewSort {
    /// Borrowed presentation hint. Admission separately checks raw identity and policy.
    pub fn supports_column(column: &Column) -> bool {
        // Captured Cmmr.Dr permits these kinds. Keep the established Number
        // precision boundary; Select must have the same valid rank table used
        // by saved-view projection. Other preserved options do not affect its
        // existing comparator. Link is absent from Dr; Vote and specialized
        // assignee/To-do metadata need additional projection/context support.
        identity(&column.id)
            && identity(&column.key)
            && match column.kind.as_str() {
                "number" => ColumnRename::supports(column),
                "select" => super::saved_views::supports_new_grouping(column),
                "text" | "checkbox" | "date" | "created_time" | "last_edited_time" | "user"
                | "channel" | "rating" | "email" | "phone" => true,
                _ => false,
            }
    }
    /// None means unsupported, not unsorted. Missing sorts and [] mean unsorted.
    /// Extra rule members (including optional column_id), null, and duplicate rules
    /// stay untouched: no canonical write semantics for those shapes are claimed.
    pub fn current(document: &Document, view: &str) -> Option<ViewSortChoice> {
        if !identity(view) {
            return None;
        }
        let mut matches = document.views.iter().filter(|v| v["id"] == view);
        let view = matches.next()?;
        if matches.next().is_some() || view["type"] != "table" || !ViewRename::supports(view) {
            return None;
        }
        let Some(value) = view.get("sorts") else {
            return Some(ViewSortChoice::None);
        };
        let rules = value.as_array()?;
        if value.to_string().len() > MAX_SORT_BYTES {
            return None;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut choices = Vec::with_capacity(rules.len());
        for raw in rules {
            let rule = raw.as_object()?;
            if rule.len() != 2 {
                return None;
            }
            let key = rule.get("key")?.as_str()?;
            if !seen.insert(key) {
                return None;
            }
            let ascending = rule.get("ascending")?.as_bool()?;
            let column = document.columns.iter().find(|c| c.key == key)?;
            choices.push(Self::choice(column, ascending)?);
        }
        Some(ViewSortChoice::from_rules(choices))
    }
    /// Canonical authoring choice. Number keeps its existing public discriminant.
    pub fn choice(column: &Column, ascending: bool) -> Option<ViewSortChoice> {
        Self::supports_column(column).then(|| {
            if column.kind == "number" {
                ViewSortChoice::Number {
                    column_id: column.id.clone(),
                    ascending,
                }
            } else {
                ViewSortChoice::Field {
                    column_id: column.id.clone(),
                    ascending,
                }
            }
        })
    }
    /// Reject cross-kind aliases before staging or request construction.
    pub fn valid_choice(document: &Document, desired: &ViewSortChoice) -> bool {
        Self::wire(document, desired).is_some()
    }
    pub(super) fn wire(document: &Document, desired: &ViewSortChoice) -> Option<Value> {
        if matches!(desired, ViewSortChoice::Ordered(rows) if rows.len() < 2) {
            return None;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut rules = Vec::new();
        for rule in desired.rules() {
            let (id, ascending) = rule.field()?;
            let column = document.column(id)?;
            if Self::choice(column, ascending).as_ref() != Some(rule) || !seen.insert(&column.key) {
                return None;
            }
            rules.push(json!({"key":column.key,"ascending":ascending}));
        }
        let value = Value::Array(rules);
        (value.to_string().len() <= MAX_SORT_BYTES).then_some(value)
    }

    pub fn capture(
        document: &Document,
        view: &str,
        desired: ViewSortChoice,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List representations disagree."));
        }
        eligible(document, allow, personal)?;
        let before = Self::current(document, view)
            .ok_or(ListError("This saved Table sort is not supported."))?;
        if before == desired {
            return Err(ListError("Choose a different sort."));
        }
        let sorts = Self::wire(document, &desired).ok_or(ListError(
            "Choose distinct supported sort fields within the request limit.",
        ))?;
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let target = expected_metadata["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == view)
            .unwrap();
        target["sorts"] = sorts.clone();
        Ok(Self {
            document: document.clone(),
            view_id: view.into(),
            desired,
            sorts,
            expected_metadata,
        })
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    pub fn desired_sort(&self) -> &ViewSortChoice {
        &self.desired
    }
    /// The complete replacement vector, including an explicit [] for clearing.
    pub fn sorts(&self) -> &Value {
        &self.sorts
    }
    pub fn revalidate(
        &self,
        fresh: &Document,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if Document::parse(fresh.raw.clone())? != *fresh
            || !super::cell_edit::document_matches(&self.document.raw, &fresh.raw)
        {
            return Err(ListError(
                "This List changed. Cancel and review its sort again.",
            ));
        }
        eligible(fresh, allow, personal)?;
        if Self::current(fresh, &self.view_id).is_none() {
            return Err(ListError("This saved Table sort is no longer supported."));
        }
        Ok(())
    }
    /// Full canonical observation, never a mutation ACK or causal attribution.
    pub fn observes_desired(&self, fresh: &Document) -> bool {
        Document::parse(fresh.raw.clone()).is_ok_and(|p| p == *fresh)
            && fresh.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|k| fresh.raw.get(*k) == self.document.raw.get(*k))
            && fresh.raw.get("list_metadata") == Some(&self.expected_metadata)
    }
}
fn eligible(
    doc: &Document,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if let Some(reason) = doc.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&doc.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    Ok(())
}
/// Host-retained operation; not serializable and not independently a durable ledger.
/// Account/session/visit/read/revision/cooldown fences belong to the desktop host.
pub struct ViewSortAttempt {
    intent: ViewSort,
    phase: ViewSortPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewSortPhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct ViewSortPermit {
    intent: ViewSort,
}
impl ViewSortPermit {
    pub(crate) fn into_intent(self) -> ViewSort {
        self.intent
    }
}
impl ViewSortAttempt {
    pub fn new(intent: ViewSort) -> Self {
        Self {
            intent,
            phase: ViewSortPhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewSortPhase {
        self.phase
    }
    pub fn intent(&self) -> &ViewSort {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewSortPermit, ListError> {
        if self.phase != ViewSortPhase::Reviewing {
            return Err(ListError(
                "This sort change attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = ViewSortPhase::InFlight;
        Ok(ViewSortPermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the sort change.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewSortPhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewSortPhase::NotRequested
        } else {
            ViewSortPhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ViewSortPhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ViewSortPhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
