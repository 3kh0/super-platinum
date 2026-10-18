//! One adjacent custom saved-view move. Exact Ommm.TU position generation;
//! existing catalog ordering, with no raw-array or unrelated metadata rewrites.
use super::{Document, ListError, PersonalTaskLists, ViewRename};
use serde_json::{Value, json};

mod key;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewReorderDirection {
    Up,
    Down,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ViewReorder {
    document: Document,
    view_id: String,
    direction: ViewReorderDirection,
    desired_position: String,
    desired_order: Vec<String>,
    expected_metadata: Value,
}
fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn custom(view: &Value) -> bool {
    view.get("default_view_key")
        .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
        && view
            .get("is_all_items_view")
            .is_none_or(|v| v == &Value::Bool(false))
}
fn switcher_catalog(document: &Document) -> Result<Vec<super::saved_views::SavedView>, ListError> {
    // OCwX.q and the captured initial/update pipeline exclude these known views
    // before switcher sorting. They need neither a switcher name nor position.
    // This is a temporary read projection; the intent retains the original raw
    // document and computes its sole target-position change against that raw.
    let mut raw = document.raw.clone();
    raw["list_metadata"]["views"] = Value::Array(
        document
            .views
            .iter()
            .filter(|view| !matches!(view["type"].as_str(), Some("record" | "canvas_inline")))
            .cloned()
            .collect(),
    );
    let switcher = Document::parse(raw)?;
    super::saved_views::catalog(&switcher)
        .map_err(|_| ListError("Saved view identities or positions are unsupported."))
}
impl ViewReorder {
    /// Closed key domain, useful to the strict request validator. This is not a
    /// dispatch capability and does not establish adjacent move eligibility.
    pub fn supports_position(position: &str) -> bool {
        key::valid(position)
    }
    /// Existing displayed custom-view order. Built-in views remain separate;
    /// malformed or legacy positions are never rewritten or coerced.
    pub fn current_order(document: &Document) -> Result<Vec<String>, ListError> {
        if Document::parse(document.raw.clone())? != *document {
            return Err(ListError("List schema representations disagree."));
        }
        // Filtering must not hide a collision with the id sent to the server.
        let mut identities = std::collections::BTreeSet::new();
        for view in &document.views {
            if view
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !identities.insert(id))
            {
                return Err(ListError("Saved view identities are ambiguous."));
            }
        }
        let catalog = switcher_catalog(document)?;
        let mut positions = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for view in catalog {
            if !identity(&view.id) {
                return Err(ListError("Saved view identity is unsupported."));
            }
            // Other unknown native layouts cannot become ordering neighbors.
            if !matches!(view.kind.as_str(), "table" | "board") {
                return Err(ListError(
                    "This saved view catalog includes an unsupported layout.",
                ));
            }
            if view
                .raw
                .get("default_view_key")
                .is_some_and(|v| !v.is_null() && !v.is_string())
                || view
                    .raw
                    .get("is_all_items_view")
                    .is_some_and(|v| !v.is_boolean())
            {
                return Err(ListError("Built-in view identity is malformed."));
            }
            let Some(position) = view.raw.get("position").and_then(Value::as_str) else {
                return Err(ListError(
                    "Saved view positions require canonical string keys.",
                ));
            };
            if !key::valid(position) || !positions.insert(position.to_owned()) {
                return Err(ListError(
                    "Saved view positions are malformed or ambiguous.",
                ));
            }
            if custom(&view.raw) {
                out.push(view.id);
            }
        }
        Ok(out)
    }
    /// Borrowed presentation hint, excluding boundaries and unsupported targets.
    /// Fresh full policy and raw metadata must still pass admission.
    pub fn choices(document: &Document, view: &str) -> Vec<ViewReorderDirection> {
        [ViewReorderDirection::Up, ViewReorderDirection::Down]
            .into_iter()
            .filter(|direction| Self::planned(document, view, *direction).is_ok())
            .collect()
    }
    fn planned(
        document: &Document,
        view: &str,
        direction: ViewReorderDirection,
    ) -> Result<(String, Vec<String>), ListError> {
        let target = document
            .views
            .iter()
            .find(|v| v["id"] == view)
            .ok_or(ListError("The saved view is unavailable."))?;
        if !ViewRename::supports(target) {
            return Err(ListError(
                "Move an ordinary unlocked custom Table or Board view.",
            ));
        }
        let mut order = Self::current_order(document)?;
        let index = order
            .iter()
            .position(|id| id == view)
            .ok_or(ListError("Built-in views cannot be reordered."))?;
        let next = match direction {
            ViewReorderDirection::Up => index.checked_sub(1),
            ViewReorderDirection::Down => (index + 1 < order.len()).then_some(index + 1),
        }
        .ok_or(ListError("This saved view is already at that boundary."))?;
        // The source's menu boundary is custom-only, but its et helper takes
        // neighbors from the complete sorted view vector. Built-in bounds count.
        let mut full = switcher_catalog(document)?;
        let full_index = full.iter().position(|v| v.id == view).unwrap();
        let full_next = full.iter().position(|v| v.id == order[next]).unwrap();
        if full_index.abs_diff(full_next) != 1 {
            return Err(ListError(
                "Interleaved built-in view positions cannot be moved safely.",
            ));
        }
        full.swap(full_index, full_next);
        order.swap(index, next);
        let position = |index: usize| full[index].raw["position"].as_str().unwrap();
        let left = full_next.checked_sub(1).map(position);
        let right = (full_next + 1 < full.len()).then(|| position(full_next + 1));
        let desired = key::between(left, right)?;
        Ok((desired, order))
    }
    pub fn capture(
        document: &Document,
        view: &str,
        direction: ViewReorderDirection,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        eligible(document, allow, personal)?;
        let (desired_position, desired_order) = Self::planned(document, view, direction)?;
        let mut expected_metadata = document.raw["list_metadata"].clone();
        let target = expected_metadata["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == view)
            .unwrap();
        target["position"] = json!(desired_position);
        Ok(Self {
            document: document.clone(),
            view_id: view.into(),
            direction,
            desired_position,
            desired_order,
            expected_metadata,
        })
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    pub fn direction(&self) -> ViewReorderDirection {
        self.direction
    }
    pub fn desired_position(&self) -> &str {
        &self.desired_position
    }
    pub fn desired_order(&self) -> &[String] {
        &self.desired_order
    }
    pub fn original(&self) -> &Document {
        &self.document
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
                "This List changed. Review the saved view order again.",
            ));
        }
        eligible(fresh, allow, personal)?;
        let (position, order) = Self::planned(fresh, &self.view_id, self.direction)?;
        if position != self.desired_position || order != self.desired_order {
            return Err(ListError("The reviewed saved view order changed."));
        }
        Ok(())
    }
    /// Observation of state, never attribution to this attempt. Preserve full
    /// metadata including raw views array order and all unknown nested members.
    pub fn observes_desired(&self, fresh: &Document) -> bool {
        Document::parse(fresh.raw.clone()).is_ok_and(|parsed| parsed == *fresh)
            && fresh.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|key| fresh.raw.get(*key) == self.document.raw.get(*key))
            && fresh.raw.get("list_metadata") == Some(&self.expected_metadata)
            && Self::current_order(fresh).is_ok_and(|order| order == self.desired_order)
    }
}
fn eligible(
    document: &Document,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if !identity(&document.id) {
        return Err(ListError("The List identity is unsupported."));
    }
    if let Some(reason) = document.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&document.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    Ok(())
}

/// Host retains account/visit/revision guards. This capability is one-use and
/// deliberately non-serializable; a lost ACK cannot manufacture another permit.
pub struct ViewReorderAttempt {
    intent: ViewReorder,
    phase: ViewReorderPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewReorderPhase {
    Reviewing,
    InFlight,
    NotRequested,
    Unconfirmed,
    Observed,
}
pub struct ViewReorderPermit {
    intent: ViewReorder,
}
impl ViewReorderPermit {
    pub(crate) fn into_intent(self) -> ViewReorder {
        self.intent
    }
}
impl ViewReorderAttempt {
    pub fn new(intent: ViewReorder) -> Self {
        Self {
            intent,
            phase: ViewReorderPhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewReorderPhase {
        self.phase
    }
    pub fn intent(&self) -> &ViewReorder {
        &self.intent
    }
    pub fn admit(
        &mut self,
        fresh: &Document,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewReorderPermit, ListError> {
        if self.phase != ViewReorderPhase::Reviewing {
            return Err(ListError("This saved view move has already been admitted."));
        }
        self.intent.revalidate(fresh, allow, personal)?;
        self.phase = ViewReorderPhase::InFlight;
        Ok(ViewReorderPermit {
            intent: self.intent.clone(),
        })
    }
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewReorderPhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewReorderPhase::NotRequested
        } else {
            ViewReorderPhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, fresh: &Document) -> bool {
        if self.phase != ViewReorderPhase::Unconfirmed || !self.intent.observes_desired(fresh) {
            return false;
        }
        self.phase = ViewReorderPhase::Observed;
        true
    }
}

#[cfg(test)]
mod tests;
