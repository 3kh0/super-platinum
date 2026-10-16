//! Minimal shared Table/Board creation. There is no client-chosen server ID.
use super::{Document, ListError, PersonalTaskLists};
use serde_json::Value;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewLayout {
    Table,
    Board,
}
impl ViewLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Board => "board",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ViewCreate {
    document: Document,
    name: String,
    layout: ViewLayout,
    grouping: Option<String>,
}
impl ViewCreate {
    /// Borrowed UI hint. Full parsing/policy is required again at capture/admit.
    pub fn supports(document: &Document) -> bool {
        document.raw.pointer("/list_limits/over_view_maximum") == Some(&Value::Bool(false))
            && document
                .raw
                .pointer("/list_metadata/views")
                .is_some_and(Value::is_array)
    }
    pub fn capture(
        document: &Document,
        name: &str,
        layout: ViewLayout,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        Self::capture_with_grouping(document, name, layout, None, allow, personal)
    }
    /// Source new-view dropdown sends only grouping.group_by (the column key).
    /// These borrowed choices are hints; capture/admit validate the full schema.
    pub fn grouping_columns(document: &Document) -> impl Iterator<Item = &super::Column> {
        document.columns.iter().filter(|column| {
            !column.key.trim().is_empty()
                && column.key.len() <= 1024
                && !column.key.chars().any(char::is_control)
                && super::saved_views::supports_new_grouping(column)
        })
    }
    pub fn capture_with_grouping(
        document: &Document,
        name: &str,
        layout: ViewLayout,
        grouping: Option<&str>,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if grouping.is_some_and(|key| !Self::grouping_columns(document).any(|c| c.key == key)) {
            return Err(ListError(
                "Choose an existing select or checkbox field for grouping.",
            ));
        }
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List representations disagree."));
        }
        eligible(document, allow, personal)?;
        if name.trim().is_empty()
            || name.encode_utf16().count() > 60
            || name.chars().any(char::is_control)
        {
            return Err(ListError(
                "Enter a nonempty view name of at most 60 UTF-16 units on one line.",
            ));
        }
        Ok(Self {
            document: document.clone(),
            name: name.into(),
            layout,
            grouping: grouping.map(str::to_owned),
        })
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn desired_name(&self) -> &str {
        &self.name
    }
    pub fn layout(&self) -> ViewLayout {
        self.layout
    }
    pub fn grouping_key(&self) -> Option<&str> {
        self.grouping.as_deref()
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
                "This List changed. Cancel and refresh before adding a view.",
            ));
        }
        eligible(fresh, allow, personal)?;
        if self
            .grouping
            .as_deref()
            .is_some_and(|key| !Self::grouping_columns(fresh).any(|c| c.key == key))
        {
            return Err(ListError("The grouping field changed. Review a new view."));
        }
        Ok(())
    }
    /// Exactly one isolated addition, never causal attribution or ownership.
    /// The host must additionally project this ID with its actual retained population
    /// and context before offering navigation. response.list is not a file read.
    pub fn matching_addition<'a>(&self, fresh: &'a Document) -> Option<&'a str> {
        if Document::parse(fresh.raw.clone()).ok().as_ref() != Some(fresh)
            || fresh.id != self.document.id
            || ["created", "user"]
                .iter()
                .any(|k| fresh.raw.get(*k) != self.document.raw.get(*k))
        {
            return None;
        }
        let baseline = self
            .document
            .raw
            .pointer("/list_metadata/views")?
            .as_array()?;
        let current = fresh.raw.pointer("/list_metadata/views")?.as_array()?;
        if current.len() != baseline.len().checked_add(1)? {
            return None;
        }
        let mut additions = current
            .iter()
            .filter(|v| !baseline.iter().any(|old| old["id"] == v["id"]));
        let added = additions.next()?;
        if additions.next().is_some()
            || added["name"].as_str() != Some(&self.name)
            || added["type"].as_str() != Some(self.layout.as_str())
        {
            return None;
        }
        // Grouped creation has a deliberately exact readback cohort. Do not
        // claim a requested grouping when it is absent, redirected, or enriched
        // with unreviewed grouping settings. Keep that full raw read for review.
        let grouping_matches = match &self.grouping {
            Some(key) => added.get("grouping") == Some(&serde_json::json!({"group_by":key})),
            None => match added.get("grouping") {
                None | Some(Value::Null) => true,
                Some(Value::Object(settings)) => settings.is_empty(),
                _ => false,
            },
        };
        if !grouping_matches {
            return None;
        }
        let id = added["id"].as_str().filter(|s| !s.trim().is_empty())?;
        if !super::ViewRename::supports(added) {
            return None;
        }
        let mut metadata = fresh.raw.get("list_metadata")?.clone();
        metadata["views"] =
            Value::Array(current.iter().filter(|v| v["id"] != id).cloned().collect());
        (Some(&metadata) == self.document.raw.get("list_metadata")).then_some(id)
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
    // Intentional conservative native policy: absent is not assumed false.
    if !ViewCreate::supports(doc) {
        return Err(ListError(
            "The current view limit is not verified as available.",
        ));
    }
    super::saved_views::catalog(doc)
        .map_err(|_| ListError("Saved view identities are unavailable."))?;
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewCreatePhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
}
pub struct ViewCreateAttempt {
    intent: ViewCreate,
    phase: ViewCreatePhase,
}
/// One-use construction capability, deliberately not Clone/Serialize.
pub struct ViewCreatePermit {
    intent: ViewCreate,
}
impl ViewCreatePermit {
    pub(crate) fn into_intent(self) -> ViewCreate {
        self.intent
    }
}
impl ViewCreateAttempt {
    pub fn new(intent: ViewCreate) -> Self {
        Self {
            intent,
            phase: ViewCreatePhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewCreatePhase {
        self.phase
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewCreatePermit, ListError> {
        if self.phase != ViewCreatePhase::Reviewing {
            return Err(ListError("This create attempt has already been admitted."));
        }
        self.intent.revalidate(fresh, allow, personal)?;
        self.phase = ViewCreatePhase::InFlight;
        Ok(ViewCreatePermit {
            intent: self.intent.clone(),
        })
    }
    /// True is valid only for typed pre-dispatch NotRequested evidence. No result
    /// permits rearming. Fresh metadata is separate navigation/review evidence.
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewCreatePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewCreatePhase::NotRequested
        } else {
            ViewCreatePhase::Unconfirmed
        };
        true
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod grouping_tests;
