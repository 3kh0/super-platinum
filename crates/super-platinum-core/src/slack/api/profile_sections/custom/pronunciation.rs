//! Closed Header text lane, captured O0Pg + /uhB + ehbb/rguE.
//! This does not turn arbitrary Header fields into custom-section controls.
use super::*;
#[cfg(test)]
mod tests;

impl SectionDraft {
    pub fn pronunciation_available(snapshot: &OwnProfileSections) -> bool {
        header_target(snapshot).is_ok()
    }
    pub fn open_pronunciation(snapshot: &OwnProfileSections) -> Result<Self, Error> {
        let (section, target) = header_target(snapshot)?;
        let field=SectionField{
            id:required(target,"elementId")?.into(),
            label:"Name pronunciation".into(),
            hint:"A phonetic spelling or example to help people say your name. Optional; up to 256 characters.".into(),
            editable:target["permissions"]["ui"]==true,
            options:Vec::new(),value:Some(SectionValue::Text(schema::text(target,"text")?.into())),raw:target.clone()
        };
        Ok(Self {
            snapshot: snapshot.clone(),
            section: section.clone(),
            id: required(section, "sectionId")?.into(),
            label: "Name pronunciation".into(),
            fields: vec![field],
            changes: BTreeMap::new(),
            header_pronunciation: true,
        })
    }
    /// Retain the closed editing scope through readback/reopen. Custom open
    /// remains unable to edit Header title, recording or pronunciation.
    pub fn reopen(&self, snapshot: &OwnProfileSections) -> Result<Self, Error> {
        let next = if self.header_pronunciation {
            Self::open_pronunciation(snapshot)?
        } else {
            Self::open(snapshot, self.id())?
        };
        if next.id() != self.id() {
            return Err(invalid("The profile section identity changed."));
        }
        Ok(next)
    }
    pub(super) fn validate_pronunciation(&self, id: &str) -> Result<(), Error> {
        let Some(SectionValue::Text(value)) = self.value(id) else {
            return Err(invalid("Name pronunciation is unavailable."));
        };
        // ehbb: trim for JS UTF-16 length only. rguE normalizes on input;
        // surrounding spaces are retained on the wire. ZOnG maximum is 256.
        if trim_tag_query(value).encode_utf16().count() > 256 {
            return Err(invalid(
                "Name pronunciation must be 256 characters or fewer.",
            ));
        }
        Ok(())
    }
    pub(super) fn capture_pronunciation(&self) -> Result<SectionIntent, Error> {
        let field = &self.fields[0];
        if !field.editable || self.changes.len() != 1 || !self.changes.contains_key(&field.id) {
            return Err(invalid(
                "There is no editable pronunciation change to save.",
            ));
        }
        self.validate_pronunciation(&field.id)?;
        let mut elements = schema::convert(&self.section)?;
        let item = elements
            .iter_mut()
            .find(|e| e["element_id"] == field.id)
            .ok_or_else(|| invalid("Name pronunciation is unavailable."))?;
        *item = self.changes[&field.id].wire(&field.id);
        // O0Pg uses text:{text:""} for clear, and preserves every converted
        // Header sibling. The custom form's empty-to-null pass does NOT apply.
        Ok(SectionIntent {
            draft: self.clone(),
            elements: Value::Array(elements),
        })
    }
}
fn header_target(snapshot: &OwnProfileSections) -> Result<(&Value, &Value), Error> {
    if snapshot.raw()["result"]["data"]["user"]
        .get("isSelf")
        .is_some_and(|v| v != true)
    {
        return Err(invalid("This is not your editable profile."));
    }
    let mut headers = snapshot.sections().iter().filter(|s| s["type"] == "HEADER");
    let section = headers
        .next()
        .ok_or_else(|| invalid("The profile Header is unavailable."))?;
    if headers.next().is_some() {
        return Err(invalid("The profile Header is ambiguous."));
    }
    let elements = section["profileElements"]
        .as_array()
        .ok_or_else(|| invalid("Header fields are unavailable."))?;
    let mut ids = HashSet::new();
    for e in elements {
        if !ids.insert(required(e, "elementId")?) {
            return Err(invalid("Header field identities are ambiguous."));
        }
    }
    let mut targets = elements
        .iter()
        .filter(|e| e["elementKey"] == "Name Pronunciation");
    let target = targets
        .next()
        .ok_or_else(|| invalid("Name pronunciation is not available for this profile."))?;
    if targets.next().is_some() || target["__typename"] != "ProfileTextElement" {
        return Err(invalid("Name pronunciation has an unsupported schema."));
    }
    schema::text(target, "text")?;
    Ok((section, target))
}
