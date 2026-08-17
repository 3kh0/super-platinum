//! Renderer-neutral Slack sidebar section grouping and ordering.
//!
//! Shells share the same section membership, sort modes, and visibility rules.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::slack::models::Channel;
use crate::state::{SectionSort, Workspace, channel_display_name, is_vip_channel};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarSection {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub channel_ids: Vec<String>,
}

/// Group workspace conversations into Slack sidebar sections with section-local
/// sort modes. Empty sections are retained only when needed for testing; callers
/// typically skip empty groups when rendering.
pub fn grouped_sidebar_sections(ws: &Workspace, active: Option<&str>) -> Vec<SidebarSection> {
    let sections = ws.resolved_sidebar_sections();

    let mut custom: HashMap<&str, usize> = HashMap::new();
    for (i, section) in sections.iter().enumerate() {
        for id in &section.channel_ids {
            custom.entry(id.as_str()).or_insert(i);
        }
    }
    let index_of = |kind: &str| sections.iter().position(|s| s.kind == kind);
    let vip = index_of("priority");
    let stars = index_of("stars");
    let dms = index_of("direct_messages");
    let connect = index_of("slack_connect");
    let channels = index_of("channels");

    let mut groups: Vec<Vec<&Channel>> = vec![Vec::new(); sections.len()];
    for c in ws.channels.values() {
        if c.is_archived {
            continue;
        }
        let target = if vip.is_some() && is_vip_channel(ws, c) && has_unreads(ws, c) {
            vip
        } else if let Some(&section) = custom.get(c.id.as_str()) {
            Some(section)
        } else if ws.is_starred_channel(c) && stars.is_some() {
            stars
        } else if c.is_im || c.is_mpim {
            dms
        } else if c.is_ext_shared && connect.is_some() {
            connect
        } else {
            channels
        };
        let Some(target) = target else {
            continue;
        };
        let visible = sections[target].show_all
            || has_unreads(ws, c)
            || active == Some(c.id.as_str())
            || ws.should_show_unstarred_read_channels();
        if visible {
            groups[target].push(c);
        }
    }

    sections
        .into_iter()
        .zip(groups)
        .map(|(section, mut channels)| {
            match section.sort {
                SectionSort::Recent => sort_recent_section(&mut channels, ws),
                SectionSort::Priority => sort_priority_section(&mut channels, ws),
                SectionSort::Alpha => sort_alpha_section(&mut channels, ws),
            }
            SidebarSection {
                id: section.id,
                kind: section.kind,
                title: section.title,
                channel_ids: channels.into_iter().map(|c| c.id.clone()).collect(),
            }
        })
        .collect()
}

fn sort_recent_section(channels: &mut [&Channel], ws: &Workspace) {
    channels.sort_by(|a, b| {
        ws.channel_recency(b)
            .cmp(&ws.channel_recency(a))
            .then_with(|| name_cmp(ws, a, b))
    });
}

fn sort_priority_section(channels: &mut [&Channel], ws: &Workspace) {
    channels.sort_by(|a, b| {
        ws.priority_score(&b.id)
            .unwrap_or(0.0)
            .total_cmp(&ws.priority_score(&a.id).unwrap_or(0.0))
            .then_with(|| name_cmp(ws, a, b))
    });
}

fn sort_alpha_section(channels: &mut [&Channel], ws: &Workspace) {
    channels.sort_by(|a, b| {
        (mention_count(ws, b) > 0)
            .cmp(&(mention_count(ws, a) > 0))
            .then_with(|| name_cmp(ws, a, b))
    });
}

fn name_cmp(ws: &Workspace, a: &Channel, b: &Channel) -> Ordering {
    natural_cmp(&channel_display_name(ws, a), &channel_display_name(ws, b))
        .then_with(|| a.id.cmp(&b.id))
}

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        return match (a.peek().copied(), b.peek().copied()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                match take_number(&mut a).cmp(&take_number(&mut b)) {
                    Ordering::Equal => continue,
                    unequal => unequal,
                }
            }
            (Some(x), Some(y)) => match x.to_lowercase().cmp(y.to_lowercase()) {
                Ordering::Equal => {
                    a.next();
                    b.next();
                    continue;
                }
                unequal => unequal,
            },
        };
    }
}

fn take_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut n: u64 = 0;
    while let Some(c) = chars.peek().and_then(|c| c.to_digit(10)) {
        n = n.saturating_mul(10).saturating_add(u64::from(c));
        chars.next();
    }
    n
}

fn mention_count(ws: &Workspace, c: &Channel) -> u32 {
    ws.messages
        .get(&c.id)
        .map(|cm| cm.mention_count)
        .or(c.mention_count)
        .unwrap_or(0)
}

fn has_unreads(ws: &Workspace, c: &Channel) -> bool {
    ws.unread_total(c) > 0
}

/// Number of people in a group DM, parsed from the `mpdm-a--b--c-1` name.
pub fn group_member_count(c: &Channel) -> Option<usize> {
    let name = c.name.as_deref()?;
    let rest = name.strip_prefix("mpdm-")?;
    let rest = rest
        .rsplit_once('-')
        .and_then(|(prefix, suffix)| suffix.parse::<u32>().ok().map(|_| prefix))
        .unwrap_or(rest);
    let count = rest
        .split("--")
        .filter(|name| !name.trim().is_empty())
        .count();
    (count > 0).then_some(count)
}

/// Topic or purpose text stored on a channel when Slack provides it.
pub fn channel_topic_or_purpose(c: &Channel) -> Option<String> {
    for key in ["topic", "purpose"] {
        if let Some(value) = c.extra.get(key)
            && let Some(text) = value
                .get("value")
                .and_then(|v| v.as_str())
                .or_else(|| value.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            return Some(text.to_owned());
        }
    }
    None
}

pub fn is_app_message(msg: &crate::slack::models::Message) -> bool {
    msg.subtype.as_deref() == Some("bot_message")
        || msg.bot_id.is_some()
        || msg.bot_profile.is_some()
}

/// Consecutive messages from the same author within five minutes on the same day.
pub fn same_message_group(
    previous: &crate::slack::models::Message,
    current: &crate::slack::models::Message,
) -> bool {
    let has_author =
        previous.user.is_some() || previous.bot_id.is_some() || previous.username.is_some();
    if !has_author {
        return false;
    }
    let same_author = previous.user.as_deref() == current.user.as_deref()
        && previous.bot_id.as_deref() == current.bot_id.as_deref()
        && previous.username.as_deref() == current.username.as_deref();
    if !same_author {
        return false;
    }
    let (Some(previous_ts), Some(current_ts)) = (previous.ts.as_deref(), current.ts.as_deref())
    else {
        return false;
    };
    if crate::state::date_key_for_ts(previous_ts) != crate::state::date_key_for_ts(current_ts) {
        return false;
    }
    let previous_secs = crate::state::ts_key(previous_ts).0;
    let current_secs = crate::state::ts_key(current_ts).0;
    current_secs.saturating_sub(previous_secs) <= 300
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use serde_json::json;

    use super::*;
    use crate::slack::models::Channel;
    use crate::state::{ChannelMessages, RealtimeStatus, Workspace, channel_display_name};

    fn channel(id: &str, name: &str) -> Channel {
        Channel {
            id: id.into(),
            name: Some(name.into()),
            is_channel: true,
            ..Default::default()
        }
    }

    fn dm(id: &str, name: &str) -> Channel {
        Channel {
            id: id.into(),
            name: Some(name.into()),
            is_im: true,
            ..Default::default()
        }
    }

    fn workspace(channels: Vec<Channel>, unreads: &[(&str, u32)]) -> Workspace {
        let mut messages = HashMap::new();
        for (channel, count) in unreads {
            messages.insert(
                (*channel).to_owned(),
                ChannelMessages {
                    unread_count: *count,
                    ..Default::default()
                },
            );
        }
        Workspace {
            team_id: "T".into(),
            name: "Test".into(),
            url: "https://test.slack.com".into(),
            self_user_id: "U_SELF".into(),
            activity_unread_count: None,
            channels: channels
                .into_iter()
                .map(|channel| (channel.id.clone(), channel))
                .collect::<BTreeMap<_, _>>(),
            starred_order: Vec::new(),
            dm_order: Vec::new(),
            recent_channels: Vec::new(),
            last_active_channel: None,
            priority_scores: BTreeMap::new(),
            frecency: BTreeMap::new(),
            hide_read_channels_unless_starred: false,
            priority_sidebar_section: false,
            vip_users: std::collections::HashSet::new(),
            sidebar: Default::default(),
            users: HashMap::new(),
            custom_emoji: HashMap::new(),
            messages,
            typing: HashMap::new(),
            presence: HashMap::new(),
            active_huddles: HashMap::new(),
            rt: RealtimeStatus::default(),
            rt_generation: 0,
        }
    }

    fn section_ids(groups: &[SidebarSection], title: &str) -> Vec<String> {
        groups
            .iter()
            .find(|group| group.title == title)
            .map(|group| group.channel_ids.clone())
            .unwrap_or_default()
    }

    fn section_titles(groups: &[SidebarSection]) -> Vec<String> {
        groups
            .iter()
            .filter(|group| !group.channel_ids.is_empty())
            .map(|group| group.title.clone())
            .collect()
    }

    #[test]
    fn groups_sidebar_like_slack_unread_priority() {
        let mut vip = channel("C_VIP", "lilys-nest");
        vip.extra
            .insert("sidebar_section_name".into(), json!("VIP unreads"));
        let mut starred = channel("C_STAR", "announcements");
        starred.is_starred = true;
        let quiet = channel("C_QUIET", "general");
        let unread = channel("C_UNREAD", "community");
        let active_read = channel("C_ACTIVE", "community-logs");
        let mut ws = workspace(
            vec![
                quiet,
                unread,
                starred,
                vip,
                active_read,
                dm("D_ALICE", "alice"),
            ],
            &[("C_VIP", 1), ("C_UNREAD", 2)],
        );
        ws.hide_read_channels_unless_starred = true;
        ws.priority_sidebar_section = true;

        let sections = grouped_sidebar_sections(&ws, Some("C_ACTIVE"));

        assert_eq!(section_ids(&sections, "VIP unreads"), ["C_VIP"]);
        assert!(section_ids(&sections, "Direct messages").is_empty());
        assert_eq!(section_ids(&sections, "Starred"), ["C_STAR"]);
        assert_eq!(section_ids(&sections, "Channels"), ["C_UNREAD", "C_ACTIVE"]);
    }

    #[test]
    fn external_channels_get_their_own_section_and_stay_visible_when_read() {
        let mut ext = channel("C_EXT", "vercel-embassy");
        ext.is_ext_shared = true;
        let read = channel("C_READ", "quiet");
        let mut ws = workspace(vec![ext, read], &[]);
        ws.hide_read_channels_unless_starred = true;

        let sections = grouped_sidebar_sections(&ws, None);

        assert_eq!(section_ids(&sections, "External connections"), ["C_EXT"]);
        assert!(section_ids(&sections, "Channels").is_empty());
    }

    #[test]
    fn hides_read_dms_when_slack_prefers_unread_sidebar() {
        let mut ws = workspace(
            vec![dm("D_READ", "read"), dm("D_UNREAD", "unread")],
            &[("D_UNREAD", 1)],
        );
        ws.hide_read_channels_unless_starred = true;

        let sections = grouped_sidebar_sections(&ws, None);

        assert_eq!(section_ids(&sections, "Direct messages"), ["D_UNREAD"]);
    }

    #[test]
    fn natural_sort_orders_numeric_suffixes() {
        assert_eq!(natural_cmp("room2", "room10"), Ordering::Less);
        assert_eq!(natural_cmp("Room10", "room2"), Ordering::Greater);
    }

    #[test]
    fn group_member_count_parses_mpdm_name() {
        let mut mpdm = dm("G_MPDM", "mpdm-aarav54897--echo--alanlichen1-1");
        mpdm.is_im = false;
        mpdm.is_mpim = true;
        assert_eq!(group_member_count(&mpdm), Some(3));

        let public = channel("C_PUBLIC", "general");
        assert_eq!(group_member_count(&public), None);
    }

    #[test]
    fn dm_labels_do_not_use_private_channel_formatting() {
        let mut mpdm = dm("G_MPDM", "mpdm-aarav54897--echo--alanlichen1-1");
        mpdm.is_im = false;
        mpdm.is_mpim = true;
        mpdm.is_group = true;
        mpdm.is_private = true;
        let ws = workspace(vec![mpdm], &[]);

        let label = channel_display_name(&ws, ws.channels.get("G_MPDM").unwrap());

        assert_eq!(label.trim(), "aarav54897, echo, alanlichen1");
        assert!(!label.contains("mpdm-"));
    }

    #[test]
    fn hidden_sections_stay_hidden() {
        use crate::slack::models::{ChannelIdsPage, ChannelSection};

        let mut ws = workspace(vec![channel("C_G", "grouped")], &[("C_G", 1)]);
        ws.apply_channel_sections(crate::slack::models::ChannelSectionsPage {
            channel_sections: vec![ChannelSection {
                channel_section_id: "L_UG".into(),
                name: "helpers".into(),
                kind: "user_group".into(),
                channel_ids_page: ChannelIdsPage {
                    channel_ids: vec!["C_G".into()],
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        });
        ws.sidebar.hidden_sections.insert("L_UG".into());

        let sections = grouped_sidebar_sections(&ws, None);

        assert!(!section_titles(&sections).contains(&"helpers".to_string()));
    }
}
