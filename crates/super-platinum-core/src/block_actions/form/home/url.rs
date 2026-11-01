//! First-owned Home URL data. Native WHATWG validation and occurrence custody
//! belong to the renderer; no Rust URL normalization or Modal submit rule.
use super::*;
use std::{collections::BTreeSet, sync::Arc};
mod profile;
pub(super) use profile::Profile;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlMount {
    NewParent,
    RetainedParent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlTrigger {
    Change,
    Enter,
}
#[derive(Clone, Debug)]
pub struct UrlScope(Arc<()>);
impl PartialEq for UrlScope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
#[derive(Clone, Debug)]
pub struct UrlParent {
    owner: Value,
    key: InputKey,
    identity: Arc<()>,
}

pub(super) fn parse(block: &Value, element: &Value) -> Result<(InputKind, InputValue), FormError> {
    let e = element.as_object().ok_or_else(invalid)?;
    if block["type"] != "input"
        || element["type"] != "url_text_input"
        || e.keys().any(|k| {
            !matches!(
                k.as_str(),
                "type"
                    | "action_id"
                    | "initial_value"
                    | "placeholder"
                    | "dispatch_action_config"
                    | "focus_on_load"
                    | "accessibility_label"
            )
        })
        || element.get("initial_value").is_some_and(|v| !v.is_string())
        || element
            .get("focus_on_load")
            .is_some_and(|v| !v.is_boolean())
        || element
            .get("accessibility_label")
            .is_some_and(|v| !v.is_string())
    {
        return Err(invalid());
    }
    dispatch(block, element)?;
    Ok((
        InputKind::Url,
        InputValue::Url(
            element
                .get("initial_value")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_owned),
        ),
    ))
}
pub(super) fn dispatch(block: &Value, element: &Value) -> Result<Dispatch, FormError> {
    let enabled = block
        .get("dispatch_action")
        .map_or(Ok(false), |v| v.as_bool().ok_or_else(invalid))?;
    let Some(config) = element.get("dispatch_action_config") else {
        return Ok(if enabled {
            Dispatch::Change
        } else {
            Dispatch::Quiet
        });
    };
    let config = config.as_object().ok_or_else(invalid)?;
    if config.keys().any(|k| k != "trigger_actions_on") {
        return Err(invalid());
    }
    let Some(triggers) = config.get("trigger_actions_on") else {
        return Ok(if enabled {
            Dispatch::Change
        } else {
            Dispatch::Quiet
        });
    };
    let (mut enter, mut character) = (false, false);
    for trigger in triggers.as_array().ok_or_else(invalid)? {
        match trigger.as_str() {
            Some("on_enter_pressed") => enter = true,
            Some("on_character_entered") => character = true,
            _ => return Err(invalid()),
        }
    }
    Ok(if !enabled {
        Dispatch::Quiet
    } else {
        match (enter, character) {
            (true, true) => Dispatch::EnterOrCharacter,
            (true, false) => Dispatch::Enter,
            (false, true) => Dispatch::Character,
            _ => Dispatch::Change,
        }
    })
}
impl HomeForm {
    pub fn url_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
        if !self.unsupported.is_empty()
            || !self.url_profile.as_ref().is_some_and(|p| p.contains(key))
        {
            return Err(FormError::UnknownField);
        }
        let field = self.fields.get(key).ok_or(FormError::UnknownField)?;
        let block = self.view["blocks"]
            .as_array()
            .and_then(|bs| bs.iter().find(|b| b["block_id"] == key.block_id))
            .ok_or_else(invalid)?;
        if block.get("element") != Some(&field.raw)
            || field.raw["action_id"] != key.action_id
            || field.optional != block["optional"].as_bool().unwrap_or(false)
            || parse(block, &field.raw)?.0 != field.kind
            || self.dispatch(key) != Some(dispatch(block, &field.raw)?)
        {
            return Err(invalid());
        }
        Ok(field)
    }
    pub(super) fn url_state_valid(&self, key: &InputKey) -> bool {
        self.url_field(key).is_ok_and(|f| {
            matches!(f.value, InputValue::Url(_))
                && self.wire.get(key) == Some(&values::wire(f))
                && f.error == self.url_expected_error(key, f)
        })
    }
    pub(super) fn url_state_inventory_valid(&self) -> bool {
        let Some(profile) = &self.url_profile else {
            return true;
        };
        let Ok(Some(next)) = self.continuation_url_profile(&self.view) else {
            return false;
        };
        profile == &next
            && self
                .fields
                .iter()
                .filter(|(_, f)| matches!(f.kind, InputKind::Url))
                .all(|(k, _)| self.url_state_valid(k))
            && self.view["blocks"].as_array().is_some_and(|bs| {
                bs.iter()
                    .filter(|b| b["element"]["type"] == "url_text_input")
                    .all(|b| {
                        self.url_state_valid(&InputKey {
                            block_id: b["block_id"].as_str().unwrap_or_default().into(),
                            action_id: b["element"]["action_id"]
                                .as_str()
                                .unwrap_or_default()
                                .into(),
                        })
                    })
            })
    }
    pub fn url_seed(&self, key: &InputKey) -> Result<String, FormError> {
        if !self.url_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(self.fields[key].raw["initial_value"]
            .as_str()
            .unwrap_or_default()
            .into())
    }
    pub fn url_parent(&self, key: &InputKey) -> Result<UrlParent, FormError> {
        self.url_seed(key)?;
        Ok(UrlParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            identity: self.url_profile.as_ref().unwrap().identity.clone(),
        })
    }
    pub fn url_mount(
        &mut self,
        key: &InputKey,
        mount: UrlMount,
        valid: bool,
    ) -> Result<String, FormError> {
        let local = self.url_seed(key)?;
        if mount == UrlMount::NewParent && self.fields[key].dirty {
            return Err(FormError::ConflictingUpdate);
        }
        if !local.is_empty() {
            self.url_store(key, local.clone(), valid)?;
        }
        Ok(local)
    }
    pub(super) fn url_expected_error(&self, key: &InputKey, field: &FormField) -> Option<String> {
        self.native_errors
            .get(key)
            .cloned()
            .or_else(|| self.url_profile.as_ref()?.error(key, &field.value))
    }
    fn url_store(&mut self, key: &InputKey, literal: String, valid: bool) -> Result<(), FormError> {
        if !self.url_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        self.native_errors.remove(key);
        self.url_profile
            .as_mut()
            .unwrap()
            .store_verdict(key.clone(), literal.clone(), valid);
        let f = self.fields.get_mut(key).unwrap();
        f.value = InputValue::Url(Some(literal));
        f.dirty = true;
        f.error = self.url_profile.as_ref().unwrap().error(key, &f.value);
        self.wire.insert(key.clone(), values::wire(f));
        Ok(())
    }
    /// Validation verdict is data from the exact original native URL occurrence.
    /// Storing a literal never grants a request or native event permission.
    pub fn url_input(
        &mut self,
        key: &InputKey,
        literal: String,
        valid: bool,
        trigger: UrlTrigger,
    ) -> Result<Option<TextEvent>, FormError> {
        let valid = literal.is_empty() || valid;
        self.url_store(key, literal, valid)?;
        let emit = match trigger {
            UrlTrigger::Change => matches!(
                self.dispatch(key),
                Some(Dispatch::Character | Dispatch::EnterOrCharacter)
            ),
            UrlTrigger::Enter => matches!(
                self.dispatch(key),
                Some(Dispatch::Enter | Dispatch::EnterOrCharacter)
            ),
        };
        if !emit || !valid {
            return Ok(None);
        }
        let mut action = self.wire[key].clone();
        action["block_id"] = json!(key.block_id);
        action["action_id"] = json!(key.action_id);
        Ok(Some(TextEvent::url(
            self.view.clone(),
            key.clone(),
            action,
            self.url_profile.as_ref().unwrap().identity.clone(),
        )))
    }
    pub fn url_mark_native_buffer(&mut self, key: &InputKey) -> Result<(), FormError> {
        self.url_field(key)?;
        let error = "Finish editing this URL before continuing.".to_owned();
        self.native_errors.insert(key.clone(), error.clone());
        self.fields.get_mut(key).unwrap().error = Some(error);
        Ok(())
    }
    pub fn url_acknowledge_native_buffer(&mut self, key: &InputKey) -> Result<(), FormError> {
        if !self.url_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        self.native_errors.remove(key);
        let error = self.url_expected_error(key, &self.fields[key]);
        self.fields.get_mut(key).unwrap().error = error;
        Ok(())
    }
    pub(super) fn url_parent_keys(
        &self,
        next: &Self,
        parents: &[UrlParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for p in parents {
            if p.owner != checkboxes::owner(&self.view)
                || p.owner != checkboxes::owner(&next.view)
                || !self
                    .url_profile
                    .as_ref()
                    .is_some_and(|v| Arc::ptr_eq(&v.identity, &p.identity))
                || !next
                    .url_profile
                    .as_ref()
                    .is_some_and(|v| Arc::ptr_eq(&v.identity, &p.identity))
                || !self.url_state_valid(&p.key)
                || !next.url_state_valid(&p.key)
                || !keys.insert(p.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }
    pub fn url_event_current(&self, event: &TextEvent) -> bool {
        event.url_profile(self)
    }
    pub fn owned_url_scope(&self) -> Option<UrlScope> {
        self.url_profile
            .as_ref()
            .map(|p| UrlScope(p.identity.clone()))
    }
    pub fn same_url_scope(&self, scope: &UrlScope) -> bool {
        self.url_profile
            .as_ref()
            .is_some_and(|p| Arc::ptr_eq(&p.identity, &scope.0))
    }
}
