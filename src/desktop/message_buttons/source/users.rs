//! Ordinary multi_users_select ownership uses the same exact canonical source as other actions.
use super::*;
use super_platinum_core::block_actions::validation::CapturedMessageUsers;

impl Source {
    pub(crate) fn users_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw = CapturedMessageUsers::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "multi_users_select".into(),
            },
        ))
    }
    pub(crate) fn capture_users(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: &[String],
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let user =
            CapturedMessageUsers::capture(self.owner.clone(), message, block, action, selected)?;
        Ok(Captured {
            source: self.clone(),
            action: Action::Users(user),
            metadata: self.users_metadata(shell, block, action)?,
            url: None,
            service,
            display: None,
        })
    }
}
