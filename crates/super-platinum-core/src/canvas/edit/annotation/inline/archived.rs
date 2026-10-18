//! Explicit renderer-selected hidden wrappers; v4 remains archived-only.
//! Version 5 carries its exact presentation; no old control or thread is deleted.
use super::{Error, Existing, Preserved};
use crate::canvas::wire::Section;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::canvas::edit::annotation) struct Removal {
    ids: Vec<String>,
}
impl Removal {
    pub(in crate::canvas::edit::annotation) fn capture(
        anchor: &Section,
        original: &Existing,
        preserved: &Preserved,
        ids: Vec<String>,
    ) -> Result<Self, Error> {
        let value = Self { ids };
        value.validate(anchor, original, preserved)?;
        Ok(value)
    }
    pub(in crate::canvas::edit::annotation) fn validate(
        &self,
        anchor: &Section,
        original: &Existing,
        preserved: &Preserved,
    ) -> Result<(), Error> {
        preserved.validate_mode(anchor, original, true)?;
        let archived = preserved.hidden(anchor)?;
        if archived.is_empty() || self.ids.len() > archived.len() {
            return Err(Error::Unsupported);
        }
        // Native common-ancestor traversal supplies this subset; numeric overlap
        // cannot reproduce the retained hidden-removal predicate. Still require
        // exact canonical identities/order, no duplicates or active control.
        let expected: Vec<_> = archived.iter().filter(|id| self.ids.contains(id)).collect();
        if expected.len() != self.ids.len() || !expected.into_iter().eq(self.ids.iter()) {
            return Err(Error::Changed);
        }
        Ok(())
    }
    pub(super) fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|old| old == id)
    }
}
