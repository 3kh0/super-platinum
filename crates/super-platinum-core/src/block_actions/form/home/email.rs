//! Home EZY9/+Key/2Ztu email callbacks and rhb/ validation.
//! Native email sanitization, local state, effects and event custody are renderer-owned.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmailTrigger {
    Change,
    Enter,
}
/// Explicit parent lifecycle, not inferred from an empty local or parent value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmailMount {
    NewParent,
    RetainedParent,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EmailParent {
    owner: Value,
    key: InputKey,
}

/// Exact ASCII rhb/ predicate. Deliberately not a general RFC email parser:
/// local dots are unrestricted, domains need at least two 1..63 byte labels.
fn valid_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty()
        || !local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&c))
    {
        return false;
    }
    let mut count = 0;
    for label in domain.split('.') {
        let bytes = label.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 63
            || !bytes[0].is_ascii_alphanumeric()
            || !bytes[bytes.len() - 1].is_ascii_alphanumeric()
            || !bytes
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || *c == b'-')
        {
            return false;
        }
        count += 1;
    }
    count >= 2
}
pub(super) fn error(field: &FormField) -> Option<String> {
    let (InputKind::Email, InputValue::Email(value)) = (&field.kind, &field.value) else {
        return Some("This value no longer matches the field.".into());
    };
    if value.as_deref().is_none_or(str::is_empty) {
        return (!field.optional).then(|| "An email address is required".into());
    }
    (!valid_email(value.as_deref().unwrap())).then(|| "You must enter a valid email address".into())
}
pub(super) fn parse(element: &Value) -> Result<(InputKind, InputValue), FormError> {
    if element["type"] != "email_text_input"
        || element.get("initial_value").is_some_and(|v| !v.is_string())
    {
        return Err(invalid());
    }
    // cxJV has no email initializer. A truthy initial affects local display;
    // its first child effect is overwritten by the parent's absent-row initializer.
    Ok((InputKind::Email, InputValue::Email(None)))
}
pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && element["type"] == "email_text_input"
        && matches!(kind, InputKind::Email)
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|k| element.get(*k).is_none())
}
pub(super) fn dispatch(element: &Value) -> Result<Dispatch, FormError> {
    let Some(config) = element.get("dispatch_action_config") else {
        return Ok(Dispatch::Change);
    };
    let config = config.as_object().ok_or_else(invalid)?;
    let Some(triggers) = config.get("trigger_actions_on") else {
        return Ok(Dispatch::Change);
    };
    let triggers = triggers.as_array().ok_or_else(invalid)?;
    let (mut enter, mut character) = (false, false);
    for trigger in triggers {
        match trigger.as_str() {
            Some("on_enter_pressed") => enter = true,
            Some("on_character_entered") => character = true,
            _ => return Err(invalid()),
        }
    }
    Ok(match (enter, character) {
        (true, true) => Dispatch::EnterOrCharacter,
        (true, false) => Dispatch::Enter,
        (false, true) => Dispatch::Character,
        _ => Dispatch::Change,
    })
}
impl HomeForm {
    pub fn email_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
        if !self.unsupported.is_empty() {
            return Err(FormError::Unsupported(self.unsupported.clone()));
        }
        let field = self.fields.get(key).ok_or(FormError::UnknownField)?;
        let block = self.view["blocks"]
            .as_array()
            .and_then(|bs| bs.iter().find(|b| b["block_id"] == key.block_id))
            .ok_or_else(invalid)?;
        if !supported(block, &field.raw, &field.kind)
            || block.get("element") != Some(&field.raw)
            || field.raw["action_id"] != key.action_id
            || field.optional != block["optional"].as_bool().unwrap_or(false)
            || parse(&field.raw)?.0 != field.kind
            || self.dispatch(key) != Some(super::dispatch(block, &field.raw, &field.kind)?)
        {
            return Err(invalid());
        }
        Ok(field)
    }
    pub(super) fn email_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.email_field(key) else {
            return false;
        };
        matches!(field.value, InputValue::Email(_))
            && self.wire.get(key) == Some(&values::wire(field))
            && (self.native_errors.contains_key(key) || field.error == error(field))
    }
    pub(super) fn email_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|bs| {
            bs.iter()
                .filter(|b| b["element"]["type"] == "email_text_input")
                .all(|b| {
                    self.email_state_valid(&InputKey {
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
            .filter(|(_, f)| matches!(f.kind, InputKind::Email))
            .all(|(key, _)| self.email_state_valid(key))
    }
    /// iGIl has no saved email mapping. Fresh local uses only current initial.
    /// This does not apply the mount effect or synchronize an existing widget.
    pub fn email_seed(&self, key: &InputKey) -> Result<String, FormError> {
        if !self.email_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(self.fields[key].raw["initial_value"]
            .as_str()
            .unwrap_or_default()
            .into())
    }
    /// Run once for a fresh owned widget, before native admission. A source-driven
    /// DOM remount of the same widget must not call this again. NewParent models
    /// final child-first/parent-last effect state for an absent parent. An already
    /// admitted saved row in a newly parsed form counts as an existing parent.
    /// Retained parent mount commits a truthy initial silently. No action is made.
    pub fn email_mount(&mut self, key: &InputKey, mount: EmailMount) -> Result<String, FormError> {
        let seed = self.email_seed(key)?;
        match mount {
            EmailMount::NewParent => {
                if self.fields[key].dirty {
                    return Err(FormError::ConflictingUpdate);
                }
                let saved_parent = self.view["state"]["values"]
                    .get(&key.block_id)
                    .and_then(|v| v.get(&key.action_id))
                    .is_some();
                if saved_parent {
                    if !seed.is_empty() {
                        self.email_store(key, seed.clone())?;
                    }
                } else if self.wire[key] != json!({"type":"email_text_input"}) {
                    return Err(FormError::ConflictingUpdate);
                }
            }
            EmailMount::RetainedParent if !seed.is_empty() => {
                self.email_store(key, seed.clone())?
            }
            EmailMount::RetainedParent => {}
        }
        Ok(seed)
    }
    pub fn email_parent(&self, key: &InputKey) -> Result<EmailParent, FormError> {
        self.email_seed(key)?;
        Ok(EmailParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
        })
    }
    pub(super) fn email_parent_keys(
        &self,
        next: &Self,
        parents: &[EmailParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.email_state_valid(&parent.key)
                || !next.email_state_valid(&parent.key)
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }
    pub(super) fn email_store(&mut self, key: &InputKey, value: String) -> Result<(), FormError> {
        if !self.email_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.value = InputValue::Email(Some(value));
        field.dirty = true;
        field.error = error(field);
        self.wire.insert(key.clone(), values::wire(field));
        Ok(())
    }
    /// Exact callback literal, including empty string and invalid editing state.
    /// Parent commit precedes validation. Equality is never suppressed here.
    /// Native callback reachability, timing and loading are renderer-owned.
    pub fn email_input(
        &mut self,
        key: &InputKey,
        literal: String,
        trigger: EmailTrigger,
    ) -> Result<Option<TextEvent>, FormError> {
        self.email_store(key, literal)?;
        let mode = self.dispatch(key).ok_or_else(invalid)?;
        let emit = match trigger {
            EmailTrigger::Change => {
                matches!(mode, Dispatch::Character | Dispatch::EnterOrCharacter)
            }
            EmailTrigger::Enter => matches!(mode, Dispatch::Enter | Dispatch::EnterOrCharacter),
        };
        if !emit || self.fields[key].error.is_some() {
            return Ok(None);
        }
        let mut action = self.wire[key].clone();
        action["block_id"] = json!(key.block_id);
        action["action_id"] = json!(key.action_id);
        Ok(Some(TextEvent::email(
            self.view.clone(),
            key.clone(),
            action,
        )))
    }
    /// Only actual owned native acknowledgement can clear the safety marker.
    /// Email domain invalidity remains a parent error, not an unseen-buffer error.
    pub fn email_acknowledge_native_buffer(
        &mut self,
        key: &InputKey,
        _literal: &str,
    ) -> Result<(), FormError> {
        if !self.email_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.error = error(field);
        Ok(())
    }
}
#[cfg(test)]
mod tests;
