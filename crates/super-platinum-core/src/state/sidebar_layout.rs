//! Slack's channel sidebar, laid out the way the official client does it.
//!
//! This is a port of the web client's `useCalculateSidebarSections` and the
//! helpers it calls (read from the shipped bundle, not guessed): which
//! conversations are candidates at all, which section each one lands in, which
//! rows a section's filter hides, and the sort-group-then-comparator ordering
//! inside a section. Names in comments are the client's own.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use icu_collator::CollatorPreferences;
use icu_collator::options::CollatorOptions;
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed};

use crate::slack::models::{Channel, ChannelId};
use crate::state::{
    PRIORITY_SECTION_ID, ResolvedSection, SectionFilter, SectionSort, Workspace,
    channel_display_name, dm_user_id, is_dm_section,
};

const SLACKBOT: &str = "USLACKBOT";
const SYSTEM_NOTIFICATIONS: &str = "USLACK";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarSection {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub collapsed: bool,
    pub channel_ids: Vec<ChannelId>,
}

/// What Slack's client carries from one layout to the next. Without it, the
/// conversation being read would jump sections the moment it is marked read.
#[derive(Debug, Clone, Default)]
pub struct SidebarMemory {
    /// Rows in VIP unreads last time (`PrioritySectionChannelManager`): the
    /// open conversation stays there after it stops qualifying.
    pub priority_rows: HashSet<ChannelId>,
    /// The sort group the open conversation had when it was opened.
    pub selected_group: Option<(ChannelId, u8)>,
}

/// Sort groups, in display order within a section (the client's `ey` table).
mod group {
    pub const SLACKBOT_MENTION: u8 = 0;
    pub const YOU_MENTION: u8 = 1;
    pub const MENTIONS: u8 = 2;
    pub const PRIVATE_MENTIONS: u8 = 3;
    pub const IMS_MENTIONS: u8 = 4;
    pub const SLACKBOT: u8 = 5;
    pub const YOU: u8 = 6;
    pub const EVERYTHING_ELSE: u8 = 10;
    pub const PRIVATE: u8 = 11;
    pub const IMS: u8 = 12;
    pub const YOU_NEW_TEAM: u8 = 13;
}

/// Lay out the sidebar. Sections come back in display order, including the
/// empty ones Slack still shows a heading for.
pub fn grouped_sidebar_sections(
    ws: &Workspace,
    active: Option<&str>,
    memory: &mut SidebarMemory,
) -> Vec<SidebarSection> {
    let sections = ws.resolved_sidebar_sections();
    let index_of = |kind: &str| sections.iter().position(|s| s.kind == kind);
    let stars = index_of("stars");
    let starred: HashSet<&str> = stars
        .map(|i| sections[i].channel_ids.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let is_starred = |c: &Channel| {
        starred.contains(c.id.as_str())
            || (ws.sidebar.sections.is_empty() && ws.is_starred_channel(c))
    };
    let mut membership: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, section) in sections.iter().enumerate() {
        for id in &section.channel_ids {
            membership.entry(id.as_str()).or_default().push(i);
        }
    }

    let candidates: Vec<&Channel> = ws
        .channels
        .values()
        .filter(|c| {
            is_candidate(
                ws,
                c,
                active,
                is_starred(c) || membership.contains_key(c.id.as_str()),
            )
        })
        .collect();

    let priority = index_of("priority");
    let mut assigned: Vec<Vec<&Channel>> = vec![Vec::new(); sections.len()];
    let mut priority_rows = HashSet::new();
    for &c in &candidates {
        if let Some(priority) = priority {
            let selected = active == Some(c.id.as_str());
            if (selected && memory.priority_rows.contains(&c.id)) || is_priority_eligible(ws, c) {
                priority_rows.insert(c.id.clone());
                assigned[priority].push(c);
                continue;
            }
        }
        let target = if is_starred(c) && stars.is_some() {
            stars
        } else {
            custom_section(&sections, membership.get(c.id.as_str()))
                .or_else(|| default_section(ws, c, &index_of))
        };
        if let Some(target) = target {
            assigned[target].push(c);
        }
    }
    memory.priority_rows = priority_rows;

    let has_starred = candidates.iter().any(|c| is_starred(c));
    let has_shared = candidates.iter().any(|c| c.is_ext_shared);
    let mut out = Vec::new();
    for (section, channels) in sections.iter().zip(assigned) {
        let shown = match section.kind.as_str() {
            "stars" => has_starred,
            "slack_connect" => ws.sidebar.show_connect_section && has_shared,
            "user_group" => !channels.is_empty(),
            "priority" => !ws.vip_users.is_empty() || !channels.is_empty(),
            _ => true,
        };
        if !shown {
            continue;
        }
        let mut rows = Vec::new();
        let mut hidden_read = 0;
        for c in channels {
            match row_visibility(ws, section, c, active) {
                Row::Shown => rows.push(c),
                Row::HiddenRead => hidden_read += 1,
                Row::Hidden => {}
            }
        }
        if matches!(
            section.kind.as_str(),
            "agents" | "priority" | "coding_channels"
        ) && rows.is_empty()
            && hidden_read == 0
        {
            continue;
        }
        let channel_ids = sort_section(ws, section, rows, active, memory);
        out.push(SidebarSection {
            id: section.id.clone(),
            kind: section.kind.clone(),
            title: section.title.clone(),
            collapsed: section.collapsed,
            channel_ids,
        });
    }
    out
}

/// Whether a conversation belongs in the sidebar at all. `placed` is a
/// conversation the user filed into a section (or starred).
fn is_candidate(ws: &Workspace, c: &Channel, active: Option<&str>, placed: bool) -> bool {
    if active == Some(c.id.as_str()) {
        return true;
    }
    if c.is_archived
        || c.extra
            .get("is_member")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
    {
        return false;
    }
    // Known only by id (`client.counts` named it, nothing described it): Slack
    // skips these as unknown until their metadata arrives.
    if !c.is_im && !c.is_mpim && c.name.as_deref().is_none_or(str::is_empty) {
        return false;
    }
    if c.is_im
        && dm_user_id(c)
            .and_then(|user| ws.users.get(user))
            .is_some_and(|user| user.deleted)
    {
        return false;
    }
    let open = c.extra.get("is_open").and_then(serde_json::Value::as_bool);
    // Record channels (Code channels, Salesforce records) show only while
    // open or filed somewhere, read or not.
    if record_id(c).is_some() {
        return open == Some(true) || placed;
    }
    // A closed DM comes back only while it has something to read.
    let closed = open == Some(false);
    !((c.is_im || c.is_mpim) && closed && !is_unread(ws, c) && !in_huddle(ws, c))
}

/// `isChannelEligibleForPrioritySection`: a VIP mention anywhere, or a badge
/// on a DM with a VIP or a group DM with one in it. A channel a VIP merely
/// posted in does not qualify.
pub fn is_priority_eligible(ws: &Workspace, c: &Channel) -> bool {
    if ws.messages.get(&c.id).is_some_and(|cm| cm.vip_count > 0) {
        return true;
    }
    if !has_badge(ws, c) {
        return false;
    }
    if c.is_im {
        return dm_user_id(c).is_some_and(|user| ws.vip_users.contains(user));
    }
    if c.is_mpim {
        return mpim_members(c).any(|user| ws.vip_users.contains(user));
    }
    false
}

/// A channel in several sections goes to the strongest of them: Starred,
/// then a custom section, then a user-group section. An unranked kind ends
/// the search, as in the client.
fn custom_section(sections: &[ResolvedSection], members: Option<&Vec<usize>>) -> Option<usize> {
    let members = members?;
    if let [only] = members.as_slice() {
        return Some(*only);
    }
    let rank = |kind: &str| match kind {
        "stars" => Some(3),
        "standard" => Some(2),
        "user_group" => Some(1),
        _ => None,
    };
    let mut best: Option<usize> = None;
    for &i in members {
        let Some(r) = rank(&sections[i].kind) else {
            break;
        };
        if best.is_none_or(|b| rank(&sections[b].kind) < Some(r)) {
            best = Some(i);
        }
    }
    best
}

/// `getChannelSectionTypeForChannel`, for a conversation in no section of its own.
fn default_section(
    ws: &Workspace,
    c: &Channel,
    index_of: &impl Fn(&str) -> Option<usize>,
) -> Option<usize> {
    if is_code_channel(c) {
        return index_of("coding_channels");
    }
    if c.is_ext_shared
        && ws.sidebar.show_connect_section
        && let Some(i) = index_of("slack_connect")
    {
        return Some(i);
    }
    if is_app_dm(ws, c)
        && let Some(i) = index_of("recent_apps")
    {
        return Some(i);
    }
    if c.is_im || c.is_mpim {
        return index_of("direct_messages");
    }
    index_of("channels")
}

fn is_app_dm(ws: &Workspace, c: &Channel) -> bool {
    if !c.is_im {
        return false;
    }
    let Some(user) = dm_user_id(c) else {
        return false;
    };
    user == SLACKBOT
        || user == SYSTEM_NOTIFICATIONS
        || ws.users.get(user).is_some_and(|user| {
            user.is_bot
                || user
                    .extra
                    .get("is_app_user")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
}

enum Row {
    Shown,
    /// Hidden because it is read. Counts toward a section having content.
    HiddenRead,
    Hidden,
}

/// The per-row filter. Assumes the client's `improve_mute_pref` behaviour,
/// which is what Slack now ships.
fn row_visibility(
    ws: &Workspace,
    section: &ResolvedSection,
    c: &Channel,
    active: Option<&str>,
) -> Row {
    if active == Some(c.id.as_str()) {
        return Row::Shown;
    }
    let collapsed = section.collapsed;
    let badge = has_badge(ws, c);
    if badge && !collapsed {
        return Row::Shown;
    }
    if section.filter == SectionFilter::Active && is_dormant(c) {
        return Row::Hidden;
    }
    if !ws.sidebar.show_muted && ws.sidebar.is_muted(&c.id) && !collapsed {
        return Row::Hidden;
    }
    if (section.filter == SectionFilter::All || in_huddle(ws, c)) && !collapsed {
        return Row::Shown;
    }
    let read = !is_unread(ws, c);
    let hides_read = matches!(
        section.filter,
        SectionFilter::HideRead | SectionFilter::MentionsOnly
    );
    if section.filter == SectionFilter::MentionsOnly && !badge {
        return if read { Row::HiddenRead } else { Row::Hidden };
    }
    if (hides_read || collapsed) && read {
        return Row::HiddenRead;
    }
    Row::Shown
}

fn sort_section(
    ws: &Workspace,
    section: &ResolvedSection,
    channels: Vec<&Channel>,
    active: Option<&str>,
    memory: &mut SidebarMemory,
) -> Vec<ChannelId> {
    struct Key<'a> {
        channel: &'a Channel,
        muted: bool,
        group: u8,
        name: String,
        recency: u64,
        priority: f64,
    }
    let mut keys: Vec<Key> = channels
        .into_iter()
        .map(|c| {
            let mut group = sort_group(ws, section, c);
            if active == Some(c.id.as_str()) {
                match &memory.selected_group {
                    Some((id, kept)) if id == &c.id => group = *kept,
                    _ => memory.selected_group = Some((c.id.clone(), group)),
                }
            }
            Key {
                channel: c,
                muted: ws.sidebar.is_muted(&c.id),
                group,
                name: channel_display_name(ws, c),
                recency: ws.channel_recency(c),
                priority: ws.priority_score(&c.id).unwrap_or(-1.0),
            }
        })
        .collect();
    let alpha = |a: &Key, b: &Key| {
        collator()
            .compare(&a.name, &b.name)
            .then_with(|| a.channel.id.cmp(&b.channel.id))
    };
    keys.sort_by(|a, b| {
        a.muted
            .cmp(&b.muted)
            .then(a.group.cmp(&b.group))
            .then_with(|| match section.sort {
                SectionSort::Recent => b.recency.cmp(&a.recency).then_with(|| alpha(a, b)),
                SectionSort::Priority => {
                    b.priority.total_cmp(&a.priority).then_with(|| alpha(a, b))
                }
                SectionSort::Alpha => alpha(a, b),
            })
    });
    keys.into_iter().map(|k| k.channel.id.clone()).collect()
}

/// The client's per-row sort group: mention-bumped rows first, Slackbot and
/// your own DM pinned, DMs kept apart from channels when the section asks.
fn sort_group(ws: &Workspace, section: &ResolvedSection, c: &Channel) -> u8 {
    let cfg = &ws.sidebar;
    let bump = cfg.boost_mentions && has_badge(ws, c);
    let dm_section = is_dm_section(&section.kind);
    let private_channel = (c.is_private || c.is_group) && !c.is_im && !c.is_mpim;
    if private_channel && cfg.separate_private_channels && section.kind != "coding_channels" {
        return if bump {
            group::PRIVATE_MENTIONS
        } else {
            group::PRIVATE
        };
    }
    let user = c.is_im.then(|| dm_user_id(c)).flatten();
    if user == Some(SLACKBOT) {
        return match (section.id == PRIORITY_SECTION_ID, bump) {
            (true, true) => group::MENTIONS,
            (false, true) => group::SLACKBOT_MENTION,
            (_, false) => group::SLACKBOT,
        };
    }
    if user == Some(ws.self_user_id.as_str()) {
        return if dm_section {
            group::YOU_NEW_TEAM
        } else if bump {
            group::YOU_MENTION
        } else {
            group::YOU
        };
    }
    if (c.is_im || c.is_mpim) && (cfg.undo_channel_intermix || dm_section) {
        return if bump {
            group::IMS_MENTIONS
        } else {
            group::IMS
        };
    }
    if bump {
        group::MENTIONS
    } else {
        group::EVERYTHING_ELSE
    }
}

/// The sidebar badge: mentions for a channel, any unread for a DM.
fn has_badge(ws: &Workspace, c: &Channel) -> bool {
    if c.is_im || c.is_mpim {
        return is_unread(ws, c);
    }
    ws.messages
        .get(&c.id)
        .map(|cm| cm.mention_count)
        .or(c.mention_count)
        .unwrap_or(0)
        > 0
}

fn is_unread(ws: &Workspace, c: &Channel) -> bool {
    ws.unread_total(c) > 0
}

fn in_huddle(ws: &Workspace, c: &Channel) -> bool {
    ws.active_huddles.contains_key(&c.id)
}

fn is_dormant(c: &Channel) -> bool {
    c.extra
        .get("properties")
        .and_then(|p| p.get("is_dormant"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn record_id(c: &Channel) -> Option<&str> {
    c.extra
        .get("properties")?
        .get("record_channel")?
        .get("record_id")?
        .as_str()
        .filter(|id| !id.is_empty())
}

/// `isCodingChannel`: an agent record channel.
fn is_code_channel(c: &Channel) -> bool {
    let kind = c
        .extra
        .get("properties")
        .and_then(|p| p.get("record_channel"))
        .and_then(|r| r.get("record_type"))
        .and_then(serde_json::Value::as_str);
    record_id(c).is_some_and(|id| kind == Some("agent_channel") || id.starts_with("AC:"))
}

fn mpim_members(c: &Channel) -> impl Iterator<Item = &str> {
    c.extra
        .get("members")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
}

/// `Intl.Collator("en-US", { numeric: true })`, which is what Slack sorts
/// names with: case- and accent-folding, digit runs compared as numbers.
fn collator() -> &'static CollatorBorrowed<'static> {
    static COLLATOR: OnceLock<CollatorBorrowed<'static>> = OnceLock::new();
    COLLATOR.get_or_init(|| {
        let mut prefs = CollatorPreferences::default();
        prefs.numeric_ordering = Some(CollationNumericOrdering::True);
        Collator::try_new(prefs, CollatorOptions::default()).expect("baked root collation data")
    })
}

/// Compare two names the way Slack's sidebar does.
pub fn sidebar_name_cmp(a: &str, b: &str) -> Ordering {
    collator().compare(a, b)
}
