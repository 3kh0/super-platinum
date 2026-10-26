//! HvtU/hGZA callbacks and initialized Home user-parent identity.
use super::*;
use std::collections::BTreeSet;
mod multiple;

#[derive(Clone, Debug, PartialEq)]
pub struct UserParent {
    owner: Value,
    key: InputKey,
    multiple: bool,
}

impl HomeForm {
    pub(in crate::block_actions::form::home) fn initialize_user_wire(&mut self) {
        for (key, field) in &self.fields {
            if !matches!(field.kind, InputKind::User | InputKind::MultiUser { .. }) {
                continue;
            }
            let has_saved = self.view["state"]["values"]
                .get(&key.block_id)
                .and_then(|v| v.get(&key.action_id))
                .is_some();
            // Mapped absent initial props preserve omitted parent properties. Keep
            // existing raw saved ingestion, including single-user null wire.
            if !has_saved
                && matches!(&field.value, InputValue::Users(v) if v.is_empty())
                && field.raw.get("initial_users").is_none_or(Value::is_null)
            {
                self.wire
                    .insert(key.clone(), json!({"type":"multi_users_select"}));
            }
            if !has_saved && matches!(field.value, InputValue::User(None)) {
                self.wire
                    .insert(key.clone(), json!({"type":"users_select"}));
            }
        }
    }

    pub(in crate::block_actions::form::home) fn user_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.user_field(key) else {
            return false;
        };
        if !matches!(field.kind, InputKind::User | InputKind::MultiUser { .. }) {
            return false;
        }
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let omitted = matches!(field.value, InputValue::User(None))
            && wire == &json!({"type":"users_select"})
            || matches!(&field.value, InputValue::Users(v) if v.is_empty())
                && wire == &json!({"type":"multi_users_select"});
        valid_state_value(field)
            && (wire == &values::wire(field) || omitted)
            && (self.native_errors.contains_key(key) || field.error == values::error(field))
    }

    pub(in crate::block_actions::form::home) fn user_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|b| {
                    b["type"] == "input"
                        && matches!(
                            b["element"]["type"].as_str(),
                            Some("users_select" | "multi_users_select")
                        )
                })
                .all(|b| {
                    self.user_state_valid(&InputKey {
                        block_id: b["block_id"].as_str().unwrap_or_default().into(),
                        action_id: b["element"]["action_id"]
                            .as_str()
                            .unwrap_or_default()
                            .into(),
                    })
                })
        }) && self
            .fields
            .iter()
            .filter(|(_, f)| matches!(f.kind, InputKind::User | InputKind::MultiUser { .. }))
            .all(|(key, _)| self.user_state_valid(key))
    }

    /// Fresh HvtU seed: an existing selected_user, including null, wins. An
    /// omitted private parent property falls back to the current source initial.
    /// Mounted prop effects and directory eligibility remain renderer-owned.
    pub fn user_seed(&self, key: &InputKey) -> Result<Option<String>, FormError> {
        if !self.user_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::User)
            || self.native_errors.contains_key(key)
        {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(match self.wire[key].get("selected_user") {
            Some(value) => value.as_str().map(str::to_owned),
            None => self.fields[key].raw["initial_user"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        })
    }

    pub fn user_parent(&self, key: &InputKey) -> Result<UserParent, FormError> {
        self.user_seed(key)?;
        Ok(UserParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: false,
        })
    }

    pub(in crate::block_actions::form::home) fn user_parent_keys(
        &self,
        next: &Self,
        parents: &[UserParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.user_state_valid(&parent.key)
                || !next.user_state_valid(&parent.key)
                || matches!(self.fields[&parent.key].kind, InputKind::MultiUser { .. })
                    != parent.multiple
                || matches!(next.fields[&parent.key].kind, InputKind::MultiUser { .. })
                    != parent.multiple
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }

    /// A directory-owned ID or explicit clear. This validates structural
    /// identity, not account/member eligibility or native occurrence authority.
    /// Required null is stored before suppressing the new developer action.
    pub fn user_input(
        &mut self,
        key: &InputKey,
        selected: Option<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.user_state_valid(key) || !matches!(self.fields[key].kind, InputKind::User) {
            return Err(FormError::ConflictingUpdate);
        }
        if selected
            .as_deref()
            .is_some_and(|id| !valid_message_user(id))
        {
            return Err(invalid());
        }
        let initial = self.fields[key].raw["initial_user"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        self.set(key, InputValue::User(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::user(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
        )))
    }
}

#[cfg(test)]
mod tests;
