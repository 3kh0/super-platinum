//! Every selected channel carries canonical evidence through confirmation.
use super::*;
use crate::message_buttons::inputs::{
    channels,
    user::{conversations::Filter, directory::Choice},
};
use super_platinum_core::block_actions::validation::CapturedMessageConversations;

impl Source {
    pub(crate) fn conversations_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw =
            CapturedMessageConversations::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "multi_conversations_select".into(),
            },
        ))
    }
    pub(crate) fn capture_conversations(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: &[String],
        proofs: &channels::Proofs,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let raw =
            CapturedMessageConversations::validate_element(&self.owner, message, block, action)?;
        let policy = crate::message_buttons::inputs::user::conversations::workspace_filter(
            shell,
            &self.owner.team_id,
            raw,
        )
        .ok_or(MessageActionError::UnsupportedButton)?;
        let choices = crate::message_buttons::inputs::conversations::validate(
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
        let value = CapturedMessageConversations::capture(
            self.owner.clone(),
            message,
            block,
            action,
            selected,
        )?;
        Ok(Captured {
            source: self.clone(),
            action: Action::Conversations(value, choices, policy),
            metadata: self.conversations_metadata(shell, block, action)?,
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
    metadata: &BlockElementMetadata,
) -> Result<(), MessageActionError> {
    let message = source.message(shell)?;
    let raw = CapturedMessageConversations::validate_element(
        &source.owner,
        message,
        metadata
            .block_id
            .as_deref()
            .ok_or(MessageActionError::MalformedButton)?,
        metadata
            .action_id
            .as_deref()
            .ok_or(MessageActionError::MalformedButton)?,
    )?;
    if Some(policy)
        != crate::message_buttons::inputs::user::conversations::workspace_filter(
            shell,
            &source.owner.team_id,
            raw,
        )
        .as_ref()
    {
        return Err(MessageActionError::StaleMessage);
    }
    let proofs = choices.iter().map(|c| (c.id.clone(), c.clone())).collect();
    crate::message_buttons::inputs::conversations::validate(
        &shell.core.workspaces[&source.owner.team_id],
        policy,
        selected,
        &proofs,
    )
    .map(|_| ())
    .map_err(|_| MessageActionError::UnsupportedButton)
}
