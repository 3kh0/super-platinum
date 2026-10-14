//! Stateless ordinary-message overflow choices, captured from canonical data.
use super::*;
use super_platinum_core::block_actions::{
    form::{OptionSpec, parse_message_overflow},
    validation::CapturedMessageOverflow,
};

impl Source {
    pub(crate) fn overflow_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw = CapturedMessageOverflow::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: "overflow".into(),
            },
        ))
    }

    pub(crate) fn overflow_options(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<Vec<OptionSpec>, MessageActionError> {
        let message = self.message(shell)?;
        let raw = CapturedMessageOverflow::validate_element(&self.owner, message, block, action)?;
        parse_message_overflow(raw).map_err(|_| MessageActionError::MalformedButton)
    }

    pub(crate) fn capture_overflow(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: &str,
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        super_platinum_core::block_actions::message_state::MessageInputs::parse(message)
            .map_err(|_| MessageActionError::UnsupportedButton)?;
        let overflow =
            CapturedMessageOverflow::capture(self.owner.clone(), message, block, action, selected)?;
        let url = overflow.url().map(str::to_owned);
        Ok(Captured {
            source: self.clone(),
            action: Action::Overflow(overflow),
            metadata: self.overflow_metadata(shell, block, action)?,
            url,
            service,
            display: None,
        })
    }
}
