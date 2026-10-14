//! Ordinary channels_select ownership uses the same exact canonical source as other actions.
use super::*;
use super_platinum_core::block_actions::validation::CapturedMessageChannel;

impl Source {
    pub(crate) fn channel_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw = CapturedMessageChannel::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "channels_select".into(),
            },
        ))
    }
    pub(crate) fn capture_channel(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: Option<&str>,
        choice: Option<&crate::message_buttons::inputs::user::directory::Choice>,
    ) -> Result<Captured, MessageActionError> {
        let selected = selected.ok_or(MessageActionError::MalformedButton)?;
        let filter = crate::message_buttons::inputs::channel::filter(shell, &self.owner.team_id);
        let cached = if choice.is_none() {
            crate::message_buttons::inputs::user::conversations::cached(shell, self, &filter, "")
        } else {
            Vec::new()
        };
        let choice = choice
            .or_else(|| cached.iter().find(|c| c.id == selected))
            .ok_or(MessageActionError::MalformedButton)?;
        if choice.id != selected {
            return Err(MessageActionError::MalformedButton);
        }
        validate_choice(shell, self, choice)?;
        let message = self.message(shell)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let user = CapturedMessageChannel::capture(
            self.owner.clone(),
            message,
            block,
            action,
            Some(selected),
        )?;
        Ok(Captured {
            source: self.clone(),
            action: Action::Channel(user, choice.clone()),
            metadata: self.channel_metadata(shell, block, action)?,
            url: None,
            service,
            display: None,
        })
    }
}

pub(super) fn validate_choice(
    shell: &ShellState,
    source: &Source,
    choice: &crate::message_buttons::inputs::user::directory::Choice,
) -> Result<(), MessageActionError> {
    let policy = &crate::message_buttons::inputs::channel::filter(shell, &source.owner.team_id);
    if choice
        .canonical
        .as_ref()
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        != Some(choice.id.as_str())
        || policy.enterprise_id.is_some()
        || !shell
            .core
            .workspaces
            .get(&source.owner.team_id)
            .is_some_and(|workspace| {
                crate::message_buttons::inputs::user::conversations::choice_current(
                    workspace, policy, choice,
                )
            })
    {
        return Err(MessageActionError::UnsupportedButton);
    }
    Ok(())
}
