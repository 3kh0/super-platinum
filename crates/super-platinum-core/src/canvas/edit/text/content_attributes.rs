//! Ordinary web CONTENT projection for already captured flat H1-H3 hosts.
//! Copy original attribute wire bytes; this grants no attribute editing authority.
use super::{Error, RawMessage, Section};

pub(in crate::canvas::edit) fn heading_content_attributes(
    section: &Section,
) -> Result<Option<RawMessage>, Error> {
    if section.section_type()?.unwrap_or(0) != 0
        || !(1..=3).contains(&section.style()?.unwrap_or(0))
    {
        return Ok(None);
    }
    let Some(attributes) = section.attributes()? else {
        return Ok(None);
    };
    if attributes.string(29)?.is_some_and(|id| !id.is_empty()) {
        Ok(Some(attributes))
    } else {
        Ok(None)
    }
}
