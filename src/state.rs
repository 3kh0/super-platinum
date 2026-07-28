use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::slack::models::{
    Channel, ChannelId, Emoji, File, Message as SlackMessage, MessageTs, Reaction, Room, TeamId,
    User, UserId,
};
use crate::slack::realtime::Connection;

pub const RECENT_CHANNELS_MAX: usize = 20;

pub const FRECENCY_HALF_LIFE_SECS: f64 = 7.0 * 24.0 * 3600.0;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrecencyEntry {
    pub score: f64,
    pub last_visit: i64,
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn decayed(score: f64, last_visit: i64, now: i64) -> f64 {
    let elapsed = (now - last_visit).max(0) as f64;
    score * 0.5_f64.powf(elapsed / FRECENCY_HALF_LIFE_SECS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Loading,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainView {
    #[default]
    Home,
    Unreads,
    Dms,
    Activity,
}

#[derive(Debug, Clone, Default)]
pub enum RealtimeStatus {
    #[default]
    Disconnected,
    Connected(Connection),
}

impl RealtimeStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, RealtimeStatus::Connected(_))
    }
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Presence {
    Active,
    Away,
    #[default]
    Unknown,
}

impl Presence {
    pub fn from_slack(value: &str) -> Self {
        match value {
            "active" => Presence::Active,
            "away" => Presence::Away,
            _ => Presence::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChannelMessages {
    pub messages: Vec<SlackMessage>,
    pub loaded: bool,
    pub history_refreshing: bool,
    pub has_more_older: bool,
    pub history_loading_older: bool,
    pub history_failed: bool,
    pub pending: Vec<MessageTs>,
    pub last_read: Option<MessageTs>,
    pub unread_count: u32,
    pub mention_count: u32,
}

impl ChannelMessages {
    pub fn upsert(&mut self, msg: SlackMessage) -> bool {
        let Some(ts) = msg.ts.clone() else {
            self.messages.push(msg);
            return true;
        };
        match self.index_of(&ts) {
            Some(i) => {
                self.messages[i] = msg;
                false
            }
            None => {
                let pos = self
                    .messages
                    .binary_search_by(|m| cmp_ts(m.ts.as_deref(), Some(&ts)))
                    .unwrap_or_else(|e| e);
                self.messages.insert(pos, msg);
                true
            }
        }
    }

    pub fn remove(&mut self, ts: &str) -> bool {
        match self.index_of(ts) {
            Some(i) => {
                self.messages.remove(i);
                self.pending.retain(|p| p != ts);
                true
            }
            None => false,
        }
    }

    pub fn confirm(&mut self, client_msg_id: &str, confirmed: SlackMessage) -> bool {
        let temp_ts = self.messages.iter().find_map(|m| {
            match (m.client_msg_id.as_deref(), m.ts.as_deref()) {
                (Some(cid), Some(ts))
                    if cid == client_msg_id && self.pending.iter().any(|p| p == ts) =>
                {
                    Some(ts.to_owned())
                }
                _ => None,
            }
        });
        if let Some(ts) = temp_ts {
            self.remove(&ts);
            self.upsert(confirmed);
            true
        } else {
            false
        }
    }

    pub fn confirm_matching_pending(
        &mut self,
        user: Option<&str>,
        text: Option<&str>,
        confirmed: SlackMessage,
    ) -> bool {
        let matches = self
            .messages
            .iter()
            .filter_map(|m| {
                let ts = m.ts.as_deref()?;
                if !self.pending.iter().any(|p| p == ts) {
                    return None;
                }
                (m.user.as_deref() == user && m.text.as_deref() == text).then(|| ts.to_owned())
            })
            .take(2)
            .collect::<Vec<_>>();
        if let [ts] = matches.as_slice() {
            self.remove(&ts);
            self.upsert(confirmed);
            true
        } else {
            false
        }
    }

    pub fn merge_update(&mut self, update: SlackMessage) -> bool {
        let Some(ts) = update.ts.clone() else {
            self.messages.push(update);
            return true;
        };
        match self.index_of(&ts) {
            Some(i) => {
                merge_message(&mut self.messages[i], update);
                false
            }
            None => self.upsert(update),
        }
    }

    pub fn is_pending(&self, ts: &str) -> bool {
        self.pending.iter().any(|p| p == ts)
    }

    pub fn latest_ts(&self) -> Option<MessageTs> {
        self.messages
            .iter()
            .filter_map(|m| m.ts.clone())
            .max_by(|a, b| ts_key(a).cmp(&ts_key(b)))
    }

    pub fn latest_confirmed_ts(&self) -> Option<MessageTs> {
        self.messages
            .iter()
            .filter_map(|message| {
                let ts = message.ts.as_ref()?;
                (!self.is_pending(ts)).then(|| ts.clone())
            })
            .max_by(|a, b| ts_key(a).cmp(&ts_key(b)))
    }

    pub fn oldest_ts(&self) -> Option<MessageTs> {
        self.messages
            .iter()
            .filter_map(|m| m.ts.clone())
            .min_by(|a, b| ts_key(a).cmp(&ts_key(b)))
    }

    pub fn apply_reaction(&mut self, ts: &str, user: &str, name: &str, added: bool) -> bool {
        let Some(i) = self.index_of(ts) else {
            return false;
        };
        apply_message_reaction(&mut self.messages[i], user, name, added)
    }

    fn index_of(&self, ts: &str) -> Option<usize> {
        self.messages
            .iter()
            .position(|m| m.ts.as_deref() == Some(ts))
    }
}

#[derive(Debug, Clone, Default)]
pub struct SidebarConfig {
    pub sections: Vec<crate::slack::models::ChannelSection>,
    pub section_prefs: HashMap<String, SectionPref>,
    pub hidden_sections: HashSet<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SectionPref {
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub sidebar: Option<String>,
    #[serde(default)]
    pub c: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionSort {
    Alpha,
    Recent,
    Priority,
}

#[derive(Debug, Clone)]
pub struct ResolvedSection {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub sort: SectionSort,
    pub show_all: bool,
    pub channel_ids: Vec<ChannelId>,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub team_id: TeamId,
    pub name: String,
    pub url: String,
    pub self_user_id: UserId,
    pub activity_unread_count: Option<u32>,
    pub channels: BTreeMap<ChannelId, Channel>,
    pub starred_order: Vec<ChannelId>,
    pub dm_order: Vec<ChannelId>,
    pub recent_channels: Vec<ChannelId>,
    pub last_active_channel: Option<ChannelId>,
    pub priority_scores: BTreeMap<ChannelId, f64>,
    pub frecency: BTreeMap<ChannelId, FrecencyEntry>,
    pub hide_read_channels_unless_starred: bool,
    pub priority_sidebar_section: bool,
    pub vip_users: HashSet<UserId>,
    pub sidebar: SidebarConfig,
    pub users: HashMap<UserId, User>,
    pub custom_emoji: HashMap<String, Emoji>,
    pub messages: HashMap<ChannelId, ChannelMessages>,
    pub typing: HashMap<ChannelId, Vec<(UserId, Instant)>>,
    pub presence: HashMap<UserId, Presence>,
    /// Active huddles keyed by channel id. A channel has at most one live huddle.
    pub active_huddles: HashMap<ChannelId, Room>,
    pub rt: RealtimeStatus,
    pub rt_generation: u64,
}

impl Workspace {
    pub fn from_session(s: &crate::config::WorkspaceSession) -> Self {
        Workspace {
            team_id: s.team_id.clone(),
            name: s.name.clone(),
            url: s.url.clone(),
            self_user_id: s.user_id.clone(),
            activity_unread_count: None,
            channels: BTreeMap::new(),
            starred_order: Vec::new(),
            dm_order: Vec::new(),
            recent_channels: Vec::new(),
            last_active_channel: None,
            priority_scores: BTreeMap::new(),
            frecency: BTreeMap::new(),
            hide_read_channels_unless_starred: false,
            priority_sidebar_section: false,
            vip_users: HashSet::new(),
            sidebar: SidebarConfig::default(),
            users: HashMap::new(),
            custom_emoji: HashMap::new(),
            messages: HashMap::new(),
            typing: HashMap::new(),
            presence: HashMap::new(),
            active_huddles: HashMap::new(),
            rt: RealtimeStatus::default(),
            rt_generation: 0,
        }
    }

    pub fn apply_boot(&mut self, boot: crate::slack::models::BootData) {
        let self_user = if boot.self_user.id.is_empty() {
            None
        } else {
            Some(User {
                id: boot.self_user.id.clone(),
                name: boot.self_user.name.clone(),
                real_name: boot
                    .self_user
                    .profile
                    .as_ref()
                    .and_then(|profile| profile.real_name.clone()),
                profile: boot.self_user.profile.clone(),
                extra: boot.self_user.extra.clone(),
                ..Default::default()
            })
        };
        if !boot.self_user.id.is_empty() {
            self.self_user_id = boot.self_user.id.clone();
        }
        if let Some(team) = &boot.team {
            if let Some(name) = &team.name {
                self.name = name.clone();
            }
            if let Some(url) = &team.url {
                self.url = url.clone();
            }
        }
        self.starred_order = boot.starred.clone();
        self.priority_scores = boot.channels_priority.clone();
        self.hide_read_channels_unless_starred =
            boot.prefs.sidebar_behavior.as_deref() == Some("hide_read_channels_unless_starred");
        self.priority_sidebar_section = boot.prefs.priority_sidebar_section;
        self.vip_users = boot
            .prefs
            .vip_users
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();
        self.sidebar.section_prefs = boot
            .prefs
            .channel_sections
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();
        self.sidebar.hidden_sections = boot
            .prefs
            .hidden_user_group_sections
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();

        for channel in boot.all_channels() {
            if channel.is_im || channel.is_mpim {
                append_unique(&mut self.dm_order, channel.id.clone());
            }
            self.apply_channel_read_state(&channel);
            self.channels.insert(channel.id.clone(), channel);
        }
        for id in &self.starred_order {
            if let Some(channel) = self.channels.get_mut(id) {
                channel.is_starred = true;
            }
        }
        for user in boot.users {
            self.users.insert(user.id.clone(), user);
        }
        if let Some(user) = self_user {
            self.users.insert(user.id.clone(), user);
        }
    }

    pub fn apply_counts(&mut self, counts: crate::slack::models::CountsPage) {
        if let Some(unread_count) = counts.activity_unread_count() {
            self.activity_unread_count = Some(unread_count);
        }
        for channel in counts.all_channels() {
            self.apply_channel_read_state(&channel);
            if let Some(existing) = self.channels.get_mut(&channel.id) {
                if channel.is_starred {
                    existing.is_starred = true;
                }
                existing.unread_count = channel.unread_count.or(existing.unread_count);
                existing.unread_count_display = channel
                    .unread_count_display
                    .or(existing.unread_count_display);
                existing.mention_count = channel.mention_count.or(existing.mention_count);
                existing.has_unreads |= channel.has_unreads;
                existing.last_read = channel.last_read.or_else(|| existing.last_read.take());
                existing.updated = channel.updated.max(existing.updated);
            } else {
                self.channels.insert(channel.id.clone(), channel);
            }
        }
    }

    pub fn apply_channel_sections(&mut self, page: crate::slack::models::ChannelSectionsPage) {
        self.sidebar.sections = page.channel_sections;
    }

    pub fn resolved_sidebar_sections(&self) -> Vec<ResolvedSection> {
        let mut out = Vec::new();
        if self.priority_sidebar_section {
            out.push(self.resolve_section("priority", "priority", "VIP unreads", &[]));
        }
        if self.sidebar.sections.is_empty() {
            for (kind, title) in [
                ("slack_connect", "External connections"),
                ("direct_messages", "Direct messages"),
                ("stars", "Starred"),
                ("channels", "Channels"),
            ] {
                out.push(self.resolve_section(kind, kind, title, &[]));
            }
            return out;
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
            let title: &str = match section.kind.as_str() {
                "slack_connect" => "External connections",
                "direct_messages" => "Direct messages",
                "stars" => "Starred",
                "channels" => "Channels",
                _ if section.channel_ids_page.channel_ids.is_empty() => continue,
                _ => &section.name,
            };
            out.push(self.resolve_section(
                &section.channel_section_id,
                &section.kind,
                title,
                &section.channel_ids_page.channel_ids,
            ));
        }
        for (kind, title) in [
            ("direct_messages", "Direct messages"),
            ("channels", "Channels"),
        ] {
            if !out.iter().any(|s| s.kind == kind) {
                out.push(self.resolve_section(kind, kind, title, &[]));
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
        let pref = self.sidebar.section_prefs.get(id);
        let sort = match pref.and_then(|p| p.sort.as_deref()) {
            Some("recent") => SectionSort::Recent,
            Some("priority") => SectionSort::Priority,
            Some(_) => SectionSort::Alpha,
            None if kind == "direct_messages" || kind == "priority" => SectionSort::Recent,
            None => SectionSort::Alpha,
        };
        let show_all = pref.map_or(kind == "stars" || kind == "slack_connect", |p| {
            p.sidebar.as_deref() == Some("all") || kind == "stars"
        });
        ResolvedSection {
            id: id.to_owned(),
            kind: kind.to_owned(),
            title: title.to_owned(),
            sort,
            show_all,
            channel_ids: channel_ids.to_vec(),
        }
    }

    pub fn apply_sidebar_dms(&mut self, dms: crate::slack::models::SidebarDmsPage) {
        for channel in dms.all_channels() {
            append_unique(&mut self.dm_order, channel.id.clone());
            self.apply_channel_read_state(&channel);
            if let Some(existing) = self.channels.get_mut(&channel.id) {
                merge_channel_metadata(existing, channel);
            } else {
                self.channels.insert(channel.id.clone(), channel);
            }
        }
    }

    pub fn apply_channels_info(&mut self, channels: Vec<Channel>) {
        for channel in channels {
            self.apply_channel_read_state(&channel);
            if let Some(existing) = self.channels.get_mut(&channel.id) {
                merge_channel_metadata(existing, channel);
            } else {
                self.channels.insert(channel.id.clone(), channel);
            }
        }
        for id in &self.starred_order {
            if let Some(channel) = self.channels.get_mut(id) {
                channel.is_starred = true;
            }
        }
    }

    pub fn display_name(&self, user_id: &str) -> String {
        display_name(self.users.get(user_id), user_id)
    }

    pub fn avatar_url(&self, user_id: &str) -> Option<String> {
        user_avatar_url(self.users.get(user_id)?).map(str::to_owned)
    }

    pub fn message_author_name(&self, msg: &SlackMessage) -> String {
        message_author_name(self, msg)
    }

    pub fn message_avatar(&self, msg: &SlackMessage) -> (Option<String>, Option<String>) {
        message_avatar(self, msg)
    }

    pub fn custom_emoji_url(&self, name: &str) -> Option<&str> {
        custom_emoji_url(&self.custom_emoji, name)
    }

    pub fn apply_emojis(&mut self, emojis: Vec<Emoji>) {
        for emoji in emojis {
            self.custom_emoji.insert(emoji.name.clone(), emoji);
        }
    }

    pub fn set_typing(&mut self, channel: &str, user: UserId, now: Instant) {
        if user == self.self_user_id {
            return;
        }
        let entry = self.typing.entry(channel.to_owned()).or_default();
        entry.retain(|(u, _)| u != &user);
        entry.push((user, now));
    }

    pub fn clear_typing_user(&mut self, channel: &str, user: &str) {
        if let Some(entry) = self.typing.get_mut(channel) {
            entry.retain(|(u, _)| u != user);
        }
        self.typing.retain(|_, v| !v.is_empty());
    }

    pub fn prune_typing(&mut self, now: Instant, ttl: Duration) -> bool {
        let mut changed = false;
        for entry in self.typing.values_mut() {
            let before = entry.len();
            entry.retain(|(_, seen)| now.duration_since(*seen) < ttl);
            changed |= entry.len() != before;
        }
        self.typing.retain(|_, v| !v.is_empty());
        changed
    }

    pub fn typing_names(&self, channel: &str) -> Vec<String> {
        self.typing
            .get(channel)
            .into_iter()
            .flatten()
            .filter(|(u, _)| u != &self.self_user_id)
            .map(|(u, _)| self.display_name(u))
            .collect()
    }

    pub fn set_presence(&mut self, user: UserId, presence: Presence) {
        if user.is_empty() {
            return;
        }
        self.presence.insert(user, presence);
    }

    /// Apply a huddle room update from a realtime `sh_room_*` event. Ended or
    /// empty huddles are removed; active ones are upserted under their channel.
    /// Returns true if the active-huddle set changed (so callers can repaint).
    pub fn apply_room(&mut self, room: Room) -> bool {
        let Some(channel) = room.channel().cloned() else {
            return false;
        };
        if !room.is_active() || room.participants.is_empty() {
            // Only clear the channel if this ending frame is for the room we
            // are actually tracking — a stale leave must not drop a newer one.
            let tracked = self
                .active_huddles
                .get(&channel)
                .is_some_and(|existing| existing.id == room.id);
            return tracked && self.active_huddles.remove(&channel).is_some();
        }
        self.active_huddles.insert(channel, room);
        true
    }

    pub fn active_huddle(&self, channel: &str) -> Option<&Room> {
        self.active_huddles.get(channel)
    }

    pub fn presence_for_channel(&self, channel: &Channel) -> Presence {
        if !(channel.is_im || channel.is_mpim) {
            return Presence::Unknown;
        }
        dm_user_id(channel)
            .and_then(|user| self.presence.get(user).copied())
            .unwrap_or(Presence::Unknown)
    }

    pub fn touch_recent(&mut self, id: &ChannelId) {
        self.recent_channels.retain(|existing| existing != id);
        self.recent_channels.insert(0, id.clone());
        self.recent_channels.truncate(RECENT_CHANNELS_MAX);
    }

    pub fn record_visit(&mut self, id: &ChannelId, now: i64) {
        let entry = self.frecency.entry(id.clone()).or_insert(FrecencyEntry {
            score: 0.0,
            last_visit: now,
        });
        entry.score = decayed(entry.score, entry.last_visit, now) + 1.0;
        entry.last_visit = now;
    }

    pub fn frecency_score(&self, id: &str, now: i64) -> f64 {
        self.frecency
            .get(id)
            .map(|e| decayed(e.score, e.last_visit, now))
            .unwrap_or(0.0)
    }

    pub fn max_frecency_score(&self, now: i64) -> f64 {
        self.frecency
            .values()
            .map(|e| decayed(e.score, e.last_visit, now))
            .fold(0.0, f64::max)
    }

    pub fn is_starred_channel(&self, channel: &Channel) -> bool {
        channel.is_starred
            || self.starred_order.iter().any(|id| id == &channel.id)
            || channel
                .extra
                .get("is_starred")
                .or_else(|| channel.extra.get("starred"))
                .or_else(|| channel.extra.get("is_favorite"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
    }

    pub fn priority_score(&self, channel_id: &str) -> Option<f64> {
        self.priority_scores.get(channel_id).copied()
    }

    pub fn channel_recency(&self, channel: &Channel) -> u64 {
        let seen = self
            .messages
            .get(&channel.id)
            .and_then(ChannelMessages::latest_ts)
            .map(|ts| ts_key(&ts).0)
            .unwrap_or(0);
        seen.max(channel.updated.unwrap_or(0))
    }

    pub fn unread_total(&self, channel: &Channel) -> u32 {
        self.messages
            .get(&channel.id)
            .map(|cm| cm.mention_count.max(cm.unread_count))
            .unwrap_or_else(|| {
                let count = channel
                    .mention_count
                    .or(channel.unread_count)
                    .or(channel.unread_count_display)
                    .unwrap_or(0);
                if count == 0 && channel.has_unreads {
                    1
                } else {
                    count
                }
            })
    }

    pub fn should_show_unstarred_read_channels(&self) -> bool {
        !self.hide_read_channels_unless_starred
    }

    fn apply_channel_read_state(&mut self, channel: &Channel) {
        let cm = self.messages.entry(channel.id.clone()).or_default();
        if let Some(last_read) = &channel.last_read {
            cm.last_read = Some(last_read.clone());
        }
        if let Some(unread) = channel.unread_count.or(channel.unread_count_display) {
            cm.unread_count = unread;
        }
        if let Some(mentions) = channel.mention_count {
            cm.mention_count = mentions;
        }
        if channel.has_unreads && cm.unread_count == 0 && cm.mention_count == 0 {
            cm.unread_count = 1;
        }
    }
}

mod helpers;
pub use helpers::*;
use helpers::{
    append_unique, apply_message_reaction, custom_emoji_url, linked_list_order,
    merge_channel_metadata, merge_message, ordinal_day,
};

#[cfg(test)]
mod tests;
