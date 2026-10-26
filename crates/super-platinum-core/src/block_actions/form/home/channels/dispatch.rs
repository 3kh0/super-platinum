//! cdYm / cxJV / AM6q Home parent wire, independent of directory authority.
use super::*;
use std::collections::BTreeSet;
mod multiple;

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelParent {
    owner: Value,
    key: InputKey,
    multiple: bool,
}

impl HomeForm {
    pub(in crate::block_actions::form::home) fn initialize_channel_wire(&mut self) {
        for (key, field) in &self.fields {
            if !matches!(
                field.kind,
                InputKind::Channel | InputKind::MultiChannel { .. }
            ) {
                continue;
            }
            let has_saved = self.view["state"]["values"]
                .get(&key.block_id)
                .and_then(|v| v.get(&key.action_id))
                .is_some();
            // cxJV seeds selection only, never the callback-only response bit.
            // Do not change the existing raw saved-state ingestion policy.
            if !has_saved {
                if let Some(wire) = self.wire.get_mut(key).and_then(Value::as_object_mut) {
                    wire.remove("response_url_enabled");
                    if matches!(field.value, InputValue::Channel(None)) {
                        wire.remove("selected_channel");
                    }
                    if matches!(&field.value, InputValue::Channels(v) if v.is_empty())
                        && field.raw.get("initial_channels").is_none_or(Value::is_null)
                    {
                        wire.remove("selected_channels");
                    }
                }
            }
        }
    }

    pub(in crate::block_actions::form::home) fn channel_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.channel_field(key) else {
            return false;
        };
        if !valid_state_value(field) {
            return false;
        }
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let expected = match (&field.kind, &field.value) {
            (InputKind::Channel, InputValue::Channel(selected)) => {
                let mut expected = json!({"type":"channels_select"});
                if selected.is_some() || wire.get("selected_channel").is_some() {
                    expected["selected_channel"] = json!(selected);
                }
                // A retained single parent may keep its old true callback bit.
                if wire.get("response_url_enabled") == Some(&Value::Bool(true)) {
                    expected["response_url_enabled"] = json!(true);
                }
                expected
            }
            (InputKind::MultiChannel { .. }, InputValue::Channels(selected)) => {
                let mut expected = json!({"type":"multi_channels_select"});
                if !selected.is_empty() || wire.get("selected_channels").is_some() {
                    expected["selected_channels"] = json!(selected);
                }
                expected
            }
            _ => return false,
        };
        wire == &expected
            && (self.native_errors.contains_key(key) || field.error == values::error(field))
    }

    pub(in crate::block_actions::form::home) fn channel_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|b| {
                    b["type"] == "input"
                        && matches!(
                            b["element"]["type"].as_str(),
                            Some("channels_select" | "multi_channels_select")
                        )
                })
                .all(|b| {
                    self.channel_state_valid(&InputKey {
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
            .filter(|(_, f)| matches!(f.kind, InputKind::Channel | InputKind::MultiChannel { .. }))
            .all(|(key, _)| self.channel_state_valid(key))
    }

    /// Actual iGIl nullish parent projection then cdYm constructor. A committed
    /// null or omitted selection falls back to current initial without writing
    /// that display value into the parent. Call only for a new mounted widget.
    pub fn channel_seed(&self, key: &InputKey) -> Result<Option<String>, FormError> {
        if !self.channel_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::Channel)
            || self.native_errors.contains_key(key)
        {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(self.wire[key]
            .get("selected_channel")
            .and_then(Value::as_str)
            .or_else(|| {
                self.fields[key].raw["initial_channel"]
                    .as_str()
                    .filter(|s| !s.is_empty())
            })
            .map(str::to_owned))
    }

    pub fn channel_parent(&self, key: &InputKey) -> Result<ChannelParent, FormError> {
        self.channel_seed(key)?;
        Ok(ChannelParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: false,
        })
    }

    pub(in crate::block_actions::form::home) fn channel_parent_keys(
        &self,
        next: &Self,
        parents: &[ChannelParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.channel_state_valid(&parent.key)
                || !next.channel_state_valid(&parent.key)
                || matches!(
                    self.fields[&parent.key].kind,
                    InputKind::MultiChannel { .. }
                ) != parent.multiple
                || matches!(
                    next.fields[&parent.key].kind,
                    InputKind::MultiChannel { .. }
                ) != parent.multiple
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }

    /// A caller-owned canonical public channel ID or explicit clear. Structural
    /// validation is atomic; account/team/native eligibility remains external.
    /// Quiet and required-empty callbacks still commit their parent first.
    pub fn channel_input(
        &mut self,
        key: &InputKey,
        selected: Option<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.channel_state_valid(key) || !matches!(self.fields[key].kind, InputKind::Channel) {
            return Err(FormError::ConflictingUpdate);
        }
        if selected
            .as_deref()
            .is_some_and(|id| !modal_channel::valid_id(id))
        {
            return Err(invalid());
        }
        let initial = self.fields[key].raw["initial_channel"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let response_url = self.fields[key].raw["response_url_enabled"] == Value::Bool(true);
        self.set(key, InputValue::Channel(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::channel(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
            response_url,
        )))
    }
}

#[cfg(test)]
mod tests;
