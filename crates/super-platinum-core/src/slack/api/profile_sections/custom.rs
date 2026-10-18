//! Own-profile section editing. B 74An/16NY supplies the complete section wire;
//! this is deliberately separate from the Header title and legacy fields API.
use super::*;
use std::collections::{BTreeMap, HashSet};
mod pronunciation;
mod rich;
pub mod texty;
pub use rich::context::RichTextContext;
mod value;
pub use value::{Country, SectionValue, countries};
#[cfg(test)]
mod tests;

/// Private profile data: intentionally no Debug implementation.
#[derive(Clone, PartialEq)]
pub struct SectionField {
    pub id: String,
    pub label: String,
    pub hint: String,
    pub editable: bool,
    pub options: Vec<String>,
    pub value: Option<SectionValue>,
    raw: Value,
}
#[derive(Clone)]
pub struct SectionDraft {
    snapshot: OwnProfileSections,
    section: Value,
    id: String,
    label: String,
    fields: Vec<SectionField>,
    changes: BTreeMap<String, SectionValue>,
    header_pronunciation: bool,
    rich_fields: BTreeMap<String, RichTextContext>,
}
impl SectionDraft {
    pub fn open(snapshot: &OwnProfileSections, id: &str) -> Result<Self, Error> {
        let section = find(snapshot, id)?;
        if section["id"].as_str().and_then(|s| s.split(':').nth(2)) != Some(id)
            || snapshot.raw()["result"]["data"]["user"]
                .get("isSelf")
                .is_some_and(|v| v != true)
        {
            return Err(invalid(
                "The profile section's editing identity is unavailable.",
            ));
        }

        if section["type"] == "HEADER" {
            return Err(invalid("Use the dedicated profile Header editors."));
        }
        let elements = section["profileElements"]
            .as_array()
            .ok_or_else(|| invalid("Profile fields are unavailable."))?;
        let mut ids = HashSet::new();
        let mut fields = Vec::new();
        for element in elements {
            let id = required(element, "elementId")?.to_owned();
            if !ids.insert(id.clone()) {
                return Err(invalid("Profile field identities are ambiguous."));
            }
            let options = match element.get("options") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(v)) => v
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| invalid("Profile options are malformed."))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(invalid("Profile options are malformed.")),
            };
            let value = SectionValue::read(element).ok();
            let metadata_matches =
                element["id"].as_str().and_then(|s| s.split(':').nth(3)) == Some(id.as_str());
            let editable = metadata_matches
                && section["canEdit"] == true
                && element["permissions"]["ui"] == true
                && value.is_some()
                && element["isScimManaged"] != true
                && !(section["type"] == "CONTACT" && element["elementKey"] == "email");
            fields.push(SectionField {
                id,
                label: schema::text(element, "label")?.to_owned(),
                hint: schema::text(element, "hint")?.to_owned(),
                editable,
                options,
                value,
                raw: element.clone(),
            });
        }
        Ok(Self {
            snapshot: snapshot.clone(),
            section: section.clone(),
            id: id.into(),
            label: schema::text(section, "label")?.to_owned(),
            fields,
            changes: BTreeMap::new(),
            header_pronunciation: false,
            rich_fields: BTreeMap::new(),
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Bind this field's exact ordered channel-token capture. Invalid lengths
    /// remain visible for correction; capture performs the final validation.
    pub fn change_rich(
        &mut self,
        id: &str,
        blocks: Vec<Value>,
        context: RichTextContext,
    ) -> Result<(), Error> {
        if !context.owns(self.snapshot.team_id(), self.snapshot.user_id()) {
            return Err(invalid(
                "This rich-text context belongs to another account.",
            ));
        }
        self.change(id, SectionValue::LongText(blocks))?;
        self.rich_fields.insert(id.to_owned(), context);
        Ok(())
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn fields(&self) -> &[SectionField] {
        &self.fields
    }
    pub fn dirty(&self) -> bool {
        !self.changes.is_empty()
    }
    pub fn changed_person_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for value in self.changes.values() {
            if let SectionValue::Persons(values) = value {
                for id in values {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
            }
        }
        ids
    }
    pub fn value(&self, id: &str) -> Option<&SectionValue> {
        self.changes
            .get(id)
            .or_else(|| self.fields.iter().find(|f| f.id == id)?.value.as_ref())
    }
    /// Keep invalid edits visible for correction; capture performs validation.
    /// Type and permission checks apply immediately, including exact SCIM policy.
    pub fn change(&mut self, id: &str, value: SectionValue) -> Result<(), Error> {
        let value = match value {
            SectionValue::Text(v) if self.header_pronunciation => {
                SectionValue::Text(crate::profile_text::normalize(&v))
            }
            value => value,
        };
        let field = self
            .fields
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| invalid("The profile field is no longer available."))?;
        if !field.editable
            || field
                .value
                .as_ref()
                .is_none_or(|old| old.kind() != value.kind())
        {
            return Err(invalid("This profile field cannot be edited."));
        }
        // A plain edit cannot retain proof from a previous native capture.
        self.rich_fields.remove(id);
        if field.value.as_ref() == Some(&value) {
            self.changes.remove(id);
        } else {
            self.changes.insert(id.into(), value);
        }
        Ok(())
    }
    pub fn validation(&self, id: &str) -> Result<(), Error> {
        if self.header_pronunciation {
            return self.validate_pronunciation(id);
        }
        let field = self
            .fields
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| invalid("The profile field is unavailable."))?;
        if let Some(value) = self.changes.get(id) {
            if let SectionValue::LongText(blocks) = value {
                let originals = match &field.value {
                    Some(SectionValue::LongText(originals)) => originals.as_slice(),
                    _ => &[],
                };
                rich::validate_with(blocks, originals, self.rich_fields.get(id))?;
            } else {
                value.validate(field)?;
            }
        } else if let Some(
            value @ (SectionValue::Text(_)
            | SectionValue::Link { .. }
            | SectionValue::Tags(_)
            | SectionValue::Location { .. }),
        ) = &field.value
        {
            let mut schema = field.clone();
            schema.options.clear();
            value.validate(&schema)?;
        }
        Ok(())
    }
    pub fn capture(&self) -> Result<SectionIntent, Error> {
        if self.header_pronunciation {
            return self.capture_pronunciation();
        }
        if self.section["canEdit"] != true || self.changes.is_empty() {
            return Err(invalid("There is no editable profile change to save."));
        }
        // Formik validates the whole section, including unchanged/read-only
        // text, link, tags and location siblings. Selection membership belongs
        // to changed controls; stale original options are preserved verbatim.
        for field in &self.fields {
            if self.changes.contains_key(&field.id) {
                continue;
            }
            if let Some(
                value @ (SectionValue::Text(_)
                | SectionValue::Link { .. }
                | SectionValue::Tags(_)
                | SectionValue::Location { .. }),
            ) = &field.value
            {
                let mut schema = field.clone();
                schema.options.clear();
                value.validate(&schema)?;
            }
        }
        let mut elements = schema::convert(&self.section)?;
        for (id, value) in &self.changes {
            self.validation(id)?;
            let item = elements
                .iter_mut()
                .find(|e| e["element_id"] == *id)
                .ok_or_else(|| invalid("The changed profile field cannot be represented."))?;
            *item = value.wire(id);
        }
        // 74An explicitly omits Contact email; it has a separate verification flow.
        if self.section["type"] == "CONTACT" {
            elements.retain(|item| {
                self.fields
                    .iter()
                    .any(|f| f.id == item["element_id"] && f.raw["elementKey"] != "email")
            });
        }
        for item in &mut elements {
            normalize_clear(item);
        }
        Ok(SectionIntent {
            draft: self.clone(),
            elements: Value::Array(elements),
        })
    }
}

#[derive(Clone)]
pub struct SectionIntent {
    draft: SectionDraft,
    elements: Value,
}
impl SectionIntent {
    pub fn draft(&self) -> &SectionDraft {
        &self.draft
    }
    pub fn team_id(&self) -> &str {
        self.draft.snapshot.team_id()
    }
    pub fn user_id(&self) -> &str {
        self.draft.snapshot.user_id()
    }
    pub fn section_id(&self) -> &str {
        self.draft.id()
    }
    pub fn revalidate(&self, fresh: &OwnProfileSections) -> Result<(), Error> {
        if self.draft.header_pronunciation {
            // Another distinct-ID Header makes this closed scope ambiguous,
            // even if the previously captured Header itself is unchanged.
            self.draft.reopen(fresh)?;
        }
        if fresh.team_id() != self.team_id()
            || fresh.user_id() != self.user_id()
            || fresh.raw()["result"]["data"]["user"]
                .get("isSelf")
                .is_some_and(|v| v != true)
            || find(fresh, self.section_id())? != &self.draft.section
        {
            return Err(invalid(
                "This profile section changed. Reload before saving.",
            ));
        }
        Ok(())
    }
    pub fn confirms_readback(&self, fresh: &OwnProfileSections) -> bool {
        if fresh.team_id() != self.team_id() || fresh.user_id() != self.user_id() {
            return false;
        }
        let Ok(next) = self.draft.reopen(fresh) else {
            return false;
        };
        self.draft.changes.iter().all(|(id, desired)| {
            next.value(id).is_some_and(|actual| {
                let mut desired = desired.wire(id);
                let mut actual = actual.wire(id);
                if !self.draft.header_pronunciation {
                    normalize_clear(&mut desired);
                    normalize_clear(&mut actual);
                }
                desired == actual
            })
        })
    }
}
fn find<'a>(snapshot: &'a OwnProfileSections, id: &str) -> Result<&'a Value, Error> {
    snapshot
        .sections()
        .iter()
        .find(|s| s["sectionId"] == id)
        .ok_or_else(|| invalid("The profile section is no longer available."))
}
/// Source clears every converted empty sibling, not just the edited target.
fn normalize_clear(item: &mut Value) {
    for (key, member) in [
        ("text", "text"),
        ("date", "date"),
        ("link", "uri"),
        ("long_text", "blocks"),
    ] {
        if item.get(key).is_some_and(|v| v[member] == "") {
            item[key] = Value::Null;
        }
    }
    if item.get("person").is_some_and(|v| {
        v["persons"]
            .as_array()
            .is_some_and(|v| v.iter().all(|v| v == ""))
    }) {
        item["person"] = Value::Null;
    }
    if item
        .get("currentLocation")
        .is_some_and(|v| v.as_object().is_some_and(|v| v.values().all(|v| v == "")))
    {
        item["currentLocation"] = Value::Null;
    }
}
pub fn section_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &SectionIntent,
    fresh: &OwnProfileSections,
) -> Result<PreparedRequest, Error> {
    if w.team_id != intent.team_id() || w.user_id != intent.user_id() {
        return Err(invalid("The profile request belongs to another account."));
    }
    intent.revalidate(fresh)?;
    Ok(c.rest_form(
        w,
        "users.profile.setSections",
        vec![
            ("user", w.user_id.clone()),
            ("section", intent.section_id().into()),
            ("elements", intent.elements.to_string()),
            ("_x_reason", "edit-profile-form-pane".into()),
        ],
    ))
}
pub async fn set_section(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &SectionIntent,
    fresh: &OwnProfileSections,
) -> Result<(), Error> {
    t.execute_own_profile_sections(section_request(c, w, intent, fresh)?)
        .await
}

/// B EditTagsElementForm -> V5QGa, with S uGgu/LYxw snake-case input.
pub fn tags_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    element: &str,
    query: &str,
) -> Result<PreparedRequest, Error> {
    if element.is_empty() || trim_tag_query(query).is_empty() {
        return Err(invalid("A field and tag query are required."));
    }
    Ok(c.rest_form(
        w,
        "search.autocomplete.tags",
        vec![
            ("element_id", element.into()),
            ("query", trim_tag_query(query).into()),
            ("_x_reason", format!("search.autocomplete.tags.{element}")),
        ],
    ))
}
pub async fn fetch_tags(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    element: &str,
    query: &str,
) -> Result<Vec<String>, Error> {
    let response = t.execute(tags_request(c, w, element, query)?).await?;
    decode_tags(&response)
}
/// ECMAScript String.trim from the captured tag-search callback. Rust's
/// is_whitespace additionally removes U+0085 and omits U+FEFF.
pub fn trim_tag_query(query: &str) -> &str {
    query.trim_matches(|c| {
        matches!(c, '\u{9}'..='\u{d}' | '\u{20}' | '\u{a0}' |
        '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' |
        '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}
fn decode_tags(response: &Value) -> Result<Vec<String>, Error> {
    if response["ok"] != true || response.get("error").is_some_and(|e| !e.is_null()) {
        return Err(Error::Api(
            response["error"]
                .as_str()
                .unwrap_or("decode_profile_tags")
                .into(),
        ));
    }
    response["tags"]
        .as_array()
        .ok_or_else(|| invalid("Slack did not return profile tags."))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("A profile tag is malformed."))
        })
        .collect()
}
