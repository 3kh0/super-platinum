//! Named fields from captured Quip `p+Jj` descriptors. No implicit write policy.
use super::{EncodedField, Error, MAX_FIELDS, MAX_SECTIONS, RawMessage};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    raw: RawMessage,
}
impl Section {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        Self::from_raw(RawMessage::decode(bytes)?)
    }
    pub fn from_raw(raw: RawMessage) -> Result<Self, Error> {
        identity(raw.string(1)?)?;
        for tag in [2, 9, 10, 18, 20, 39] {
            raw.uint(tag)?;
        }
        for tag in [3, 19, 35, 36] {
            raw.boolean(tag)?;
        }
        for tag in [6, 7, 8, 21, 33] {
            raw.string(tag)?;
        }
        for tag in [12, 13, 15, 16, 38] {
            raw.message(tag)?;
        }
        if let Some(attrs) = raw.message(16)? {
            attrs.boolean(2)?;
        }
        Ok(Self { raw })
    }
    pub fn raw(&self) -> &RawMessage {
        &self.raw
    }
    pub fn id(&self) -> &str {
        self.raw
            .string(1)
            .expect("validated identity")
            .expect("validated identity")
    }
    pub fn sequence(&self) -> Result<Option<u64>, Error> {
        self.raw.uint(2)
    }
    pub fn thread_id(&self) -> Result<Option<&str>, Error> {
        self.raw.string(6)
    }
    pub fn document_id(&self) -> Result<Option<&str>, Error> {
        self.raw.string(7)
    }
    pub fn section_type(&self) -> Result<Option<u64>, Error> {
        self.raw.uint(9)
    }
    pub fn style(&self) -> Result<Option<u64>, Error> {
        self.raw.uint(10)
    }
    pub fn deleted(&self) -> Result<Option<bool>, Error> {
        self.raw.boolean(3)
    }
    pub fn read_only(&self) -> Result<Option<bool>, Error> {
        self.raw.boolean(35)
    }
    pub fn attributes(&self) -> Result<Option<RawMessage>, Error> {
        self.raw.message(16)
    }
    pub fn status(&self) -> Result<Option<RawMessage>, Error> {
        self.raw.message(15)
    }
    pub fn parents(&self) -> Result<Option<RawMessage>, Error> {
        self.raw.message(13)
    }
    pub fn crdt(&self) -> Result<Option<RawMessage>, Error> {
        self.raw.message(38)
    }
    pub fn checked(&self) -> Result<Option<bool>, Error> {
        self.attributes()?
            .map(|attrs| attrs.boolean(2))
            .transpose()
            .map(Option::flatten)
    }
    /// Merge a canonical partial by field presence, not protobuf defaults.
    /// This is mechanical merging; callers must enforce revision ordering and
    /// reject stale/concurrent receipts before adopting the result.
    pub fn merge_partial(&self, update: &Self) -> Result<Self, Error> {
        if self.id() != update.id() {
            return Err(Error::IdentityChanged);
        }
        for tag in [6, 7] {
            if let (Some(a), Some(b)) = (self.raw.string(tag)?, update.raw.string(tag)?) {
                if a != b {
                    return Err(Error::IdentityChanged);
                }
            }
        }
        Self::from_raw(self.raw.merge_partial(&update.raw)?)
    }
}
fn identity(value: Option<&str>) -> Result<&str, Error> {
    value
        .filter(|value| {
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        })
        .ok_or(Error::Identity)
}
fn sections(raw: &RawMessage, tag: u32) -> Result<Vec<Section>, Error> {
    let messages = raw.messages(tag)?;
    if messages.len() > MAX_SECTIONS {
        return Err(Error::TooManyFields);
    }
    let mut ids = HashSet::new();
    let mut result = Vec::with_capacity(messages.len());
    for message in messages {
        let section = Section::from_raw(message)?;
        if !ids.insert(section.id().to_owned()) {
            return Err(Error::Duplicate);
        }
        result.push(section);
    }
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentData {
    raw: RawMessage,
    sections: Vec<Section>,
}
impl DocumentData {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        Self::from_raw(RawMessage::decode(bytes)?)
    }
    pub fn from_raw(raw: RawMessage) -> Result<Self, Error> {
        raw.string(15)?;
        raw.boolean(3)?;
        let sections = sections(&raw, 1)?;
        Ok(Self { raw, sections })
    }
    pub fn raw(&self) -> &RawMessage {
        &self.raw
    }
    pub fn document_id(&self) -> Result<Option<&str>, Error> {
        self.raw.string(15)
    }
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }
    /// Section collections merge by identity; a one-section acknowledgement
    /// never erases unrelated sections. Explicit tombstones remain represented.
    pub fn merge_partial(&self, update: &Self) -> Result<Self, Error> {
        if let (Some(a), Some(b)) = (self.document_id()?, update.document_id()?) {
            if a != b {
                return Err(Error::IdentityChanged);
            }
        }
        let mut merged = self.sections.clone();
        let mut positions: HashMap<String, usize> = merged
            .iter()
            .enumerate()
            .map(|(i, section)| (section.id().to_owned(), i))
            .collect();
        for section in &update.sections {
            if let Some(&index) = positions.get(section.id()) {
                merged[index] = merged[index].merge_partial(section)?;
            } else {
                positions.insert(section.id().to_owned(), merged.len());
                merged.push(section.clone());
            }
        }
        if merged.len() > MAX_SECTIONS {
            return Err(Error::TooManyFields);
        }
        let fields = merged
            .iter()
            .map(|section| EncodedField::message(1, section.raw()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_raw(
            self.raw
                .merge_partial(&update.raw)?
                .replace_fields(1, &fields)?,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditAck {
    raw: RawMessage,
    pub changed: Option<bool>,
    pub final_data: Option<DocumentData>,
    pub cell_data: Option<RawMessage>,
    pub import_sections: Option<DocumentData>,
    pub mount_points: Option<RawMessage>,
    pub is_new_edit_session: Option<bool>,
}
impl EditAck {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let raw = RawMessage::decode(bytes)?;
        let result = Self {
            changed: raw.boolean(1)?,
            final_data: raw.message(2)?.map(DocumentData::from_raw).transpose()?,
            cell_data: raw.message(3)?,
            import_sections: raw.message(4)?.map(DocumentData::from_raw).transpose()?,
            mount_points: raw.message(5)?,
            is_new_edit_session: raw.boolean(6)?,
            raw,
        };
        // Two independently bounded documents must also fit the aggregate
        // field budget. Byte ownership remains shared with the acknowledgement.
        let total = result
            .final_data
            .iter()
            .chain(result.import_sections.iter())
            .map(|doc| {
                doc.raw.fields().len()
                    + doc
                        .sections
                        .iter()
                        .map(|s| s.raw.fields().len())
                        .sum::<usize>()
            })
            .sum::<usize>();
        if total > MAX_FIELDS {
            return Err(Error::TooManyFields);
        }
        Ok(result)
    }
    pub fn raw(&self) -> &RawMessage {
        &self.raw
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadDataResponse {
    raw: RawMessage,
    response: Option<RawMessage>,
    preserialized: Option<RawMessage>,
}
impl LoadDataResponse {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let raw = RawMessage::decode(bytes)?;
        raw.string(1)?;
        let response = raw.message(2)?;
        if let Some(response) = &response {
            response.boolean(1)?;
            response.message(2)?;
        }
        let preserialized = raw.message(3)?;
        Ok(Self {
            raw,
            response,
            preserialized,
        })
    }
    pub fn raw(&self) -> &RawMessage {
        &self.raw
    }
    pub fn error(&self) -> Result<Option<&str>, Error> {
        self.raw.string(1)
    }
    pub fn response(&self) -> Option<&RawMessage> {
        self.response.as_ref()
    }
    pub fn success(&self) -> Result<Option<bool>, Error> {
        self.response
            .as_ref()
            .map(|response| response.boolean(1))
            .transpose()
            .map(Option::flatten)
    }
    pub fn preserialized(&self) -> Option<&RawMessage> {
        self.preserialized.as_ref()
    }
    /// Nonempty preserialized items need their own captured type dispatch. Do
    /// not silently hand a caller an incomplete canonical payload.
    pub fn payload(&self) -> Result<Option<RawMessage>, Error> {
        if self
            .preserialized
            .as_ref()
            .is_some_and(|raw| !raw.fields().is_empty())
        {
            return Err(Error::Preserialized);
        }
        self.response
            .as_ref()
            .map(|response| response.message(2))
            .transpose()
            .map(Option::flatten)
    }
    pub fn sections(&self) -> Result<Vec<Section>, Error> {
        self.payload()?
            .map(|payload| sections(&payload, 7))
            .transpose()
            .map(Option::unwrap_or_default)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchLatest {
    raw: RawMessage,
    changes: Option<RawMessage>,
}
impl FetchLatest {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let raw = RawMessage::decode(bytes)?;
        let changes = raw.message(1)?;
        Ok(Self { raw, changes })
    }
    pub fn raw(&self) -> &RawMessage {
        &self.raw
    }
    pub fn changes(&self) -> Option<&RawMessage> {
        self.changes.as_ref()
    }
    pub fn payload(&self) -> Result<Option<RawMessage>, Error> {
        self.changes
            .as_ref()
            .map(|changes| changes.message(7))
            .transpose()
            .map(Option::flatten)
    }
    pub fn sections(&self) -> Result<Vec<Section>, Error> {
        self.payload()?
            .map(|payload| sections(&payload, 7))
            .transpose()
            .map(Option::unwrap_or_default)
    }
}
