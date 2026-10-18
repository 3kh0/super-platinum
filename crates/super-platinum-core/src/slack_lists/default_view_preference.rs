//! Independent user preference effect after an explicitly reviewed view Save.
//! Source eah+ -> N0Kh -> Nu4c, distinct from lists.views.update/create.
use super::{Document, ListError, ViewRefinementSavePhase};
use serde_json::{Value, json};
#[derive(Clone, Debug, PartialEq)]
pub struct ListDefaultViewPreference {
    document: Document,
    view_id: String,
    original_encoded: String,
    expected: Value,
}
fn unsupported() -> ListError {
    ListError("The saved-view preference cannot be verified.")
}
fn identity(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
fn snapshot(response: &Value) -> Result<(&str, Value), ListError> {
    if response.get("ok") != Some(&Value::Bool(true)) {
        return Err(unsupported());
    }
    let encoded = response
        .pointer("/prefs/lists_default_view")
        .and_then(Value::as_str)
        .ok_or_else(unsupported)?;
    // Actual 8A13 getDefaultViewUserPrefs parses e || "{}". An explicitly
    // observed empty string is known-empty; missing or failed reads still refuse.
    // Retain encoded verbatim so preflight never treats a changed representation
    // as the exact original observation.
    let value = if encoded.is_empty() {
        json!({})
    } else {
        crate::slash_commands::codec::decode(encoded.as_bytes(), 64 * 1024)
            .map_err(|_| unsupported())?
    };
    let map = value.as_object().ok_or_else(unsupported)?;
    // Ordinary List-ID -> view-ID map. Unknown entries remain in the map; an
    // unsupported value shape refuses the complete write instead of dropping it.
    if map.len() > 4096
        || map
            .iter()
            .any(|(k, v)| !identity(k) || !v.as_str().is_some_and(identity))
    {
        return Err(unsupported());
    }
    Ok((encoded, value))
}
impl ListDefaultViewPreference {
    /// The host must independently bind authenticated owner and current Save
    /// observation; this value alone is not account/session/dispatch authority.
    pub fn capture(
        response: &Value,
        document: &Document,
        view_id: &str,
    ) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document
            || !identity(&document.id)
            || !identity(view_id)
        {
            return Err(unsupported());
        }
        let mut views = document.views.iter().filter(|v| v["id"] == view_id);
        let view = views.next().ok_or_else(unsupported)?;
        if views.next().is_some() || !super::ViewRename::supports(view) {
            return Err(unsupported());
        }
        Self::capture_map(response, document, view_id)
    }
    /// Source-equivalent ACK-triggered independent child, before metadata Check.
    /// Consumes the admitted Save response handoff; never invents a created ID.
    /// Host performs a current full-map read and binds the originating Save job.
    pub fn capture_acknowledged(
        response: &Value,
        acknowledgement: super::ViewRefinementAcknowledgement,
    ) -> Result<Self, ListError> {
        let (document, view_id) = acknowledgement.into_parts();
        Self::capture_map(response, &document, &view_id)
    }
    fn capture_map(
        response: &Value,
        document: &Document,
        view_id: &str,
    ) -> Result<Self, ListError> {
        let (encoded, mut expected) = snapshot(response)?;
        if expected.get(&document.id).and_then(Value::as_str) == Some(view_id) {
            return Err(ListError("This view is already the default preference."));
        }
        expected[&document.id] = json!(view_id);
        Ok(Self {
            document: document.clone(),
            view_id: view_id.into(),
            original_encoded: encoded.into(),
            expected,
        })
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    pub fn expected(&self) -> &Value {
        &self.expected
    }
    pub fn revalidate(&self, response: &Value, document: &Document) -> Result<(), ListError> {
        let (encoded, _) = snapshot(response)?;
        if encoded != self.original_encoded
            || Document::parse(document.raw.clone())? != *document
            || !super::cell_edit::document_matches(&self.document.raw, &document.raw)
        {
            return Err(ListError(
                "The view or preference changed. Review it again.",
            ));
        }
        Ok(())
    }
    /// A full independent users.prefs.get observation, never metadata or ACK.
    pub fn observes_desired(&self, response: &Value) -> bool {
        snapshot(response).is_ok_and(|(_, map)| map == self.expected)
    }
}
pub struct ListDefaultViewPreferencePermit {
    intent: ListDefaultViewPreference,
}
impl ListDefaultViewPreferencePermit {
    pub(crate) fn into_intent(self) -> ListDefaultViewPreference {
        self.intent
    }
}
pub struct ListDefaultViewPreferenceAttempt {
    intent: ListDefaultViewPreference,
    phase: ViewRefinementSavePhase,
}
impl ListDefaultViewPreferenceAttempt {
    pub fn new(intent: ListDefaultViewPreference) -> Self {
        Self {
            intent,
            phase: ViewRefinementSavePhase::Reviewing,
        }
    }
    pub fn phase(&self) -> ViewRefinementSavePhase {
        self.phase
    }
    pub fn intent(&self) -> &ListDefaultViewPreference {
        &self.intent
    }
    pub fn admit(
        &mut self,
        response: &Value,
        document: &Document,
    ) -> Result<ListDefaultViewPreferencePermit, ListError> {
        if self.phase != ViewRefinementSavePhase::Reviewing {
            return Err(ListError("This preference attempt was already admitted."));
        }
        self.intent.revalidate(response, document)?;
        self.phase = ViewRefinementSavePhase::InFlight;
        Ok(ListDefaultViewPreferencePermit {
            intent: self.intent.clone(),
        })
    }
    pub fn finish(&mut self, not_requested: bool) -> bool {
        if self.phase != ViewRefinementSavePhase::InFlight {
            return false;
        }
        self.phase = if not_requested {
            ViewRefinementSavePhase::NotRequested
        } else {
            ViewRefinementSavePhase::Unconfirmed
        };
        true
    }
    pub fn observe(&mut self, response: &Value) -> bool {
        if self.phase != ViewRefinementSavePhase::Unconfirmed
            || !self.intent.observes_desired(response)
        {
            return false;
        }
        self.phase = ViewRefinementSavePhase::Observed;
        true
    }
}
#[cfg(test)]
mod tests;
