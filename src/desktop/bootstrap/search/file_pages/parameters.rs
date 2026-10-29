//! A closed read sum preserves distinct local and saved admission proofs.
use super::*;
use crate::app_home::channel_fetches::file_saved_parameters;
#[derive(Clone)]
pub(super) enum ReadParameters {
    Local(Parameters),
    Saved(file_saved_parameters::Parameters),
}
impl ReadParameters {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        match self {
            Self::Local(p) => p.current(s),
            Self::Saved(p) => p.current(s),
        }
    }
    pub(super) fn client(&self) -> &super_platinum_core::slack::SlackClient {
        match self {
            Self::Local(p) => p.client(),
            Self::Saved(p) => p.client(),
        }
    }
    pub(super) fn session(&self) -> &super_platinum_core::config::WorkspaceSession {
        match self {
            Self::Local(p) => p.session(),
            Self::Saved(p) => p.session(),
        }
    }
    pub(super) fn transport(&self) -> &Arc<super_platinum_core::slack::Transport> {
        match self {
            Self::Local(p) => p.transport(),
            Self::Saved(p) => p.transport(),
        }
    }
    pub(super) fn exclude_bots(&self) -> Option<bool> {
        match self {
            Self::Local(p) => p.exclude_bots(),
            Self::Saved(p) => p.exclude_bots(),
        }
    }
    pub(super) fn only_my_channels(&self) -> Option<bool> {
        match self {
            Self::Local(p) => p.only_my_channels(),
            Self::Saved(p) => p.only_my_channels(),
        }
    }
}
