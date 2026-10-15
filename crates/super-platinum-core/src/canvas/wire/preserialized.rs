//! Conservative LoadData wrapper3 SECTION projection. Current shipped nEcU.Wf
//! joins sidecars only to preserialized Sections, then appends them to Payload7.
//! Original wrapper/response/items remain untouched in LoadDataResponse.
use super::{EncodedField, Error, MAX_BYTES, MAX_FIELDS, MAX_SECTIONS, RawMessage, Section};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
struct Limits {
    bytes: usize,
    fields: usize,
    sections: usize,
}
const LIMITS: Limits = Limits {
    bytes: MAX_BYTES,
    fields: MAX_FIELDS,
    sections: MAX_SECTIONS,
};
struct Budget {
    limits: Limits,
    bytes: usize,
    fields: usize,
}
impl Budget {
    fn fields(&mut self, count: usize) -> Result<(), Error> {
        self.fields = self.fields.checked_add(count).ok_or(Error::TooManyFields)?;
        if self.fields > self.limits.fields {
            return Err(Error::TooManyFields);
        }
        Ok(())
    }
    fn extra_bytes(&mut self, count: usize) -> Result<(), Error> {
        self.bytes = self.bytes.checked_add(count).ok_or(Error::TooLarge)?;
        if self.bytes > self.limits.bytes {
            return Err(Error::TooLarge);
        }
        Ok(())
    }
}
fn known(raw: &RawMessage, tags: &[u32]) -> Result<(), Error> {
    if raw
        .fields()
        .iter()
        .any(|field| !tags.contains(&field.tag()))
    {
        return Err(Error::Preserialized);
    }
    Ok(())
}
fn session(raw: &RawMessage, budget: &mut Budget) -> Result<String, Error> {
    budget.fields(raw.fields().len())?;
    let id = raw
        .string(1)?
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
        .ok_or(Error::Identity)?;
    for tag in [2, 3, 4] {
        raw.uint(tag)?;
    }
    raw.bytes(5)?;
    raw.boolean(7)?;
    for tag in [8, 12] {
        raw.string(tag)?;
    }
    for field in raw.fields().iter().filter(|field| field.tag() == 6) {
        if field.wire_type() != 2 {
            return Err(Error::WrongType);
        }
        std::str::from_utf8(&raw.data[field.value.clone()]).map_err(|_| Error::Utf8)?;
    }
    if let Some(merge) = raw.message(9)? {
        budget.fields(merge.fields().len())?;
    }
    for tag in [10, 11] {
        for child in raw.messages(tag)? {
            budget.fields(child.fields().len())?;
        }
    }
    Ok(id.to_owned())
}
fn admit_section(
    raw: RawMessage,
    budget: &mut Budget,
    ids: &mut HashSet<String>,
) -> Result<Section, Error> {
    budget.fields(raw.fields().len())?;
    let section = Section::from_raw(raw)?;
    if ids.len() == budget.limits.sections {
        return Err(Error::TooManyFields);
    }
    if !ids.insert(section.id().to_owned()) {
        return Err(Error::Duplicate);
    }
    for tag in [12, 13, 15, 16] {
        if let Some(child) = section.raw().message(tag)? {
            budget.fields(child.fields().len())?;
        }
    }
    if let Some(crdt) = section.crdt()? {
        if session(&crdt, budget)? != section.id() {
            return Err(Error::IdentityChanged);
        }
    }
    Ok(section)
}

pub(super) fn project(
    wrapper: &RawMessage,
    response: Option<&RawMessage>,
    items: &RawMessage,
) -> Result<Option<RawMessage>, Error> {
    project_bounded(wrapper, response, items, LIMITS)
}
fn project_bounded(
    wrapper: &RawMessage,
    response: Option<&RawMessage>,
    items: &RawMessage,
    limits: Limits,
) -> Result<Option<RawMessage>, Error> {
    // An orphan wrapper3 must not manufacture a response that was absent.
    let response = response.ok_or(Error::Preserialized)?;
    known(items, &[1, 2])?;
    let payload = response.message(2)?;
    let mut budget = Budget {
        limits,
        bytes: 0,
        fields: 0,
    };
    budget.extra_bytes(wrapper.raw().len())?;
    for raw in [wrapper, response, items] {
        budget.fields(raw.fields().len())?;
    }
    let mut ids = HashSet::new();
    if let Some(payload) = &payload {
        budget.fields(payload.fields().len())?;
        for raw in payload.messages(7)? {
            admit_section(raw, &mut budget, &mut ids)?;
        }
    }
    let mut sessions = HashMap::new();
    for raw in items.messages(2)? {
        let id = session(&raw, &mut budget)?;
        if sessions.len() == limits.sections {
            return Err(Error::TooManyFields);
        }
        if sessions.insert(id, raw).is_some() {
            return Err(Error::Duplicate);
        }
    }
    let mut appended = Vec::new();
    for record in items.messages(1)? {
        budget.fields(record.fields().len())?;
        known(&record, &[1, 2])?;
        // Stock IdType.SECTION == 2; this is not Payload's sections tag7.
        if record.uint(1)? != Some(2) {
            return Err(Error::Preserialized);
        }
        for raw in record.messages(2)? {
            let section = admit_section(raw, &mut budget, &mut ids)?;
            let mut raw = section.raw().clone();
            if let Some(sidecar) = sessions.remove(section.id()) {
                if let Some(inline) = section.crdt()? {
                    if inline.raw() != sidecar.raw() {
                        return Err(Error::Preserialized);
                    }
                } else {
                    let field = EncodedField::message(38, &sidecar)?;
                    // Bound retained input plus expansion, not just each child.
                    budget.extra_bytes(field.raw().len())?;
                    budget.fields(1 + sidecar.fields().len())?;
                    raw = raw.replace_fields(38, &[field])?;
                }
            }
            appended.push(EncodedField::message(7, &raw)?);
            budget.fields(1)?;
        }
    }
    // Stock ignores orphans; this bounded subset cannot prove their association.
    if !sessions.is_empty() {
        return Err(Error::Preserialized);
    }
    if appended.is_empty() {
        return Ok(payload);
    }
    // Append without regrouping tag7: every original Payload field, including
    // interleaved unknown fields and nonminimal encodings, stays in place.
    let mut bytes = payload
        .as_ref()
        .map(|p| p.raw().to_vec())
        .unwrap_or_default();
    for field in appended {
        if bytes
            .len()
            .checked_add(field.raw().len())
            .is_none_or(|n| n > limits.bytes)
        {
            return Err(Error::TooLarge);
        }
        bytes.extend_from_slice(field.raw());
    }
    Ok(Some(RawMessage::decode(&bytes)?))
}

#[cfg(test)]
mod tests;
