//! Result-local conversation labels; search metadata never replaces channel facts.
use super_platinum_core::{slack::models::Channel, state::Workspace};

fn effective(workspace: &Workspace, channel: &Channel) -> Channel {
    let known = workspace.channels.get(&channel.id);
    // Explicit result classification wins. Decoded false flags do not tell us
    // whether Slack omitted them, so cached classification is only a fallback.
    let (is_im, is_mpim) = if channel.is_mpim {
        (false, true)
    } else if channel.is_im {
        (true, false)
    } else {
        known
            .map(|c| (c.is_im && !c.is_mpim, c.is_mpim))
            .unwrap_or_default()
    };
    let mut result = channel.clone();
    result.is_im = is_im;
    result.is_mpim = is_mpim;
    if is_im {
        let nonblank = |id: &&str| !id.trim().is_empty();
        let peer = super_platinum_core::state::dm_user_id(channel)
            .filter(nonblank)
            .or_else(|| {
                known
                    .filter(|c| c.is_im && !c.is_mpim)
                    .and_then(super_platinum_core::state::dm_user_id)
                    .filter(nonblank)
            });
        if let Some(peer) = peer {
            result.user = Some(peer.into());
        }
    }
    result
}

pub(super) fn unresolved_peer(workspace: &Workspace, channel: &Channel) -> Option<String> {
    if channel.id.trim().is_empty() {
        return None;
    }
    let channel = effective(workspace, channel);
    if !channel.is_im {
        return None;
    }
    let peer = super_platinum_core::state::dm_user_id(&channel)?;
    super::user_names::needs_name(workspace, peer).then(|| peer.into())
}

pub(super) fn label(workspace: &Workspace, channel: &Channel) -> (String, bool) {
    let channel = effective(workspace, channel);
    (
        super_platinum_core::state::channel_display_name(workspace, &channel),
        channel.is_im || channel.is_mpim,
    )
}

mod fixture;
pub(super) use fixture::install;

#[cfg(test)]
mod tests;
