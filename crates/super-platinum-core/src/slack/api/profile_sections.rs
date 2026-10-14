//! Own-profile Header title editing through Slack's structured sections API.
//! Captured XpEt/O0Pg/16NY preserve typed Header siblings; this is not a
//! users.profile.set title patch or an administrator profile-section editor.
use super::*;
use serde_json::{Value, json};
mod schema;
pub use schema::OwnProfileSections;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct OwnProfileTitleIntent {
    baseline: OwnProfileSections,
    section: Value,
    section_id: String,
    element_id: String,
    desired: String,
    elements: Value,
}
impl OwnProfileTitleIntent {
    pub fn capture(snapshot: &OwnProfileSections, title: &str) -> Result<Self, Error> {
        let capability = snapshot.title_capability();
        if !capability.allowed() {
            return Err(invalid(
                capability.reason().unwrap_or("Title is not editable."),
            ));
        }
        let desired = normalize_profile_title(title)?;
        let (section, target) = snapshot.title_element()?;
        let section_id = required(section, "sectionId")?.to_owned();
        let element_id = required(target, "elementId")?.to_owned();
        let mut elements = schema::convert(section)?;
        let item = elements
            .iter_mut()
            .find(|e| e["element_id"] == element_id)
            .ok_or_else(|| invalid("The title field is missing from this Header."))?;
        item["text"] = json!({"text":desired});
        Ok(Self {
            baseline: snapshot.clone(),
            section: section.clone(),
            section_id,
            element_id,
            desired,
            elements: Value::Array(elements),
        })
    }
    pub fn team_id(&self) -> &str {
        self.baseline.team_id()
    }
    pub fn user_id(&self) -> &str {
        self.baseline.user_id()
    }
    pub fn section_id(&self) -> &str {
        &self.section_id
    }
    pub fn element_id(&self) -> &str {
        &self.element_id
    }
    pub fn desired_title(&self) -> &str {
        &self.desired
    }
    /// Exact Header values, permissions and schema are pinned until dispatch.
    /// Unrelated sections may change; no data from those sections is written.
    pub fn revalidate(&self, fresh: &OwnProfileSections) -> Result<(), Error> {
        if self.team_id() != fresh.team_id() || self.user_id() != fresh.user_id() {
            return Err(invalid("The profile owner changed. Reopen Edit profile."));
        }
        let (section, target) = fresh.title_element()?;
        if section != &self.section || required(target, "elementId")? != self.element_id {
            return Err(invalid(
                "The profile Header changed. Reload before saving the title.",
            ));
        }
        if !fresh.title_capability().allowed() {
            return Err(invalid("The title can no longer be edited."));
        }
        Ok(())
    }
    /// Authoritative, request-bound readback. Sibling edits after acceptance do
    /// not invalidate the fact that this exact title was observed on this owner.
    pub fn confirms_readback(&self, fresh: &OwnProfileSections) -> bool {
        self.team_id() == fresh.team_id()
            && self.user_id() == fresh.user_id()
            && fresh.title_element().is_ok_and(|(section, target)| {
                section.get("sectionId").and_then(Value::as_str) == Some(self.section_id())
                    && target.get("elementId").and_then(Value::as_str) == Some(self.element_id())
                    && schema::text(target, "text").is_ok_and(|s| s == self.desired)
            })
    }
}

/// rguE normalizes ordinary text inputs to NFC; ehbb validates trimmed JS
/// string length against ZOnG.TITLE=249. It does not trim the transmitted value.
pub fn normalize_profile_title(value: &str) -> Result<String, Error> {
    let value = crate::profile_text::normalize(value);
    let trimmed = value.trim_matches(|c| {
        matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}'
        | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}'
        | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    });
    if trimmed.encode_utf16().count() > 249 {
        return Err(invalid("Title must be 249 characters or fewer."));
    }
    Ok(value)
}
pub fn own_profile_sections_request(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    c.rest_form(
        w,
        "users.profile.getSections",
        vec![
            ("user", w.user_id.clone()),
            ("_x_reason", "profiles".into()),
        ],
    )
}
pub async fn fetch_own_profile_sections(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<OwnProfileSections, Error> {
    let value = t.execute(own_profile_sections_request(c, w)).await?;
    OwnProfileSections::from_response(w, &value)
}
pub fn own_profile_title_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &OwnProfileTitleIntent,
    fresh: &OwnProfileSections,
) -> Result<PreparedRequest, Error> {
    if w.team_id != intent.team_id() || w.user_id != intent.user_id() {
        return Err(invalid("The profile request belongs to another account."));
    }
    intent.revalidate(fresh)?;
    // uGgu camelCaseInput:false; qVKj JSON encodes object arrays. `reason`
    // is request metadata and computeMetadata serializes it as `_x_reason`.
    Ok(c.rest_form(
        w,
        "users.profile.setSections",
        vec![
            ("user", w.user_id.clone()),
            ("section", intent.section_id.clone()),
            ("elements", intent.elements.to_string()),
            ("_x_reason", "edit-profile-form-pane".into()),
        ],
    ))
}
pub async fn set_own_profile_title(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &OwnProfileTitleIntent,
    fresh: &OwnProfileSections,
) -> Result<(), Error> {
    t.execute_own_profile_sections(own_profile_title_request(c, w, intent, fresh)?)
        .await
}
fn invalid(message: &str) -> Error {
    Error::Api(message.into())
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, Error> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("Slack returned an incomplete profile section identity."))
}
