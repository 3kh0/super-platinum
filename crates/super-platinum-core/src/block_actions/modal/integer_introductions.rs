//! Extra integer provenance belongs to accepted RT-root mutation custody.
use super::*;
use crate::block_actions::form::{FormError, InputKey, InputValue, ModalForm};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicU64, Ordering},
};

const HISTORY_LIMIT: usize = 4096;
const FIELD_LIMIT: usize = 100;
mod readonly;

#[derive(Clone)]
pub(in crate::block_actions) struct Binding {
    cell: Weak<AtomicU64>,
    epoch: u64,
}
impl Binding {
    pub(in crate::block_actions) fn valid(&self) -> bool {
        self.epoch != 0
            && self
                .cell
                .upgrade()
                .is_some_and(|cell| cell.load(Ordering::Acquire) == self.epoch)
    }
    pub(in crate::block_actions) fn belongs_to(&self, cell: &Arc<AtomicU64>) -> bool {
        Weak::ptr_eq(&self.cell, &Arc::downgrade(cell))
    }
}
impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntegerBinding")
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}
impl PartialEq for Binding {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch && Weak::ptr_eq(&self.cell, &other.cell)
    }
}

/// A linear refresh witness. Read-only form clones never copy this token.
#[derive(Debug)]
pub(in crate::block_actions) struct Custody(Arc<()>);
impl Custody {
    pub(in crate::block_actions) fn belongs_to(&self, marker: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.0, marker)
    }
}

#[derive(Debug)]
pub(super) struct Owner {
    root: String,
    payload: Value,
    previous: Value,
    seen: BTreeSet<String>,
    fresh: BTreeSet<String>,
    profiles: BTreeMap<InputKey, Value>,
    retired: BTreeMap<InputKey, InputValue>,
    cell: Arc<AtomicU64>,
    custody: Option<Arc<()>>,
    drafts_frozen: bool,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.cell.store(0, Ordering::Release);
    }
}
impl Owner {
    pub(super) fn matches(&self, view: &str) -> bool {
        self.root == view
    }
    pub(super) fn new(view: &ModalView) -> Option<Self> {
        if view.kind != "modal" || view.previous_view_id.is_some() || view.needs_content {
            return None;
        }
        let payload = view.payload.as_ref()?;
        let (seen, blocks) = scan(payload);
        blocks?;
        Some(Self {
            root: view.id.clone(),
            payload: payload.clone(),
            previous: payload.clone(),
            seen,
            fresh: BTreeSet::new(),
            profiles: BTreeMap::new(),
            retired: BTreeMap::new(),
            cell: Arc::new(AtomicU64::new(1)),
            custody: None,
            drafts_frozen: false,
        })
    }
    pub(super) fn parse_root(&mut self, view: &ModalView) -> Result<ModalForm, FormError> {
        if self.custody.is_some() {
            return Err(FormError::ConflictingUpdate);
        }
        if !self.live()
            || self.cell.load(Ordering::Acquire) != 1
            || view.id != self.root
            || view.payload.as_ref() != Some(&self.payload)
        {
            self.revoke();
            return Err(FormError::ConflictingUpdate);
        }
        let parsed = ModalForm::parse_with_signed_integer_context(
            &self.payload,
            view.origin_conversation.as_deref(),
            view.viewed_conversation.as_ref(),
        );
        match parsed {
            Ok(mut form) if form.unsupported.is_empty() => {
                let marker = Arc::new(());
                self.custody = Some(marker.clone());
                form.enroll_integer_custody(Custody(marker));
                Ok(form)
            }
            other => {
                self.revoke();
                other
            }
        }
    }
    fn owns_form(&self, form: &ModalForm) -> bool {
        self.custody
            .as_ref()
            .is_some_and(|marker| form.integer_custody_matches(marker))
    }
    pub(super) fn live(&self) -> bool {
        self.cell.load(Ordering::Acquire) != 0
    }
    pub(super) fn revoke(&self) -> bool {
        self.cell.swap(0, Ordering::AcqRel) != 0
    }
    pub(super) fn observe(&mut self, view: &ModalView) {
        if !self.live() {
            return;
        }
        if view.id != self.root
            || view.kind != "modal"
            || view.previous_view_id.is_some()
            || view.needs_content
        {
            self.revoke();
            return;
        }
        let Some(payload) = view.payload.as_ref() else {
            self.revoke();
            return;
        };
        let (ids, blocks) = scan(payload);
        let fresh: BTreeSet<_> = ids.difference(&self.seen).cloned().collect();
        if self
            .seen
            .len()
            .checked_add(fresh.len())
            .is_none_or(|count| count > HISTORY_LIMIT)
        {
            self.revoke();
            return;
        }
        self.seen.extend(ids);
        let Some(blocks) = blocks else {
            self.revoke();
            return;
        };
        if self.profiles.iter().any(|(key, original)| {
            blocks.get(&key.block_id) != Some(original) || has_saved_block(payload, &key.block_id)
        }) {
            self.revoke();
            return;
        }
        let previous_epoch = self.cell.load(Ordering::Acquire);
        let Some(epoch) = previous_epoch
            .checked_add(1)
            .filter(|_| previous_epoch != 0)
        else {
            self.revoke();
            return;
        };
        if self
            .cell
            .compare_exchange(previous_epoch, epoch, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.revoke();
            return;
        }
        self.previous = std::mem::replace(&mut self.payload, payload.clone());
        self.fresh = fresh;
    }
    fn binding(&self) -> Binding {
        Binding {
            cell: Arc::downgrade(&self.cell),
            epoch: self.cell.load(Ordering::Acquire),
        }
    }
    pub(super) fn capture(&mut self, form: &ModalForm) {
        if form.view_id == self.root && self.owns_form(form) && form.introduced_owner_is(&self.cell)
        {
            if self.drafts_frozen {
                return;
            }
            form.capture_introduced_values(&mut self.retired);
            if !self.live() {
                self.drafts_frozen = true;
            }
            debug_assert!(self.retired.len() <= FIELD_LIMIT);
        }
    }
    pub(super) fn reconcile(
        &mut self,
        form: &mut ModalForm,
        payload: &Value,
    ) -> Result<(), FormError> {
        if !self.owns_form(form) {
            return Err(FormError::ConflictingUpdate);
        }
        self.capture(form);
        if form.view_id != self.root {
            self.revoke();
            return Err(FormError::ConflictingUpdate);
        }
        if !self.live() {
            return form.reconcile_retired_introductions(payload);
        }
        if payload != &self.payload
            || (form.raw_view != self.previous && form.raw_view != self.payload)
        {
            self.revoke();
            return Err(FormError::ConflictingUpdate);
        }
        let result = form.reconcile_introduced(&self.payload, &Grant { owner: self });
        if result.is_err() || !form.unsupported.is_empty() {
            self.revoke();
        } else {
            self.profiles = form.introduced_profiles().clone();
            if self.profiles.len() > FIELD_LIMIT {
                self.revoke();
            }
        }
        result
    }
    pub(super) fn consume(&self, form: &ModalForm) -> bool {
        if !form.has_introduced_integers() {
            return true;
        }
        let Some(binding) = form.introduced_binding() else {
            return false;
        };
        if form.view_id != self.root
            || form.raw_view != self.payload
            || !form.introduced_owner_is(&self.cell)
            || !self.owns_form(form)
            || !binding.valid()
        {
            return false;
        }
        self.cell
            .compare_exchange(binding.epoch, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

pub(in crate::block_actions) struct Grant<'a> {
    owner: &'a Owner,
}
impl Grant<'_> {
    pub(in crate::block_actions) fn allows(&self, key: &InputKey, block: &Value) -> bool {
        self.owner.live()
            && !has_saved_block(&self.owner.payload, &key.block_id)
            && (self.owner.fresh.contains(&key.block_id)
                || self.owner.profiles.get(key) == Some(block))
            && self
                .owner
                .payload
                .get("blocks")
                .and_then(Value::as_array)
                .is_some_and(|blocks| blocks.contains(block))
    }
    pub(in crate::block_actions) fn binding(&self) -> Binding {
        self.owner.binding()
    }
    pub(in crate::block_actions) fn matches(&self, payload: &Value) -> bool {
        self.owner.live() && payload == &self.owner.payload
    }
}

fn has_saved_block(view: &Value, id: &str) -> bool {
    view.pointer("/state/values")
        .and_then(|values| values.get(id))
        .is_some()
}
fn exact_id(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty() && id.len() <= 255)
}
fn only_keys(value: &Value, keys: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| keys.contains(&key.as_str())))
}
/// Collect identifiable keys before deciding whether this complete frame qualifies.
fn scan(view: &Value) -> (BTreeSet<String>, Option<BTreeMap<String, Value>>) {
    let mut ids = BTreeSet::new();
    let Some(blocks) = view
        .get("blocks")
        .and_then(Value::as_array)
        .filter(|blocks| blocks.len() <= FIELD_LIMIT)
    else {
        return (ids, None);
    };
    let mut valid = view["type"] == "modal"
        && only_keys(
            view,
            &[
                "id",
                "type",
                "app_id",
                "title",
                "submit",
                "close",
                "blocks",
                "hash",
                "root_view_id",
                "previous_view_id",
                "callback_id",
                "private_metadata",
                "clear_on_close",
                "notify_on_close",
                "state",
                "external_id",
            ],
        );
    if let Some(state) = view.get("state") {
        valid &= only_keys(state, &["values"]) && state.get("values").is_some_and(Value::is_object);
    }
    let mut result = BTreeMap::new();
    for block in blocks {
        let id = exact_id(block.get("block_id"));
        if let Some(id) = id {
            valid &= ids.insert(id.to_owned());
            result.insert(id.to_owned(), block.clone());
        } else {
            valid = false;
        }
        if block["type"] != "input" {
            valid &= readonly::block(block);
            continue;
        }
        valid &= block["type"] == "input"
            && only_keys(
                block,
                &["type", "block_id", "element", "label", "hint", "optional"],
            )
            && block.get("element").is_some_and(Value::is_object);
        let element = &block["element"];
        valid &= match element["type"].as_str() {
            Some("plain_text_input") => only_keys(
                element,
                &[
                    "type",
                    "action_id",
                    "initial_value",
                    "multiline",
                    "min_length",
                    "max_length",
                    "placeholder",
                    "focus_on_load",
                ],
            ),
            Some("number_input") => only_keys(
                element,
                &[
                    "type",
                    "action_id",
                    "initial_value",
                    "is_decimal_allowed",
                    "placeholder",
                    "focus_on_load",
                ],
            ),
            _ => false,
        };
    }
    (ids, valid.then_some(result))
}

#[cfg(test)]
mod readonly_tests;
#[cfg(test)]
mod tests;
