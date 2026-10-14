//! Exact Inbox category policy from captured S:fexP and S:SWeb.
use std::collections::BTreeMap;

pub const BADGE_EXPERIMENT_NAMES: &[&str] = &[
    "activity_inbox",
    "thread_badging_home_sidebar",
    "dual_badging_in_home_sidebar",
    "lists_monster_step",
    "approvals_in_todos",
    "unjoined_channel_mention_in_activity",
    "quietly_added_to_channel_in_activity",
    "generic_system_user_alert",
    "inviter_joiner_first_dm_v2",
];

/// A successfully read complete assignment subset. Unknown group strings stay
/// intact; they do not become `on`. Missing assignments in this snapshot are off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BadgeExperiments {
    groups: BTreeMap<String, String>,
}

impl BadgeExperiments {
    pub(crate) fn from_groups(groups: BTreeMap<String, String>) -> Self {
        Self { groups }
    }

    pub fn group(&self, name: &str) -> Option<&str> {
        self.groups.get(name).map(String::as_str)
    }

    pub fn inbox_enabled(&self) -> bool {
        self.enabled("activity_inbox")
    }

    fn enabled(&self, name: &str) -> bool {
        self.group(name) == Some("on")
    }

    pub(super) fn includes(&self, category: &str, dm_opt_out: bool) -> bool {
        match category {
            "thread_v2"
            | "channel"
            | "internal_channel_invite"
            | "list_record_edited"
            | "bot_dm_bundle"
            | "at_user"
            | "at_user_group"
            | "at_channel"
            | "at_everyone"
            | "keyword"
            | "list_record_assigned"
            | "list_user_mentioned"
            | "external_channel_invite"
            | "external_dm_invite"
            | "shared_workspace_invite"
            | "prejoin_dm_welcome_party_alert"
            | "saved_reminder" => true,
            "dm" => !dm_opt_out,
            "list_todo_notification" => self.enabled("lists_monster_step"),
            "list_approval_request" | "list_approval_reviewed" => {
                self.enabled("approvals_in_todos")
            }
            "unjoined_channel_mention" => self.enabled("unjoined_channel_mention_in_activity"),
            "quietly_added_to_channel" => self.enabled("quietly_added_to_channel_in_activity"),
            "generic_system_alert" => self.enabled("generic_system_user_alert"),
            "team_joiner_invite" => self.group("inviter_joiner_first_dm_v2") == Some("treatment"),
            // Reactions are explicitly excluded by fexP after its allowlist.
            // Unknown categories remain stored, but are not guessed eligible.
            _ => false,
        }
    }
}

impl super::super::Workspace {
    pub fn activity_inbox_enabled(&self) -> Option<bool> {
        self.activity_counts
            .experiments()
            .map(BadgeExperiments::inbox_enabled)
    }

    /// Exact Inbox projection only. Legacy/unknown policy or missing category
    /// authority returns None; callers decide their explicit compatibility UI.
    pub fn activity_badge_count(&self) -> Option<u32> {
        let policy = self.activity_counts.experiments()?;
        if !policy.inbox_enabled() {
            return None;
        }
        let opted_out = self
            .notifications
            .user
            .get("activity_dms_opt_out")
            .is_some_and(super::super::notifications::pref_truthy);
        Some(
            self.activity_counts
                .snapshot
                .as_ref()?
                .categories()
                .iter()
                .filter(|(category, _)| policy.includes(category, opted_out))
                .map(|(_, count)| *count)
                .fold(0, u32::saturating_add),
        )
    }

    pub fn dm_activity_badge_count(&self) -> Option<u32> {
        if self.activity_inbox_enabled() != Some(true) {
            return None;
        }
        Some(
            self.activity_counts
                .snapshot
                .as_ref()?
                .categories()
                .get("dm")
                .copied()
                .unwrap_or(0),
        )
    }
}
