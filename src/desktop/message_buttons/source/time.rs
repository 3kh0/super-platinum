//! Ordinary timepicker ownership uses the same exact canonical source as other actions.
use super::*;
use super_platinum_core::block_actions::validation::CapturedMessageTime;

impl Source {
    pub(crate) fn time_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw = CapturedMessageTime::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "timepicker".into(),
            },
        ))
    }
    pub(crate) fn capture_time(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: Option<&str>,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let time =
            CapturedMessageTime::capture(self.owner.clone(), message, block, action, selected)?;
        Ok(Captured {
            source: self.clone(),
            action: Action::Time(time),
            metadata: self.time_metadata(shell, block, action)?,
            url: None,
            service,
            display: None,
        })
    }
}
