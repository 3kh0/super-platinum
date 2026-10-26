//! z+fW ordered callbacks and iGIl projected parent seed; no directory authority.
use super::*;

impl HomeForm {
    /// Explicit parent [] wins; an omitted parent falls back to current initial.
    /// Existing raw saved null ingestion remains the generic parser's policy.
    pub fn multi_channel_seed(&self, key: &InputKey) -> Result<Vec<String>, FormError> {
        if !self.channel_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::MultiChannel { .. })
            || self.native_errors.contains_key(key)
        {
            return Err(FormError::ConflictingUpdate);
        }
        match self.wire[key].get("selected_channels") {
            Some(_) => match &self.fields[key].value {
                InputValue::Channels(v) => Ok(v.clone()),
                _ => Err(invalid()),
            },
            None => match parse_message_channels(&self.fields[key].raw)?.value {
                InputValue::Channels(v) => Ok(v),
                _ => Err(invalid()),
            },
        }
    }

    pub fn multi_channel_parent(&self, key: &InputKey) -> Result<ChannelParent, FormError> {
        self.multi_channel_seed(key)?;
        Ok(ChannelParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: true,
        })
    }

    /// Structural identity is validated before mutation; required/overmax edits
    /// commit before suppressing a new event. The renderer owns public eligibility.
    pub fn multi_channel_input(
        &mut self,
        key: &InputKey,
        selected: Vec<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.channel_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::MultiChannel { .. })
        {
            return Err(FormError::ConflictingUpdate);
        }
        let mut candidate = self.fields[key].clone();
        candidate.value = InputValue::Channels(selected.clone());
        if !valid_state_value(&candidate) {
            return Err(invalid());
        }
        let initial = self.fields[key]
            .raw
            .get("initial_channels")
            .filter(|v| v.is_array())
            .cloned();
        self.set(key, InputValue::Channels(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::multi_channel(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
        )))
    }
}

#[cfg(test)]
mod tests;
