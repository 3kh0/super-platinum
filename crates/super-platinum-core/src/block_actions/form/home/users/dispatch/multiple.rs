//! hGZA input callbacks; native directory occurrence authority stays renderer-owned.
use super::*;

impl HomeForm {
    /// Existing parent [] wins; an omitted parent falls back to current initial props.
    pub fn multi_user_seed(&self, key: &InputKey) -> Result<Vec<String>, FormError> {
        if !self.user_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::MultiUser { .. })
            || self.native_errors.contains_key(key)
        {
            return Err(FormError::ConflictingUpdate);
        }
        match self.wire[key].get("selected_users") {
            Some(_) => match &self.fields[key].value {
                InputValue::Users(v) => Ok(v.clone()),
                _ => Err(invalid()),
            },
            None => match parse_message_users(&self.fields[key].raw)?.value {
                InputValue::Users(v) => Ok(v),
                _ => Err(invalid()),
            },
        }
    }

    pub fn multi_user_parent(&self, key: &InputKey) -> Result<UserParent, FormError> {
        self.multi_user_seed(key)?;
        Ok(UserParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: true,
        })
    }

    /// Preserve order and commit required-empty/overmax before suppressing a new action.
    /// Structural failures refuse atomically; no member eligibility is inferred here.
    pub fn multi_user_input(
        &mut self,
        key: &InputKey,
        selected: Vec<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.user_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::MultiUser { .. })
        {
            return Err(FormError::ConflictingUpdate);
        }
        let mut candidate = self.fields[key].clone();
        candidate.value = InputValue::Users(selected.clone());
        if !valid_state_value(&candidate) {
            return Err(invalid());
        }
        let initial = self.fields[key]
            .raw
            .get("initial_users")
            .filter(|v| v.is_array())
            .cloned();
        self.set(key, InputValue::Users(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::multi_user(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
        )))
    }
}

#[cfg(test)]
mod tests;
