//! Positive response evidence authorizes only the source's independent child
//! preference effect. It does not confirm metadata, navigate, or clear drafts.
use super::*;
/// Single-use child handoff. Not Clone or serializable; the host must also bind
/// the admitted Save job, Scope/account identity and current local draft.
pub struct ViewRefinementAcknowledgement {
    original: Document,
    view_id: String,
}
impl ViewRefinementAcknowledgement {
    pub fn list_id(&self) -> &str {
        &self.original.id
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    pub(crate) fn into_parts(self) -> (Document, String) {
        (self.original, self.view_id)
    }
    pub(super) fn capture(intent: &ViewRefinementSave, reply: &Value) -> Option<Self> {
        if reply.get("ok") != Some(&Value::Bool(true))
            || reply.get("error").is_some_and(|v| !v.is_null())
        {
            return None;
        }
        let id = match &intent.destination {
            // Actual 0Kzd dispatches the preference before inspecting H.list.
            ViewRefinementDestination::Existing => intent.draft.view_id().to_owned(),
            ViewRefinementDestination::New { name } => {
                let raw = reply.get("list")?;
                let acknowledged = Document::parse(raw.clone()).ok()?;
                if acknowledged.id != intent.document.id {
                    return None;
                }
                // Actual aV chooses the final ACK entry BEFORE filtering/sorting.
                // Native admission additionally rejects foreign/reused identities
                // and a last entry inconsistent with the requested ordinary view.
                // No other ACK metadata becomes an observed canonical snapshot.
                let last = acknowledged.views.last()?;
                let id = last["id"].as_str()?;
                if id.trim().is_empty()
                    || id.len() > 1024
                    || id.chars().any(char::is_control)
                    || intent.document.views.iter().any(|v| {
                        v["id"]
                            .as_str()
                            .is_some_and(|old| old.to_lowercase() == id.to_lowercase())
                    })
                    || last["type"] != intent.expected_view["type"]
                    || last["name"].as_str() != Some(name.as_str())
                {
                    return None;
                }
                id.to_owned()
            }
        };
        Some(Self {
            original: intent.document.clone(),
            view_id: id,
        })
    }
}

#[cfg(test)]
mod tests;
