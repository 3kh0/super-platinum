//! Every selected channel carries canonical evidence through confirmation.
use super::*;
use crate::message_buttons::inputs::{
    channels,
    user::{conversations::Filter, directory::Choice},
};
use super_platinum_core::block_actions::validation::CapturedMessageChannels;

impl Source {
    pub(crate) fn channels_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw = CapturedMessageChannels::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "multi_channels_select".into(),
            },
        ))
    }
    pub(crate) fn capture_channels(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: &[String],
        proofs: &channels::Proofs,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let policy = channels::filter(shell, &self.owner.team_id);
        let choices = channels::validate(
            &shell.core.workspaces[&self.owner.team_id],
            &policy,
            selected,
            proofs,
        )
        .map_err(|_| MessageActionError::UnsupportedButton)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let value =
            CapturedMessageChannels::capture(self.owner.clone(), message, block, action, selected)?;
        Ok(Captured {
            source: self.clone(),
            action: Action::Channels(value, choices, policy),
            metadata: self.channels_metadata(shell, block, action)?,
            url: None,
            service,
            display: None,
        })
    }
}
pub(super) fn validate_choices(
    shell: &ShellState,
    source: &Source,
    selected: &[String],
    choices: &[Choice],
    policy: &Filter,
) -> Result<(), MessageActionError> {
    if *policy != channels::filter(shell, &source.owner.team_id) {
        return Err(MessageActionError::StaleMessage);
    }
    let proofs = choices.iter().map(|c| (c.id.clone(), c.clone())).collect();
    channels::validate(
        &shell.core.workspaces[&source.owner.team_id],
        policy,
        selected,
        &proofs,
    )
    .map(|_| ())
    .map_err(|_| MessageActionError::UnsupportedButton)
}
