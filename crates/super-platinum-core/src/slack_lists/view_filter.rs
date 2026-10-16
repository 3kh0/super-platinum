//! Complete bounded Checkbox/Number AND filter replacement for an ordinary saved Table.
//! This authoring boundary is deliberately narrower than saved-view read projection.
use super::{Column, ColumnRename, Document, ListError, PersonalTaskLists, ViewRename};
#[cfg(test)]
mod compound_tests;
mod number;
pub use number::NumberFilterOperator;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewFilterChoice {
    /// Canonical flat 2..=8 condition set; nested sets and None rows refuse.
    All(Vec<ViewFilterChoice>),
    None,
    Checkbox {
        column_id: String,
        checked: bool,
    },
    Number {
        column_id: String,
        operator: NumberFilterOperator,
        value: Option<String>,
    },
}
pub const MAX_FILTER_CONDITIONS: usize = 8;
impl ViewFilterChoice {
    pub fn conditions(&self) -> &[Self] {
        match self {
            Self::None => &[],
            Self::All(rows) => rows,
            _ => std::slice::from_ref(self),
        }
    }
    pub fn from_conditions(mut rows: Vec<Self>) -> Self {
        match rows.len() {
            0 => Self::None,
            1 => rows.pop().unwrap(),
            _ => Self::All(rows),
        }
    }
    pub fn column_id(&self) -> Option<&str> {
        match self {
            Self::Checkbox { column_id, .. } | Self::Number { column_id, .. } => Some(column_id),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ViewFilter {
    document: Document,
    view_id: String,
    desired: ViewFilterChoice,
    filters: Value,
    expected_metadata: Value,
}
fn identity(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
impl ViewFilter {
    /// Borrowed presentation hint. Admission separately checks raw identity and policy.
    pub fn supports_column(column: &Column) -> bool {
        (column.kind == "checkbox" || (column.kind == "number" && ColumnRename::supports(column)))
            && identity(&column.id)
            && identity(&column.key)
    }
    /// None means unsupported, not unfiltered. Missing filters and [] mean unfiltered.
    /// Extra rule members (including optional column_id), null, and oversized sets
    /// stay untouched: no canonical write semantics for those shapes are claimed.
    pub fn current(document: &Document, view: &str) -> Option<ViewFilterChoice> {
        if !identity(view) {
            return None;
        }
        let mut matches = document.views.iter().filter(|v| v["id"] == view);
        let view = matches.next()?;
        if matches.next().is_some() || view["type"] != "table" || !ViewRename::supports(view) {
            return None;
        }
        if view.get("filter_set").is_some_and(|v| !v.is_null())
            || view
                .get("info_column_filters")
                .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return None;
        }
        let Some(value) = view.get("filters") else {
            return Some(ViewFilterChoice::None);
        };
        let rules = value.as_array()?;
        if rules.is_empty() {
            return Some(ViewFilterChoice::None);
        }
        if rules.len() > MAX_FILTER_CONDITIONS {
            return None;
        }
        let mut keys = std::collections::BTreeSet::new();
        let rows = rules
            .iter()
            .map(|value| {
                let row = parse_condition(document, value)?;
                keys.insert(row.column_id()?.to_owned()).then_some(row)
            })
            .collect::<Option<Vec<_>>>()?;
        Some(ViewFilterChoice::from_conditions(rows))
    }
    pub fn capture(
        document: &Document,
        view: &str,
        desired: ViewFilterChoice,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List representations disagree."));
        }
        eligible(document, allow, personal)?;
        let before = Self::current(document, view)
            .ok_or(ListError("This saved Table filter is not supported."))?;
        if before == desired {
            return Err(ListError("Choose a different filter."));
        }
        let rows = desired.conditions();
        if rows.len() > MAX_FILTER_CONDITIONS
            || matches!(&desired, ViewFilterChoice::All(v) if v.len()<2)
        {
            return Err(ListError("Use at most eight filter conditions."));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut values = Vec::with_capacity(rows.len());
        for row in rows {
            if !row.column_id().is_some_and(|id| seen.insert(id)) {
                return Err(ListError("Use each filter field only once."));
            }
            let value = match row {
                ViewFilterChoice::None | ViewFilterChoice::All(_) => {
                    return Err(ListError("Choose a flat set of filter conditions."));
                }
                ViewFilterChoice::Checkbox { column_id, checked } => {
                    let column = document
                        .column(column_id)
                        .filter(|c| c.kind == "checkbox" && Self::supports_column(c))
                        .ok_or(ListError("Choose a supported Checkbox field."))?;
                    json!({ "key": column.key, "operator": if *checked {"is_not_empty"} else {"is_empty"}, "values":[] })
                }
                ViewFilterChoice::Number {
                    column_id,
                    operator,
                    value,
                } => {
                    let column = document
                        .column(column_id)
                        .filter(|c| c.kind == "number" && Self::supports_column(c))
                        .ok_or(ListError("Choose a supported Number field."))?;
                    if !operator.valid_value(value.as_deref()) {
                        return Err(ListError("Enter a valid finite number for this filter."));
                    }
                    let values: Vec<&str> = value.as_deref().into_iter().collect();
                    json!({ "key": column.key, "operator": operator.as_str(), "values": values })
                }
            };
            values.push(value);
        }
        let filters = Value::Array(values);
        if serde_json::to_vec(&filters).map_or(true, |v| v.len() > 8192) {
            return Err(ListError("These filter conditions are too large."));
        }
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let target = expected_metadata["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == view)
            .unwrap();
        target["filters"] = filters.clone();
        // Both arrays are explicit on the wire. Do not normalize absent readback
        // properties into empty arrays when deciding whether the result was observed.
        target["info_column_filters"] = json!([]);
        Ok(Self {
            document: document.clone(),
            view_id: view.into(),
            desired,
            filters,
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
    pub fn desired_filter(&self) -> &ViewFilterChoice {
        &self.desired
    }
    /// The complete replacement vector, including an explicit [] for clearing.
    pub fn filters(&self) -> &Value {
        &self.filters
    }
    pub fn info_column_filters(&self) -> Value {
        json!([])
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
                "This List changed. Cancel and review its filter again.",
            ));
        }
        eligible(fresh, allow, personal)?;
        if Self::current(fresh, &self.view_id).is_none() {
            return Err(ListError("This saved Table filter is no longer supported."));
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
fn parse_condition(document: &Document, value: &Value) -> Option<ViewFilterChoice> {
    let rule = value.as_object()?;
    if rule.len() != 3 {
        return None;
    }
    let key = rule.get("key")?.as_str()?;
    let operator = NumberFilterOperator::parse(rule.get("operator")?.as_str()?)?;
    let values = rule.get("values")?.as_array()?;
    let column = document.columns.iter().find(|c| c.key == key)?;
    if !ViewFilter::supports_column(column) {
        return None;
    }
    if column.kind == "checkbox" {
        if !values.is_empty() {
            return None;
        }
        let checked = match operator {
            NumberFilterOperator::IsEmpty => false,
            NumberFilterOperator::IsNotEmpty => true,
            _ => return None,
        };
        return Some(ViewFilterChoice::Checkbox {
            column_id: column.id.clone(),
            checked,
        });
    }
    let value = match values.as_slice() {
        [] => None,
        [Value::String(value)] => Some(value.as_str()),
        _ => return None,
    };
    operator
        .valid_value(value)
        .then(|| ViewFilterChoice::Number {
            column_id: column.id.clone(),
            operator,
            value: value.map(str::to_owned),
        })
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
pub struct ViewFilterAttempt {
    intent: ViewFilter,
    phase: ViewFilterPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewFilterPhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
/// A consumed, one-use request-construction capability. Not Clone.
pub struct ViewFilterPermit {
    intent: ViewFilter,
}
impl ViewFilterPermit {
    pub(crate) fn into_intent(self) -> ViewFilter {
        self.intent
    }
}
impl ViewFilterAttempt {
    pub fn new(intent: ViewFilter) -> Self {
        Self {
            intent,
            phase: ViewFilterPhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewFilterPhase {
        self.phase
    }
    pub fn intent(&self) -> &ViewFilter {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewFilterPermit, ListError> {
        if self.phase != ViewFilterPhase::Reviewing {
            return Err(ListError(
                "This filter change attempt has already been admitted.",
            ));
        }
        self.intent.revalidate(fresh, allow_lists, personal)?;
        self.phase = ViewFilterPhase::InFlight;
        Ok(ViewFilterPermit {
            intent: self.intent.clone(),
        })
    }
    /// The host may pass true only for typed pre-dispatch NotRequested evidence.
    /// Every received ACK, API denial, timeout or post-dispatch failure stays
    /// unconfirmed until a separate current metadata read observes the filter change.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewFilterPhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewFilterPhase::NotRequested
        } else {
            ViewFilterPhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ViewFilterPhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ViewFilterPhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
