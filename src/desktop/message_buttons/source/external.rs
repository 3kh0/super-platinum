//! Ordinary external_select ownership uses the same exact canonical source as other actions.
use super::*;
use super_platinum_core::block_actions::validation::{
    CapturedMessageExternal, CapturedMessageExternalOptions,
};

impl Source {
    pub(crate) fn external_options_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw =
            CapturedMessageExternalOptions::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "multi_external_select".into(),
            },
        ))
    }
    pub(crate) fn capture_external_options(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: &[super_platinum_core::block_actions::external::ExternalOption],
        proof: super::super::inputs::external::Proof,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let value = CapturedMessageExternalOptions::capture(
            self.owner.clone(),
            message,
            block,
            action,
            selected,
        )?;
        Ok(Captured {
            source: self.clone(),
            action: Action::ExternalOptions(value, proof),
            metadata: self.external_options_metadata(shell, block, action)?,
            url: None,
            service,
            display: None,
        })
    }
    pub(crate) fn external_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw = CapturedMessageExternal::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "external_select".into(),
            },
        ))
    }
    pub(crate) fn capture_external(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: Option<&super_platinum_core::block_actions::external::ExternalOption>,
        proof: super::super::inputs::external::Proof,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let user =
            CapturedMessageExternal::capture(self.owner.clone(), message, block, action, selected)?;
        Ok(Captured {
            source: self.clone(),
            action: Action::External(user, proof),
            metadata: self.external_metadata(shell, block, action)?,
            url: None,
            service,
            display: None,
        })
    }
}
