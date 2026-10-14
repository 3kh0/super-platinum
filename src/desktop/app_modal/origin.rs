//! A source message and a viewed conversation are distinct pieces of evidence.
use super::*;
use crate::message_buttons::inputs::{channels, user::conversations};
use crate::model::{ChannelTab, MainView};

/// Mirror the visible conversation boundaries in view/mod.rs. A list-only rail,
/// empty detail, resource tab or overlay cannot attest a conversation behind it.
pub(super) fn capture(shell: &ShellState) -> Option<ViewedConversation> {
    if shell.overlay.is_some()
        || shell.channel_tab != ChannelTab::Messages
        || !matches!(
            shell.main_view,
            MainView::Home | MainView::Dms | MainView::Activity | MainView::Later
        )
        || !shell.detail_open()
    {
        return None;
    }
    let id = shell.core.active_channel.as_deref()?;
    if shell.channels.get(shell.active_channel)?.id != id
        || (shell.main_view.has_own_list() && shell.surfaces.get(&shell.main_view)?.channel != id)
    {
        return None;
    }
    let workspace = shell
        .core
        .workspaces
        .get(shell.core.active_team.as_ref()?)?;
    let channel = workspace.channels.get(id)?;
    if channel.is_im
        || !(channel.is_channel || channel.is_group || channel.is_private || channel.is_mpim)
    {
        return None;
    }
    ViewedConversation::capture(id)
}

pub(super) const UNAVAILABLE: &str =
    "The conversation viewed when this form opened is unavailable for this selection.";

/// The unresolved default/filter interaction is not guessed: refuse the control
/// unless its exact frozen origin is a currently eligible non-IM conversation.
pub(super) fn unavailable(shell: &ShellState, raw: &Value) -> Option<&'static str> {
    let eligible = (|| {
        let session = shell.app_modal.as_ref()?;
        let view = session.modal.stack().last()?;
        let origin = view.viewed_conversation.as_ref()?;
        let team = &session.modal.scope().team;
        let workspace = shell.core.workspaces.get(team)?;
        let channel = workspace.channels.get(origin.id())?;
        if channel.is_im {
            return None;
        }
        let policy = conversations::workspace_filter(shell, team, raw)?;
        if conversations::unavailable(&policy).is_some() {
            return None;
        }
        channels::validate(
            workspace,
            &policy,
            &[origin.id().to_owned()],
            &session.viewed_proofs,
        )
        .ok()
    })()
    .is_some();
    (!eligible).then_some(UNAVAILABLE)
}

#[cfg(test)]
mod tests;
