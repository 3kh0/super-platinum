//! Bounded Quip protobuf wire values. Unknown fields retain their exact encoding.
//! This module does not authorize edits, invent missing defaults, or log content.
use std::{collections::HashSet, ops::Range, sync::Arc};

mod schema;
pub use schema::{DocumentData, EditAck, FetchLatest, LoadDataResponse, Section};

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FIELDS: usize = 100_000;
pub const MAX_SECTIONS: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("canvas protobuf exceeds its size limit")]
    TooLarge,
    #[error("canvas protobuf exceeds its field limit")]
    TooManyFields,
    #[error("canvas protobuf is truncated")]
    Truncated,
    #[error("canvas protobuf contains an invalid varint")]
    Varint,
    #[error("canvas protobuf contains an invalid field tag")]
    Tag,
    #[error("canvas protobuf uses an unsupported wire type")]
    WireType,
    #[error("canvas protobuf field has the wrong wire type")]
    WrongType,
    #[error("canvas protobuf has a duplicate singular field")]
    Duplicate,
    #[error("canvas protobuf string is not UTF-8")]
    Utf8,
    #[error("canvas protobuf has an invalid boolean")]
    Boolean,
    #[error("canvas protobuf is missing a required identity")]
    Identity,
    #[error("canvas protobuf update changes its identity")]
    IdentityChanged,
    #[error("canvas protobuf uses an unsupported preserialized payload")]
    Preserialized,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Field {
    tag: u32,
    wire: u8,
    encoded: Range<usize>,
    value: Range<usize>,
}
impl Field {
    pub fn tag(&self) -> u32 {
        self.tag
    }
    pub fn wire_type(&self) -> u8 {
        self.wire
    }
}

/// Clones share immutable bytes. Debug intentionally omits private payloads.
#[derive(Clone)]
pub struct RawMessage {
    data: Arc<[u8]>,
    range: Range<usize>,
    fields: Vec<Field>,
}
impl PartialEq for RawMessage {
    fn eq(&self, other: &Self) -> bool {
        self.raw() == other.raw()
    }
}
impl Eq for RawMessage {}
impl std::fmt::Debug for RawMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawMessage")
            .field("bytes", &self.raw().len())
            .field("fields", &self.fields.len())
            .finish()
    }
}
impl RawMessage {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        Self::parse(Arc::from(bytes), 0..bytes.len())
    }
    pub fn from_fields(fields: &[EncodedField]) -> Result<Self, Error> {
        let empty = Self::decode(&[])?;
        empty.replace_tags(&HashSet::new(), fields.iter().map(|field| field.raw()))
    }
    fn parse(data: Arc<[u8]>, range: Range<usize>) -> Result<Self, Error> {
        let mut fields = Vec::new();
        let mut cursor = range.start;
        while cursor < range.end {
            if fields.len() == MAX_FIELDS {
                return Err(Error::TooManyFields);
            }
            let start = cursor;
            let key = read_varint(&data, &mut cursor, range.end)?;
            let tag = key >> 3;
            if tag == 0 || tag > 0x1fff_ffff {
                return Err(Error::Tag);
            }
            let wire = (key & 7) as u8;
            let value = match wire {
                0 => {
                    let start = cursor;
                    read_varint(&data, &mut cursor, range.end)?;
                    start..cursor
                }
                1 | 5 => {
                    let start = cursor;
                    cursor = cursor
                        .checked_add(if wire == 1 { 8 } else { 4 })
                        .ok_or(Error::TooLarge)?;
                    if cursor > range.end {
                        return Err(Error::Truncated);
                    }
                    start..cursor
                }
                2 => {
                    let length = read_varint(&data, &mut cursor, range.end)?;
                    let length = usize::try_from(length).map_err(|_| Error::TooLarge)?;
                    if length > MAX_BYTES {
                        return Err(Error::TooLarge);
                    }
                    let start = cursor;
                    cursor = cursor.checked_add(length).ok_or(Error::TooLarge)?;
                    if cursor > range.end {
                        return Err(Error::Truncated);
                    }
                    start..cursor
                }
                // Quip's captured schemas contain no groups. Reject rather than
                // dropping unknown group boundaries or recursing without a bound.
                _ => return Err(Error::WireType),
            };
            fields.push(Field {
                tag: tag as u32,
                wire,
                encoded: start..cursor,
                value,
            });
        }
        Ok(Self {
            data,
            range,
            fields,
        })
    }
    pub fn raw(&self) -> &[u8] {
        &self.data[self.range.clone()]
    }
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
    pub fn encoded_field(&self, field: &Field) -> Result<&[u8], Error> {
        // Field handles from another message must not address unrelated bytes.
        if !self.fields.contains(field) {
            return Err(Error::Identity);
        }
        Ok(&self.data[field.encoded.clone()])
    }
    pub fn has(&self, tag: u32) -> bool {
        self.fields.iter().any(|field| field.tag == tag)
    }
    fn single(&self, tag: u32, wire: u8) -> Result<Option<&Field>, Error> {
        let mut found = self.fields.iter().filter(|field| field.tag == tag);
        let Some(field) = found.next() else {
            return Ok(None);
        };
        if found.next().is_some() {
            return Err(Error::Duplicate);
        }
        if field.wire != wire {
            return Err(Error::WrongType);
        }
        Ok(Some(field))
    }
    pub fn bytes(&self, tag: u32) -> Result<Option<&[u8]>, Error> {
        Ok(self
            .single(tag, 2)?
            .map(|field| &self.data[field.value.clone()]))
    }
    pub fn string(&self, tag: u32) -> Result<Option<&str>, Error> {
        self.bytes(tag)?
            .map(|value| std::str::from_utf8(value).map_err(|_| Error::Utf8))
            .transpose()
    }
    pub fn uint(&self, tag: u32) -> Result<Option<u64>, Error> {
        self.single(tag, 0)?
            .map(|field| {
                let mut cursor = field.value.start;
                read_varint(&self.data, &mut cursor, field.value.end)
            })
            .transpose()
    }
    /// Repeated integer fields may mix unpacked and packed protobuf encodings.
    /// Preserve encounter order; bound decoded values even inside one packed field.
    pub fn uints(&self, tag: u32) -> Result<Vec<u64>, Error> {
        let mut values = Vec::new();
        for field in self.fields.iter().filter(|field| field.tag == tag) {
            if !matches!(field.wire, 0 | 2) {
                return Err(Error::WrongType);
            }
            let mut cursor = field.value.start;
            while cursor < field.value.end {
                if values.len() == MAX_FIELDS {
                    return Err(Error::TooManyFields);
                }
                values.push(read_varint(&self.data, &mut cursor, field.value.end)?);
            }
        }
        Ok(values)
    }
    pub fn boolean(&self, tag: u32) -> Result<Option<bool>, Error> {
        self.uint(tag)?
            .map(|value| match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(Error::Boolean),
            })
            .transpose()
    }
    pub fn message(&self, tag: u32) -> Result<Option<Self>, Error> {
        self.single(tag, 2)?
            .map(|field| Self::parse(self.data.clone(), field.value.clone()))
            .transpose()
    }
    pub fn messages(&self, tag: u32) -> Result<Vec<Self>, Error> {
        let mut result = Vec::new();
        let mut total = self.fields.len();
        for field in self.fields.iter().filter(|field| field.tag == tag) {
            if field.wire != 2 {
                return Err(Error::WrongType);
            }
            let child = Self::parse(self.data.clone(), field.value.clone())?;
            total = total
                .checked_add(child.fields.len())
                .ok_or(Error::TooManyFields)?;
            if total > MAX_FIELDS {
                return Err(Error::TooManyFields);
            }
            result.push(child);
        }
        Ok(result)
    }
    /// Replace all occurrences of one tag; untouched fields remain byte-exact.
    pub fn replace_fields(&self, tag: u32, replacements: &[EncodedField]) -> Result<Self, Error> {
        check_tag(tag)?;
        if replacements.iter().any(|field| field.tag != tag) {
            return Err(Error::Tag);
        }
        let tags = HashSet::from([tag]);
        self.replace_tags(
            &tags,
            replacements.iter().map(|field| field.bytes.as_slice()),
        )
    }
    pub fn remove(&self, tag: u32) -> Result<Self, Error> {
        self.replace_fields(tag, &[])
    }
    /// Mechanical partial merge: present tags replace that entire field,
    /// including repeated values. Absent tags remain intact. No nested merge is
    /// inferred; the caller must explicitly project nested attribute updates.
    pub fn merge_partial(&self, update: &Self) -> Result<Self, Error> {
        let tags = update.fields.iter().map(|field| field.tag).collect();
        self.replace_tags(
            &tags,
            update
                .fields
                .iter()
                .map(|field| &update.data[field.encoded.clone()]),
        )
    }
    fn replace_tags<'a>(
        &'a self,
        tags: &HashSet<u32>,
        replacements: impl Iterator<Item = &'a [u8]>,
    ) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        for part in self
            .fields
            .iter()
            .filter(|field| !tags.contains(&field.tag))
            .map(|field| &self.data[field.encoded.clone()])
            .chain(replacements)
        {
            if bytes
                .len()
                .checked_add(part.len())
                .is_none_or(|n| n > MAX_BYTES)
            {
                return Err(Error::TooLarge);
            }
            bytes.extend_from_slice(part);
        }
        Self::decode(&bytes)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct EncodedField {
    tag: u32,
    bytes: Vec<u8>,
}
impl EncodedField {
    pub fn uint(tag: u32, value: u64) -> Result<Self, Error> {
        check_tag(tag)?;
        let mut bytes = Vec::new();
        write_varint(u64::from(tag) << 3, &mut bytes);
        write_varint(value, &mut bytes);
        Ok(Self { tag, bytes })
    }
    pub fn bytes(tag: u32, value: &[u8]) -> Result<Self, Error> {
        check_tag(tag)?;
        if value.len() > MAX_BYTES.saturating_sub(10) {
            return Err(Error::TooLarge);
        }
        let mut bytes = Vec::new();
        write_varint((u64::from(tag) << 3) | 2, &mut bytes);
        write_varint(value.len() as u64, &mut bytes);
        bytes.extend_from_slice(value);
        Ok(Self { tag, bytes })
    }
    pub fn message(tag: u32, value: &RawMessage) -> Result<Self, Error> {
        Self::bytes(tag, value.raw())
    }
    pub fn raw(&self) -> &[u8] {
        &self.bytes
    }
}
fn check_tag(tag: u32) -> Result<(), Error> {
    if tag == 0 || tag > 0x1fff_ffff {
        Err(Error::Tag)
    } else {
        Ok(())
    }
}
fn read_varint(data: &[u8], cursor: &mut usize, end: usize) -> Result<u64, Error> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        if *cursor >= end {
            return Err(Error::Truncated);
        }
        let byte = data[*cursor];
        *cursor += 1;
        if shift == 63 && byte > 1 {
            return Err(Error::Varint);
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err(Error::Varint)
}
fn write_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 128 {
        out.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    out.push(value as u8);
}

#[cfg(test)]
mod tests;
