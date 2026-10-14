//! DM rail: B:0SNR.e, distinct from fexP's Activity/Dock DM category.
use crate::state::{Workspace, notifications::pref_truthy};

impl Workspace {
    /// Highlight total for the native, always-present DM rail. The stock
    /// tabs-hidden branch routes this total to Home; this client shows its rail.
    /// fgSa supplies eligible per-channel highlights before 0SNR excludes apps.
    pub fn dm_rail_badge_count(&self) -> u32 {
        self.channels
            .values()
            .filter(|channel| channel.is_im || channel.is_mpim)
            .filter(|channel| !self.sidebar.is_muted(&channel.id))
            .filter(|channel| {
                // processNewChannelForUpserting normalizes IM/MPIM membership
                // true and fromAnotherTeam false (S2277553), so raw omitted or
                // false membership must not remove a group DM here.
                channel.is_im
                    || (!channel.is_archived
                        && !channel.extra.get("is_file").is_some_and(pref_truthy))
            })
            .filter(|channel| {
                // HhPa.e only supplies a user for an IM. A group containing
                // bots remains a DM conversation; do not inspect its members.
                if !channel.is_im {
                    return true;
                }
                let user_id = channel.user.as_deref().unwrap_or("");
                if matches!(
                    user_id,
                    "USEARCH" | "USLACKSECURITY" | "USLACKBOT" | "USLACK"
                ) {
                    return false;
                }
                self.users.get(user_id).is_none_or(|user| {
                    !user.is_bot
                        && !user.extra.get("is_slackbot").is_some_and(pref_truthy)
                        && !matches!(user.id.as_str(), "USLACKBOT" | "USLACK")
                })
            })
            .map(|channel| {
                let bag = self.messages.get(&channel.id);
                let mentions = bag
                    .map(|b| b.mention_count)
                    .or(channel.mention_count)
                    .unwrap_or(0);
                // kmLC client.counts initializes IM/MPIM unreadCnt with
                // mention_count || Number(has_unreads); fgSa uses it as the
                // highlight count. Never substitute all unread message units.
                if mentions > 0 {
                    mentions
                } else {
                    // apply_channel_read_state promotes canonical has_unreads
                    // into bag.unread_count >= 1. Once a bag exists, its newer
                    // cleared state outranks stale channel metadata.
                    u32::from(
                        bag.map(|b| b.unread_count > 0)
                            .unwrap_or(channel.has_unreads),
                    )
                }
            })
            .fold(0, u32::saturating_add)
    }
}

#[cfg(test)]
mod tests;
