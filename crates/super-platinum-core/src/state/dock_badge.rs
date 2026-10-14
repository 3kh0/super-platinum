//! Renderer-neutral Dock badge values. This consumes canonical workspace
//! projections, never rows from a paginated feed or a visible transcript.

mod counts;
mod projection;
pub use counts::DockCountsState;

/// Grid suppression depends on app-level unified-user/MRU context, not merely
/// enterprise_id. Unknown is deliberate until an authenticated owner supplies
/// those source-derived inputs. These values are not persisted across accounts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DockWorkspaceContext {
    #[default]
    Unknown,
    Standalone,
    Unified,
    Legacy {
        /// Full y0HN.d$ union: source-derived fromAnotherTeam channels OR
        /// qtAD organization-channel suppression, not only org-shared IDs.
        suppressed_channels: std::collections::BTreeSet<String>,
        suppress_threads: bool,
        include_later: bool,
    },
}

/// One workspace's `3Y5k` unread contribution to the desktop bridge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DockBadgeContribution {
    pub highlights: u32,
    pub has_unreads: bool,
    pub show_bullet: bool,
}

/// Numeric authority can arrive before Grid ordinary-unread suppression context.
/// Consumers must replace the numeric value even when the optional dot input is
/// unknown; they may retain only the previous same-owner unread input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockBadgeProjection {
    pub highlights: u32,
    pub has_unreads: Option<bool>,
    pub show_bullet: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DockBadge {
    #[default]
    Empty,
    Dot,
    Count(u64),
}

impl DockBadge {
    /// M9601xx sums workspace highlights; M1218022 gives numeric highlights
    /// priority over the optional ordinary-unread bullet. A different workspace
    /// opting into bullets cannot expose the unread state of an opted-out one.
    pub fn aggregate(contributions: impl IntoIterator<Item = DockBadgeContribution>) -> Self {
        let mut highlights = 0u64;
        let mut bullet = false;
        for contribution in contributions {
            highlights = highlights.saturating_add(u64::from(contribution.highlights));
            bullet |= contribution.has_unreads && contribution.show_bullet;
        }
        if highlights > 0 {
            Self::Count(highlights)
        } else if bullet {
            Self::Dot
        } else {
            Self::Empty
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod aggregate_tests {
    use super::*;

    #[test]
    fn numeric_badges_survive_disabled_bullets_and_sum_all_workspaces() {
        assert_eq!(
            DockBadge::aggregate([
                DockBadgeContribution {
                    highlights: u32::MAX,
                    has_unreads: true,
                    show_bullet: false
                },
                DockBadgeContribution {
                    highlights: 5,
                    has_unreads: true,
                    show_bullet: true
                },
            ]),
            DockBadge::Count(u64::from(u32::MAX) + 5)
        );
    }

    #[test]
    fn bullet_requires_unreads_and_opt_in_in_the_same_workspace() {
        let unread = DockBadgeContribution {
            highlights: 0,
            has_unreads: true,
            show_bullet: false,
        };
        let opted_in = DockBadgeContribution {
            highlights: 0,
            has_unreads: false,
            show_bullet: true,
        };
        assert_eq!(DockBadge::aggregate([unread, opted_in]), DockBadge::Empty);
        assert_eq!(
            DockBadge::aggregate([DockBadgeContribution {
                show_bullet: true,
                ..unread
            }]),
            DockBadge::Dot
        );
        assert_eq!(DockBadge::aggregate([]), DockBadge::Empty);
    }
}
