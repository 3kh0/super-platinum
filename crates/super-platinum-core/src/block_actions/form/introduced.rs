//! Introduced provenance never becomes an initial integer key.
use super::*;
use crate::block_actions::modal::integer_introductions::{Binding, Custody, Grant};
use std::sync::{Arc, atomic::AtomicU64};

#[derive(Debug, Default)]
pub(super) struct Introduced {
    pub(super) profiles: BTreeMap<InputKey, Value>,
    binding: Option<Binding>,
    retired: BTreeMap<InputKey, InputValue>,
    frozen: BTreeSet<InputKey>,
    custody: Option<Custody>,
}
impl Clone for Introduced {
    fn clone(&self) -> Self {
        Self {
            profiles: self.profiles.clone(),
            binding: self.binding.clone(),
            retired: self.retired.clone(),
            frozen: self.frozen.clone(),
            custody: None,
        }
    }
}
impl PartialEq for Introduced {
    fn eq(&self, other: &Self) -> bool {
        // Projection equality carries data, never the linear mutation witness.
        self.profiles == other.profiles
            && self.binding == other.binding
            && self.retired == other.retired
            && self.frozen == other.frozen
    }
}
fn original_field(profile: &Value, field: &FormField) -> bool {
    matches!(&field.kind, InputKind::Number { .. }) && profile.get("element") == Some(&field.raw)
}
impl Introduced {
    pub(super) fn original_field(&self, key: &InputKey, field: &FormField) -> bool {
        self.profiles
            .get(key)
            .is_some_and(|profile| original_field(profile, field))
    }
    fn capture(&mut self, fields: &BTreeMap<InputKey, FormField>, terminal: bool) {
        for (key, profile) in &self.profiles {
            if !self.frozen.contains(key)
                && let Some(field) = fields
                    .get(key)
                    .filter(|field| original_field(profile, field))
            {
                self.retired.insert(key.clone(), field.value.clone());
            }
        }
        if terminal {
            self.frozen.extend(self.profiles.keys().cloned());
        }
        debug_assert!(self.retired.len() <= 100);
    }
    pub(super) fn register(&mut self, key: &InputKey, block: &Value, grant: &Grant<'_>) {
        self.profiles.insert(key.clone(), block.clone());
        self.binding = Some(grant.binding());
    }
    pub(super) fn bind(&mut self, grant: &Grant<'_>) {
        if !self.profiles.is_empty() {
            self.binding = Some(grant.binding());
        }
    }
}

impl ModalForm {
    pub(in crate::block_actions) fn enroll_integer_custody(&mut self, custody: Custody) {
        self.introduced.custody = Some(custody);
    }
    pub(in crate::block_actions) fn integer_custody_matches(&self, marker: &Arc<()>) -> bool {
        self.introduced
            .custody
            .as_ref()
            .is_some_and(|custody| custody.belongs_to(marker))
    }
    pub(in crate::block_actions) fn has_integer_custody(&self) -> bool {
        self.introduced.custody.is_some()
    }
    /// Relinquishing unused refresh custody cannot create introduced provenance.
    pub fn release_unused_integer_custody(&mut self) {
        if !self.has_introduced_integers() {
            self.introduced.custody = None;
        }
    }
    pub fn has_introduced_integers(&self) -> bool {
        !self.introduced.profiles.is_empty()
    }

    /// Additional source admission; the ordinary input and caller guards still apply.
    pub fn introduced_integers_current(&self) -> bool {
        !self.has_introduced_integers()
            || self.introduced.binding.as_ref().is_some_and(Binding::valid)
    }
    pub(in crate::block_actions) fn introduced_binding(&self) -> Option<&Binding> {
        self.introduced.binding.as_ref()
    }
    pub(in crate::block_actions) fn introduced_profiles(&self) -> &BTreeMap<InputKey, Value> {
        &self.introduced.profiles
    }
    pub(in crate::block_actions) fn introduced_owner_is(&self, cell: &Arc<AtomicU64>) -> bool {
        self.introduced
            .binding
            .as_ref()
            .is_some_and(|binding| binding.belongs_to(cell))
    }
    pub(in crate::block_actions) fn capture_introduced_values(
        &self,
        retired: &mut BTreeMap<InputKey, InputValue>,
    ) {
        retired.extend(self.introduced.retired.clone());
        for (key, profile) in &self.introduced.profiles {
            if !self.introduced.frozen.contains(key)
                && let Some(field) = self
                    .fields
                    .get(key)
                    .filter(|field| original_field(profile, field))
            {
                retired.insert(key.clone(), field.value.clone());
            }
        }
    }
    pub(in crate::block_actions) fn reconcile_introduced(
        &mut self,
        view: &Value,
        grant: &Grant<'_>,
    ) -> Result<(), FormError> {
        self.introduced.capture(&self.fields, false);
        if !grant.matches(view) {
            return Err(FormError::ConflictingUpdate);
        }
        let mut next = self.reconciled_update(view, Some(grant), false)?;
        next.introduced.bind(grant);
        if !grant.matches(view) || !next.introduced_integers_current() {
            return Err(FormError::ConflictingUpdate);
        }
        next.introduced.custody = self.introduced.custody.take();
        *self = next;
        Ok(())
    }
    pub(in crate::block_actions) fn reconcile_retired_introductions(
        &mut self,
        view: &Value,
    ) -> Result<(), FormError> {
        self.introduced.capture(&self.fields, true);
        let mut next = self.reconciled_update(view, None, true)?;
        next.introduced.custody = self.introduced.custody.take();
        *self = next;
        Ok(())
    }
}
