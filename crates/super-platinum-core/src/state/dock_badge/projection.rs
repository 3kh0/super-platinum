use super::{DockBadgeContribution, DockBadgeProjection, DockWorkspaceContext};
use crate::state::{Workspace, notifications::pref_truthy};
use serde_json::Value;

fn truthy_default(value: Option<&Value>, default: bool) -> bool {
    value.filter(|v| !v.is_null()).map_or(default, pref_truthy)
}

impl Workspace {
    /// Exact 3Y5k projection when its dependencies are authoritative. Missing
    /// Grid suppression context or canonical counts is unknown, never raw-summed.
    pub fn dock_badge_contribution(&self) -> Option<DockBadgeContribution> {
        let projection = self.dock_badge_projection()?;
        Some(DockBadgeContribution {
            highlights: projection.highlights,
            has_unreads: projection.has_unreads?,
            show_bullet: projection.show_bullet,
        })
    }

    pub fn dock_badge_projection(&self) -> Option<DockBadgeProjection> {
        if !self.notifications.dock_loaded {
            return None;
        }
        let inbox = self.activity_inbox_enabled()?;
        let show_bullet = self
            .notifications
            .user
            .get("mac_ssb_bullet")
            .is_some_and(pref_truthy);
        let (highlights, has_unreads) = if inbox {
            let activity = self.activity_badge_count()?;
            let later = if truthy_default(
                self.notifications
                    .global
                    .get("global_reminders_in_activity"),
                true,
            ) || !self.dock_later_visible()
            {
                0
            } else {
                self.dock_counts.overdue_count()?
            };
            let dm = if self
                .notifications
                .user
                .get("activity_dms_opt_out")
                .is_some_and(pref_truthy)
            {
                self.dm_activity_badge_count()?
            } else {
                0
            };
            let total = activity.saturating_add(later).saturating_add(dm);
            let unread = if show_bullet && activity == 0 {
                self.dock_ordinary_unreads()
            } else {
                Some(total > 0)
            };
            (total, unread)
        } else {
            if !self.dock_counts.counts_loaded {
                return None;
            }
            let badges = self.dock_counts.channel_badges()?;
            let policy = self.activity_counts.experiments()?;
            let include_threads = (policy.group("thread_badging_home_sidebar") == Some("on")
                && policy.group("dual_badging_in_home_sidebar") != Some("on"))
                || self
                    .notifications
                    .user
                    .get("activity_include_threads_in_badge_count")
                    .is_some_and(pref_truthy);
            let mut total = ["channels", "thread_mentions", "app_dms"]
                .into_iter()
                .map(|key| badges.get(key).copied().unwrap_or(0))
                .fold(0, u32::saturating_add);
            if include_threads {
                total = total.saturating_add(badges.get("thread_unreads").copied().unwrap_or(0));
            }
            // JOlo excludes apps/Slackbot, but unlike the DM rail does not
            // exclude USEARCH or USLACKSECURITY here.
            total = total.saturating_add(self.dock_dm_highlights());
            let include_later = match &self.dock_counts.context {
                DockWorkspaceContext::Standalone | DockWorkspaceContext::Unified => true,
                DockWorkspaceContext::Legacy { include_later, .. } => *include_later,
                DockWorkspaceContext::Unknown => return None,
            };
            if include_later {
                total = total.saturating_add(self.dock_counts.overdue_count()?);
            }
            (total, self.dock_ordinary_unreads())
        };
        Some(DockBadgeProjection {
            highlights,
            has_unreads,
            show_bullet,
        })
    }

    /// Later defaults true and is never a mandatory-tab override (3RLp).
    fn dock_later_visible(&self) -> bool {
        let parsed = self
            .notifications
            .user
            .get("tab_rail_customization")
            .and_then(Value::as_str)
            .and_then(|s| serde_json::from_str::<Value>(s).ok());
        truthy_default(parsed.as_ref().and_then(|v| v.get("later")), true)
    }

    fn dock_channel_eligible(&self, channel: &crate::slack::models::Channel) -> bool {
        if self.sidebar.is_muted(&channel.id) {
            return false;
        }
        if channel.is_im {
            return true;
        }
        !channel.is_archived
            && !channel.extra.get("is_file").is_some_and(pref_truthy)
            && (channel.is_mpim || channel.extra.get("is_member").is_some_and(pref_truthy))
    }

    fn dock_dm_highlights(&self) -> u32 {
        self.channels
            .values()
            .filter(|c| (c.is_im || c.is_mpim) && self.dock_channel_eligible(c))
            .filter(|c| {
                !c.is_im || {
                    let id = c.user.as_deref().unwrap_or("");
                    !matches!(id, "USLACKBOT" | "USLACK")
                        && self.users.get(id).is_none_or(|user| {
                            !user.is_bot
                                && !user.extra.get("is_slackbot").is_some_and(pref_truthy)
                                && !matches!(user.id.as_str(), "USLACKBOT" | "USLACK")
                        })
                }
            })
            .map(|c| {
                let bag = self.messages.get(&c.id);
                let highlights = bag
                    .map(|b| b.mention_count)
                    .or(c.mention_count)
                    .unwrap_or(0);
                if highlights > 0 {
                    highlights
                } else {
                    u32::from(bag.map(|b| b.unread_count > 0).unwrap_or(c.has_unreads))
                }
            })
            .fold(0, u32::saturating_add)
    }

    fn dock_ordinary_unreads(&self) -> Option<bool> {
        if !self.dock_counts.counts_loaded {
            return None;
        }
        let mut unknown = false;
        for channel in self
            .channels
            .values()
            .filter(|c| self.dock_channel_eligible(c))
        {
            let unread = self
                .messages
                .get(&channel.id)
                .map(|b| {
                    b.unread_count > 0
                        || ((channel.is_im || channel.is_mpim) && b.mention_count > 0)
                })
                .unwrap_or(channel.has_unreads);
            if !unread {
                continue;
            }
            // Suppression precedes ALL channel contributions, including org
            // DMs. JOlo's numeric DM loop deliberately does not use this map.
            if let DockWorkspaceContext::Legacy {
                suppressed_channels,
                ..
            } = &self.dock_counts.context
            {
                if suppressed_channels.contains(&channel.id) {
                    continue;
                }
            }
            // setFromAnotherTeam derives false for IM/MPIM and all standalone
            // or unified contexts. Never trust a raw API/cache convenience flag.
            if (channel.is_im || channel.is_mpim)
                && !channel.extra.get("is_org_shared").is_some_and(pref_truthy)
            {
                return Some(true);
            }
            match &self.dock_counts.context {
                DockWorkspaceContext::Standalone | DockWorkspaceContext::Unified => {
                    return Some(true);
                }
                DockWorkspaceContext::Legacy {
                    suppressed_channels,
                    ..
                } if !suppressed_channels.contains(&channel.id) => return Some(true),
                DockWorkspaceContext::Unknown => unknown = true,
                _ => {}
            }
        }
        match &self.dock_counts.context {
            DockWorkspaceContext::Legacy {
                suppress_threads: true,
                ..
            } => {}
            DockWorkspaceContext::Unknown => {
                if self.thread_counts.snapshot.as_ref().is_none_or(|t| {
                    t.has_unreads || t.unread_count_by_channel.values().any(|v| *v > 0)
                }) {
                    unknown = true;
                }
            }
            _ => {
                let threads = self.thread_counts.snapshot.as_ref()?;
                if threads.has_unreads || threads.unread_count_by_channel.values().any(|v| *v > 0) {
                    return Some(true);
                }
            }
        }
        if unknown { None } else { Some(false) }
    }
}
