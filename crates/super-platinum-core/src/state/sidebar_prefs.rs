//! The sidebar preferences Slack's own client lays the channel list out from,
//! and the section list they resolve to.
//!
//! Every default here is lifted from the official web client rather than
//! guessed: `getChannelSectionSortOrder`, the per-section sidebar-behaviour
//! default, and `getChannelSectionName`. Slack stores almost nothing on the
//! section itself; the user's choices live in `client.userBoot` prefs.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::slack::models::{BootPrefs, ChannelId, ChannelSection};
use crate::state::{Workspace, linked_list_order};

/// The client-local section Slack synthesizes for VIP unreads. Its prefs are
/// keyed by this literal in the `channel_sections` pref, not by a section id.
pub const PRIORITY_SECTION_ID: &str = "priority";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarConfig {
    /// `users.channelSections.list`, in whatever order Slack sent it. Only the
    /// `next_channel_section_id` chain is display order.
    #[serde(default)]
    pub sections: Vec<ChannelSection>,
    /// The `channel_sections` pref: per-section sort, filter and collapse.
    #[serde(default)]
    pub section_prefs: HashMap<String, SectionPref>,
    /// `hidden_user_group_sections`.
    #[serde(default)]
    pub hidden_sections: HashSet<String>,
    /// `sidebar_behavior`: the global "Show…" choice every section defaults from.
    #[serde(default)]
    pub behavior: String,
    /// `channel_sort`: the global sort every section without its own defaults to.
    #[serde(default)]
    pub channel_sort: String,
    /// `boost_mentions`: conversations with a badge float to the top.
    #[serde(default)]
    pub boost_mentions: bool,
    /// `should_show_connect_section`.
    #[serde(default = "default_true")]
    pub show_connect_section: bool,
    /// `show_muted_items_outside_sidebar_menus`.
    #[serde(default)]
    pub show_muted: bool,
    /// `separate_private_channels`.
    #[serde(default)]
    pub separate_private_channels: bool,
    /// `undo_channel_intermix`: DMs sort after channels within a section.
    #[serde(default)]
    pub undo_channel_intermix: bool,
    /// Channels muted in `all_notifications_prefs`.
    #[serde(default)]
    pub muted: HashSet<ChannelId>,
    /// Slack's `filter_dormant_channels` rollout: sections that would show
    /// every read row instead hide the ones Slack marked dormant. On for every
    /// account it has been seen on.
    #[serde(default = "default_true")]
    pub filter_dormant: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            section_prefs: HashMap::new(),
            hidden_sections: HashSet::new(),
            behavior: String::new(),
            channel_sort: String::new(),
            boost_mentions: false,
            show_connect_section: true,
            show_muted: false,
            separate_private_channels: false,
            undo_channel_intermix: false,
            muted: HashSet::new(),
            filter_dormant: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectionPref {
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub sidebar: Option<String>,
    /// Collapsed: `"1"`. Slack writes it as a string.
    #[serde(default)]
    pub c: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionSort {
    Alpha,
    Recent,
    Priority,
}

/// A section's "Show…" filter (the `sidebar` key of a section pref).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionFilter {
    /// `all`: read conversations stay.
    All,
    /// `hid`: read conversations are hidden.
    HideRead,
    /// `mentions`: only conversations with a badge.
    MentionsOnly,
    /// `active`: read conversations stay unless Slack marked them dormant.
    Active,
}

#[derive(Debug, Clone)]
pub struct ResolvedSection {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub sort: SectionSort,
    pub filter: SectionFilter,
    pub collapsed: bool,
    pub channel_ids: Vec<ChannelId>,
}

impl SidebarConfig {
    /// Read every sidebar pref out of `client.userBoot`. Sections arrive from
    /// their own endpoint and are kept.
    pub fn apply_prefs(&mut self, prefs: &BootPrefs) {
        let flag = |name: &str| prefs.extra.get(name).and_then(serde_json::Value::as_bool);
        let text = |name: &str| {
            prefs
                .extra
                .get(name)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        self.section_prefs = prefs
            .channel_sections
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();
        self.hidden_sections = split_ids(prefs.hidden_user_group_sections.as_deref());
        self.behavior = prefs.sidebar_behavior.clone().unwrap_or_default();
        self.channel_sort = text("channel_sort");
        self.boost_mentions = flag("boost_mentions").unwrap_or(false);
        self.show_connect_section = flag("should_show_connect_section").unwrap_or(true);
        self.show_muted = flag("show_muted_items_outside_sidebar_menus").unwrap_or(false);
        self.separate_private_channels = flag("separate_private_channels").unwrap_or(false);
        self.undo_channel_intermix = flag("undo_channel_intermix").unwrap_or(false);
        self.muted = muted_channels(&text("all_notifications_prefs"));
    }

    pub fn is_muted(&self, channel: &str) -> bool {
        self.muted.contains(channel)
    }

    pub fn is_collapsed(&self, section: &str) -> bool {
        self.section_prefs
            .get(section)
            .and_then(|pref| pref.c.as_deref())
            .is_some_and(|c| c == "1" || c == "true")
    }

    /// Flip a section's collapsed state locally. Slack's copy of the pref is
    /// not written; the next boot reasserts it.
    pub fn toggle_collapsed(&mut self, section: &str) {
        let collapsed = !self.is_collapsed(section);
        self.section_prefs.entry(section.to_owned()).or_default().c =
            Some(if collapsed { "1" } else { "0" }.to_owned());
    }

    fn filter(&self, id: &str, kind: &str) -> SectionFilter {
        let explicit = self
            .section_prefs
            .get(id)
            .and_then(|pref| pref.sidebar.as_deref());
        match explicit.unwrap_or_else(|| default_filter(&self.behavior, kind, self.filter_dormant))
        {
            "hid" => SectionFilter::HideRead,
            "mentions" => SectionFilter::MentionsOnly,
            "active" => SectionFilter::Active,
            _ => SectionFilter::All,
        }
    }

    fn sort(&self, id: &str, kind: &str) -> SectionSort {
        let explicit = self
            .section_prefs
            .get(id)
            .and_then(|pref| pref.sort.as_deref());
        let sort = match explicit {
            Some(sort @ ("priority" | "default" | "recent")) => sort,
            // Slack's DM-recency default and its code-channel default.
            None if is_dm_section(kind) || kind == "coding_channels" => "recent",
            _ => match self.channel_sort.as_str() {
                "priority" => "priority",
                "recent" => "recent",
                "recentDms" if is_dm_section(kind) => "recent",
                _ => "default",
            },
        };
        match sort {
            "recent" => SectionSort::Recent,
            "priority" => SectionSort::Priority,
            _ => SectionSort::Alpha,
        }
    }
}

/// The per-section sidebar-behaviour default for a section with no pref of
/// its own, given the global `sidebar_behavior`. Where Slack would show every
/// row it shows every *active* row once dormant filtering is on.
fn default_filter(global: &str, kind: &str, filter_dormant: bool) -> &'static str {
    let all = if filter_dormant { "active" } else { "all" };
    match global {
        "hide_inactive_channels" => "active",
        _ if kind == "coding_channels" => all,
        "hide_read_channels" => "hid",
        "" => "all",
        "show_mentions_only" if is_dm_section(kind) => "hid",
        "show_mentions_only" => "mentions",
        "hide_read_channels_unless_starred" => match kind {
            "channels" | "direct_messages" | "org_channels" | "recent_apps" | "shared_channels"
            | "slack_connect" => "hid",
            _ => all,
        },
        _ => "all",
    }
}

pub(crate) fn is_dm_section(kind: &str) -> bool {
    kind == "direct_messages"
}

fn split_ids(value: Option<&str>) -> HashSet<String> {
    value
        .unwrap_or_default()
        .split(',')
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `all_notifications_prefs` is a JSON string: `{"channels":{"C1":{"muted":true}}}`.
fn muted_channels(json: &str) -> HashSet<ChannelId> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return HashSet::new();
    };
    value
        .get("channels")
        .and_then(serde_json::Value::as_object)
        .map(|channels| {
            channels
                .iter()
                .filter(|(_, pref)| {
                    pref.get("muted").and_then(serde_json::Value::as_bool) == Some(true)
                })
                .map(|(id, _)| id.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// `getChannelSectionName`: Slack renames its built-in sections client-side,
/// so the server's `"Direct Messages"` and `"Recent Apps"` never show.
fn section_title(section: &ChannelSection) -> Option<String> {
    Some(
        match section.kind.as_str() {
            "channels" => "Channels",
            "direct_messages" => "Direct messages",
            "recent_apps" => "Agents & apps",
            "agents" => "Agents",
            "coding_channels" => "Code channels",
            "slack_connect" => "External connections",
            "stars" => "Starred",
            "salesforce_records" | "temporary_channels" | "quip" | "canvases_and_files" => {
                return None;
            }
            _ => return Some(section.name.clone()),
        }
        .to_owned(),
    )
}

impl Workspace {
    pub fn apply_channel_sections(&mut self, page: crate::slack::models::ChannelSectionsPage) {
        self.sidebar.sections = page.channel_sections;
    }

    /// Sections in display order: VIP unreads first when the pref is on, then
    /// the server's chain. Channels and Direct messages are synthesized when
    /// missing so a conversation never has nowhere to go.
    pub fn resolved_sidebar_sections(&self) -> Vec<ResolvedSection> {
        let mut out = Vec::new();
        if self.priority_sidebar_section {
            out.push(self.resolve_section(PRIORITY_SECTION_ID, "priority", "VIP unreads", &[]));
        }
        for section in linked_list_order(&self.sidebar.sections) {
            if section.is_hidden
                || self
                    .sidebar
                    .hidden_sections
                    .contains(&section.channel_section_id)
            {
                continue;
            }
            let Some(title) = section_title(section) else {
                continue;
            };
            out.push(self.resolve_section(
                &section.channel_section_id,
                &section.kind,
                &title,
                &section.channel_ids_page.channel_ids,
            ));
        }
        let synthesized: &[(&str, &str)] = if self.sidebar.sections.is_empty() {
            &[
                ("direct_messages", "Direct messages"),
                ("slack_connect", "External connections"),
                ("stars", "Starred"),
                ("recent_apps", "Agents & apps"),
                ("channels", "Channels"),
            ]
        } else {
            &[
                ("direct_messages", "Direct messages"),
                ("channels", "Channels"),
            ]
        };
        for (kind, title) in synthesized {
            if !out.iter().any(|s| s.kind == *kind) {
                let ids = if *kind == "stars" {
                    self.starred_order.clone()
                } else {
                    Vec::new()
                };
                out.push(self.resolve_section(kind, kind, title, &ids));
            }
        }
        out
    }

    fn resolve_section(
        &self,
        id: &str,
        kind: &str,
        title: &str,
        channel_ids: &[ChannelId],
    ) -> ResolvedSection {
        ResolvedSection {
            id: id.to_owned(),
            kind: kind.to_owned(),
            title: title.to_owned(),
            sort: self.sidebar.sort(id, kind),
            filter: self.sidebar.filter(id, kind),
            collapsed: self.sidebar.is_collapsed(id),
            channel_ids: channel_ids.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(json: serde_json::Value) -> BootPrefs {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn reads_every_sidebar_pref_from_user_boot() {
        let mut config = SidebarConfig::default();
        config.apply_prefs(&prefs(serde_json::json!({
            "sidebar_behavior": "hide_read_channels_unless_starred",
            "channel_sort": "default",
            "boost_mentions": true,
            "should_show_connect_section": true,
            "channel_sections": r#"{"L1":{"c":"1","sidebar":"all"},"priority":{"sort":"recent"}}"#,
            "hidden_user_group_sections": "L2,L3",
            "all_notifications_prefs": r#"{"channels":{"C1":{"muted":true},"C2":{"muted":false}}}"#,
        })));

        assert!(config.boost_mentions);
        assert!(config.is_collapsed("L1"));
        assert!(!config.is_collapsed("L4"));
        assert_eq!(config.hidden_sections.len(), 2);
        assert!(config.is_muted("C1"));
        assert!(!config.is_muted("C2"));
        assert_eq!(config.filter("L1", "channels"), SectionFilter::All);
        assert_eq!(config.sort("priority", "priority"), SectionSort::Recent);
    }

    #[test]
    fn unless_starred_hides_read_rows_in_builtin_sections_only() {
        let config = SidebarConfig {
            behavior: "hide_read_channels_unless_starred".into(),
            ..Default::default()
        };
        assert_eq!(config.filter("L", "channels"), SectionFilter::HideRead);
        assert_eq!(
            config.filter("L", "direct_messages"),
            SectionFilter::HideRead
        );
        assert_eq!(config.filter("L", "slack_connect"), SectionFilter::HideRead);
        assert_eq!(config.filter("L", "stars"), SectionFilter::Active);
        assert_eq!(config.filter("L", "standard"), SectionFilter::Active);
    }

    #[test]
    fn section_sort_falls_back_to_global_channel_sort() {
        let mut config = SidebarConfig {
            channel_sort: "priority".into(),
            ..Default::default()
        };
        assert_eq!(config.sort("L", "channels"), SectionSort::Priority);
        assert_eq!(config.sort("L", "direct_messages"), SectionSort::Recent);
        config.section_prefs.insert(
            "L".into(),
            SectionPref {
                sort: Some("default".into()),
                ..Default::default()
            },
        );
        assert_eq!(config.sort("L", "channels"), SectionSort::Alpha);
    }

    #[test]
    fn toggling_collapse_flips_the_local_pref() {
        let mut config = SidebarConfig::default();
        config.toggle_collapsed("L1");
        assert!(config.is_collapsed("L1"));
        config.toggle_collapsed("L1");
        assert!(!config.is_collapsed("L1"));
    }
}
