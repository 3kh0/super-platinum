//! Multi-option capture accepts only canonical options or trusted mounted history.
use super::*;
use super_platinum_core::block_actions::validation::CapturedMessageMulti;

impl Source {
    pub(crate) fn multi_metadata(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
    ) -> Result<BlockElementMetadata, MessageActionError> {
        let message = self.message(shell)?;
        super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let raw = CapturedMessageMulti::validate_element(&self.owner, message, block, action)?;
        Ok(crate::blocks::action::metadata(
            raw,
            Some(block),
            BlockElementSource::UnsupportedControl {
                kind: raw
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            },
        ))
    }
    pub(crate) fn capture_multi_optional(
        &self,
        shell: &ShellState,
        block: &str,
        action: &str,
        selected: Option<&[Value]>,
        retained: &[Value],
    ) -> Result<Captured, MessageActionError> {
        let message = self.message(shell)?;
        let service =
            super::super::service::resolve(&shell.core.workspaces[&self.owner.team_id], message)?;
        let multi = CapturedMessageMulti::capture_optional(
            self.owner.clone(),
            message,
            block,
            action,
            selected,
            retained,
        )?;
        let metadata = self.multi_metadata(shell, block, action)?;
        Ok(Captured {
            source: self.clone(),
            action: Action::Multi(multi),
            metadata,
            url: None,
            service,
            display: None,
        })
    }
}
