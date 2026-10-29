use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::slack::models::{
    Channel, ChannelId, Emoji, Message as SlackMessage, MessageTs, Room, TeamId, User, UserId,
};
use crate::slack::realtime::Connection;

pub const RECENT_CHANNELS_MAX: usize = 20;
/// Conversations shown in the empty quick-switcher recents list.
pub const PALETTE_RECENTS_SHOWN: usize = 5;

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
    Threads,
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
    /// Realtime deletion evidence for this in-memory account/workspace/channel.
    /// Upload verification must not reinsert a row from an older readback page.
    /// Ordinary optimistic removal does not populate this set.
    pub remote_deletions: HashSet<MessageTs>,
    pub loaded: bool,
    pub history_refreshing: bool,
    pub has_more_older: bool,
    pub history_loading_older: bool,
    pub history_failed: bool,
    pub pending: Vec<MessageTs>,
    pub last_read: Option<MessageTs>,
    /// Local ordering for conversation/thread acknowledgements, including cursor ABA.
    pub read_revision: u64,
    pub unread_count: u32,
    pub mention_count: u32,
    /// `client.counts` `vip_count`: unread mentions from a VIP. Any at all puts
    /// the conversation in VIP unreads.
    pub vip_count: u32,
    /// `client.counts` `latest`: the newest message's ts, which the "recent"
    /// sidebar sort orders by.
    pub latest: Option<MessageTs>,
}

impl ChannelMessages {
    pub fn record_remote_deletion(&mut self, ts: &str) {
        self.remote_deletions.insert(ts.to_owned());
        self.remove(ts);
    }

    pub fn was_deleted(&self, ts: &str) -> bool {
        self.remote_deletions.contains(ts)
    }

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
            self.remove(ts);
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
            .filter_map(|m| m.ts.as_ref())
            .max_by_key(|ts| ts_key(ts))
            .cloned()
    }

    pub fn latest_confirmed_ts(&self) -> Option<MessageTs> {
        self.messages
            .iter()
            .filter_map(|message| {
                let ts = message.ts.as_ref()?;
                (!self.is_pending(ts)).then_some(ts)
            })
            .max_by_key(|ts| ts_key(ts))
            .cloned()
    }

    pub fn oldest_ts(&self) -> Option<MessageTs> {
        self.messages
            .iter()
            .filter_map(|m| m.ts.as_ref())
            .min_by_key(|ts| ts_key(ts))
            .cloned()
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

#[derive(Debug, Clone)]
pub struct Workspace {
    /// Confirmed deletions also mask stale history/search responses and warm cache rows.
    pub deleted_files: HashSet<String>,
    pub channel_lifecycle: ChannelLifecycle,
    pub team_id: TeamId,
    pub name: String,
    pub url: String,
    pub self_user_id: UserId,
    /// Slack's locale preference, independent of the host operating system.
    pub locale: Option<String>,
    /// Exact boot team plan; None means unknown, not a guessed paid entitlement.
    pub team_plan: Option<String>,
    /// Fresh boot posting facts; never restored from the warm cache.
    pub posting_metadata: posting_metadata::Metadata,
    /// Raw source history and canonical publication witnesses, separate from typed UI maps.
    pub directory_channels: directory_channels::Store,
    pub directory_members: directory_members::Store,
    pub directory_usergroups: directory_usergroups::Store,
    pub cfm_payload: cfm_payload::Stores,
    /// Read-only unified-draft identities, separate from recoverable local composers.
    pub draft_presence: crate::drafts::presence::Presence,
    pub activity_unread_count: Option<u32>,
    pub activity_counts: ActivityCountsState,
    pub dock_counts: DockCountsState,
    pub thread_counts: ThreadCountsState,
    pub channels: BTreeMap<ChannelId, Channel>,
    pub starred_order: Vec<ChannelId>,
    pub dm_order: Vec<ChannelId>,
    pub recent_channels: Vec<ChannelId>,
    pub recent_searches: crate::search::SearchHistory,
    pub last_active_channel: Option<ChannelId>,
    pub priority_scores: BTreeMap<ChannelId, f64>,
    pub frecency: BTreeMap<ChannelId, FrecencyEntry>,
    pub priority_sidebar_section: bool,
    pub vip_users: HashSet<UserId>,
    pub vip_revisions: HashMap<UserId, u64>,
    pub sidebar: SidebarConfig,
    pub notifications: NotificationConfig,
    pub read_preferences: ReadPreferences,
    pub clear_unreads_preference: ClearUnreadsPreference,
    pub users: HashMap<UserId, User>,
    pub bots: bots::BotLookupState,
    pub usergroups: HashMap<String, crate::slack::models::UserGroup>,
    pub custom_emoji: HashMap<String, Emoji>,
    pub custom_emoji_revision: u64,
    pub emoji_preferences: crate::composer_emoji::preferences::Preferences,
    pub messages: HashMap<ChannelId, ChannelMessages>,
    pub typing: HashMap<ChannelId, Vec<(UserId, Instant)>>,
    pub presence: HashMap<UserId, Presence>,
    /// `dnd.info` for the signed-in user: drives the rail's snooze badge.
    pub self_dnd: crate::slack::models::DndInfo,
    /// Active huddles keyed by channel id. A channel has at most one live huddle.
    pub active_huddles: HashMap<ChannelId, Room>,
    pub rt: RealtimeStatus,
    pub rt_generation: u64,
}

impl Workspace {
    pub fn from_session(s: &crate::config::WorkspaceSession) -> Self {
        Workspace {
            deleted_files: Default::default(),
            team_id: s.team_id.clone(),
            name: s.name.clone(),
            url: s.url.clone(),
            self_user_id: s.user_id.clone(),
            locale: None,
            team_plan: None,
            posting_metadata: Default::default(),
            directory_channels: Default::default(),
            directory_members: Default::default(),
            directory_usergroups: directory_usergroups::Store::fresh(),
            cfm_payload: Default::default(),
            draft_presence: Default::default(),
            activity_unread_count: None,
            activity_counts: Default::default(),
            dock_counts: Default::default(),
            thread_counts: Default::default(),
            channels: BTreeMap::new(),
            starred_order: Vec::new(),
            dm_order: Vec::new(),
            recent_channels: Vec::new(),
            recent_searches: Default::default(),
            last_active_channel: None,
            priority_scores: BTreeMap::new(),
            frecency: BTreeMap::new(),
            priority_sidebar_section: false,
            vip_users: HashSet::new(),
            vip_revisions: Default::default(),
            sidebar: SidebarConfig::default(),
            notifications: NotificationConfig::default(),
            read_preferences: ReadPreferences::default(),
            clear_unreads_preference: ClearUnreadsPreference::default(),
            users: HashMap::new(),
            bots: Default::default(),
            usergroups: HashMap::new(),
            custom_emoji: HashMap::new(),
            custom_emoji_revision: 0,
            emoji_preferences: Default::default(),
            messages: HashMap::new(),
            typing: HashMap::new(),
            presence: HashMap::new(),
            self_dnd: Default::default(),
            active_huddles: HashMap::new(),
            rt: RealtimeStatus::default(),
            rt_generation: 0,
            channel_lifecycle: Default::default(),
        }
    }

    pub fn apply_boot(&mut self, boot: crate::slack::models::BootData) {
        self.posting_metadata.replace_boot_preserving_external(
            posting_metadata::Metadata::from_boot(&self.team_id, &self.self_user_id, &boot),
        );
        self.team_plan = boot
            .team
            .as_ref()
            .and_then(|t| t.extra.get("plan"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let Some(groups) = boot
            .extra
            .get("subteams")
            .and_then(|v| v.get("self"))
            .and_then(|v| v.as_array())
        {
            for group in self.usergroups.values_mut() {
                group.users.retain(|user| user != &self.self_user_id);
            }
            for id in groups.iter().filter_map(|v| v.as_str()) {
                let group = self.usergroups.entry(id.into()).or_insert_with(|| {
                    crate::slack::models::UserGroup {
                        id: id.into(),
                        ..Default::default()
                    }
                });
                group.users.push(self.self_user_id.clone());
            }
        }
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
        self.priority_sidebar_section = boot.prefs.priority_sidebar_section;
        self.apply_vip_snapshot(
            boot.prefs
                .vip_users
                .as_deref()
                .unwrap_or_default()
                .split(',')
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect(),
        );
        self.sidebar.apply_prefs(&boot.prefs);
        self.notifications.apply_prefs(&boot.prefs);
        if boot.prefs.is_present() {
            self.read_preferences.apply_boot(&boot.prefs);
            self.emoji_preferences.replace(&boot.prefs.extra);
            self.clear_unreads_preference.apply_boot(&boot.prefs);
        }
        if let Some(locale) = boot.prefs.extra.get("locale") {
            self.locale = locale.as_str().map(str::to_owned);
        }
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
            let existing = self.users.remove(&user.id);
            self.users
                .insert(user.id.clone(), merge_boot_user(existing, user));
        }
        if let Some(user) = self_user {
            let existing = self.users.remove(&user.id);
            self.users
                .insert(user.id.clone(), merge_boot_user(existing, user));
        }
    }

    /// Merge a realtime user update without discarding fields omitted by the
    /// event. Status fields that are explicitly empty remain empty, allowing a
    /// `user_change` frame to clear the signed-in user's status immediately.
    pub fn apply_user_update(&mut self, user: User) {
        let existing = self.users.remove(&user.id);
        self.users
            .insert(user.id.clone(), merge_boot_user(existing, user));
    }

    pub fn apply_counts(&mut self, counts: crate::slack::models::CountsPage) {
        self.apply_counts_since(counts, self.channel_lifecycle.revision());
    }

    pub fn apply_counts_since(&mut self, counts: crate::slack::models::CountsPage, since: u64) {
        self.apply_counts_snapshot(counts, since, self.thread_counts.revision());
    }

    pub fn apply_counts_snapshot(
        &mut self,
        counts: crate::slack::models::CountsPage,
        since: u64,
        thread_revision: u64,
    ) {
        self.apply_counts_snapshot_with_activity(
            counts,
            since,
            thread_revision,
            self.activity_counts.revision(),
        );
    }

    pub fn apply_counts_snapshot_with_activity(
        &mut self,
        counts: crate::slack::models::CountsPage,
        since: u64,
        thread_revision: u64,
        activity_revision: u64,
    ) {
        self.apply_counts_snapshot_with_dock(
            counts,
            since,
            thread_revision,
            activity_revision,
            self.dock_counts.revision(),
        );
    }

    pub fn apply_counts_snapshot_with_dock(
        &mut self,
        mut counts: crate::slack::models::CountsPage,
        since: u64,
        thread_revision: u64,
        activity_revision: u64,
        dock_revision: u64,
    ) {
        // Saved/channel-badge freshness is independent of channel membership
        // and read state, which continue through their existing guards below.
        self.dock_counts
            .apply_snapshot(&counts.extra, dock_revision);
        self.dock_counts.counts_loaded = true;
        if let Some(threads) = counts.threads.take() {
            self.thread_counts.apply_snapshot(threads, thread_revision);
        }
        if let Some(activity) = counts.activity_v2.take() {
            let total = activity.total();
            if self
                .activity_counts
                .apply_snapshot(activity, activity_revision)
            {
                self.activity_unread_count = Some(total);
            }
        }
        self.apply_counts_membership(&counts, since);
        for mut channel in counts.all_channels() {
            // Canonical counts replace numeric mentions even when omitted.
            // An unread DM without a numeric count contributes the separate
            // forced-unread indicator, not its previous mention total.
            channel.mention_count.get_or_insert(0);
            self.apply_channel_counts_read_state(&channel);
            if let Some(existing) = self.channels.get_mut(&channel.id) {
                if channel.is_starred {
                    existing.is_starred = true;
                }
                // Counts carry the server version, not the metadata itself.
                // Keep the cached version until genericInfo fetches that body.
            } else {
                if !channel.is_im && !channel.is_mpim {
                    channel.extra.insert("is_member".into(), true.into());
                }
                self.channel_lifecycle.overlay(&mut channel, since);
                self.channels.insert(channel.id.clone(), channel);
            }
        }
    }

    /// Apply one authoritative `client.counts` row, including a backward read
    /// cursor after Mark Unread. Callers must guard account and read freshness.
    /// This does not infer membership from other rows' absence, insert channel
    /// metadata, or update lifecycle, star, thread, activity, or dock state.
    pub fn apply_channel_counts_read_state(&mut self, channel: &Channel) {
        // Missing numeric mentions are zero before forced-unread calculation.
        self.messages
            .entry(channel.id.clone())
            .or_default()
            .mention_count = channel.mention_count.unwrap_or(0);
        self.apply_channel_read_state(channel);
        // Canonical read rows omit counts; absent VIP counts also mean zero.
        if let Some(cm) = self.messages.get_mut(&channel.id) {
            if !channel.has_unreads && channel.unread_count.is_none() {
                cm.unread_count = 0;
            }
            if !channel.extra.contains_key("vip_count") {
                cm.vip_count = 0;
            }
        }
        if let Some(existing) = self.channels.get_mut(&channel.id) {
            let cleared = (!channel.has_unreads).then_some(0);
            existing.unread_count = channel.unread_count.or(cleared).or(existing.unread_count);
            existing.unread_count_display = channel
                .unread_count_display
                .or(cleared)
                .or(existing.unread_count_display);
            existing.mention_count = Some(channel.mention_count.unwrap_or(0));
            existing.has_unreads = channel.has_unreads;
            if let Some(last_read) = &channel.last_read {
                existing.last_read = Some(last_read.clone());
            }
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
        user_avatar_url(self.users.get(user_id)?)
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

    pub fn emoji_glyph(&self, name: &str) -> String {
        match custom_emoji_source(&self.custom_emoji, name) {
            Some(EmojiSource::Unicode(glyph)) => glyph.to_owned(),
            _ => emoji_glyph(name),
        }
    }

    pub fn replace_emojis(&mut self, emojis: Vec<Emoji>) {
        self.custom_emoji.clear();
        self.apply_emojis(emojis);
    }

    pub fn apply_emojis(&mut self, emojis: Vec<Emoji>) {
        self.custom_emoji_revision = self.custom_emoji_revision.wrapping_add(1);
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

    /// Presence for the signed-in user. Slack answers `presence_query` for
    /// your own id like anyone else's, so this reads the same map.
    pub fn self_presence(&self) -> Presence {
        self.presence
            .get(&self.self_user_id)
            .copied()
            .unwrap_or(Presence::Unknown)
    }

    /// True while the signed-in user's notifications are suppressed by a
    /// manual snooze or an open scheduled Do Not Disturb window.
    pub fn self_snoozed(&self) -> bool {
        self.self_dnd.is_snoozed(now_secs())
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

    /// Record an explicit conversation visit for recents, last-active, and frecency.
    pub fn remember_visit(&mut self, id: &ChannelId) {
        self.last_active_channel = Some(id.clone());
        self.touch_recent(id);
        self.record_visit(id, now_secs());
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

    /// The newest message in a conversation, as Slack's "recent" sort sees it:
    /// `client.counts` `latest`, or a newer message seen since.
    pub fn channel_recency(&self, channel: &Channel) -> u64 {
        let messages = self.messages.get(&channel.id);
        let counted = messages
            .and_then(|cm| cm.latest.as_deref())
            .map(|ts| ts_key(ts).0)
            .unwrap_or(0);
        let seen = messages
            .and_then(ChannelMessages::latest_ts)
            .map(|ts| ts_key(&ts).0)
            .unwrap_or(0);
        counted.max(seen)
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

    fn apply_channel_read_state(&mut self, channel: &Channel) {
        let cm = self.messages.entry(channel.id.clone()).or_default();
        if let Some(last_read) = &channel.last_read {
            if cm.last_read.as_ref() != Some(last_read) {
                cm.read_revision = cm.read_revision.wrapping_add(1);
            }
            cm.last_read = Some(last_read.clone());
        }
        if let Some(unread) = channel.unread_count.or(channel.unread_count_display) {
            cm.unread_count = unread;
        }
        if let Some(mentions) = channel.mention_count {
            cm.mention_count = mentions;
        }
        if let Some(vip) = channel.extra.get("vip_count") {
            cm.vip_count = vip.as_u64().map_or(0, |n| n as u32);
        }
        if let Some(latest) = channel.extra.get("latest").and_then(latest_ts) {
            cm.latest = Some(latest);
        }
        if channel.has_unreads && cm.unread_count == 0 && cm.mention_count == 0 {
            cm.unread_count = 1;
        }
    }
}

/// `client.userBoot` is allowed to return partial user records. Preserve fields
/// learned from `users/info` (and the warm cache) when a boot refresh omits
/// them, while still accepting fields that are present in the newer record.
fn merge_boot_user(existing: Option<User>, mut update: User) -> User {
    let Some(mut existing) = existing else {
        return update;
    };

    preserve_blank_string(&mut update.name, existing.name.take());
    preserve_blank_string(&mut update.real_name, existing.real_name.take());
    update.profile = match (existing.profile.take(), update.profile.take()) {
        (Some(existing), Some(update)) => Some(merge_boot_profile(existing, update)),
        (Some(existing), None) => Some(existing),
        (None, update) => update,
    };
    if update.im_mpim_ids.is_empty() {
        update.im_mpim_ids = existing.im_mpim_ids;
        update.has_more_mpims |= existing.has_more_mpims;
    }
    for (key, value) in existing.extra {
        update.extra.entry(key).or_insert(value);
    }
    update
}

fn merge_boot_profile(
    mut existing: crate::slack::models::UserProfile,
    mut update: crate::slack::models::UserProfile,
) -> crate::slack::models::UserProfile {
    preserve_blank_string(&mut update.real_name, existing.real_name.take());
    preserve_blank_string(&mut update.display_name, existing.display_name.take());
    preserve_missing(&mut update.title, existing.title.take());
    preserve_missing(&mut update.pronouns, existing.pronouns.take());
    preserve_missing(&mut update.phone, existing.phone.take());
    preserve_missing(&mut update.email, existing.email.take());
    preserve_missing(&mut update.start_date, existing.start_date.take());
    preserve_missing(&mut update.image_24, existing.image_24.take());
    preserve_missing(&mut update.image_32, existing.image_32.take());
    preserve_missing(&mut update.image_48, existing.image_48.take());
    preserve_missing(&mut update.image_72, existing.image_72.take());
    preserve_missing(&mut update.image_192, existing.image_192.take());
    preserve_missing(&mut update.image_512, existing.image_512.take());
    preserve_missing(&mut update.image_original, existing.image_original.take());
    preserve_missing(&mut update.avatar_hash, existing.avatar_hash.take());
    preserve_missing(&mut update.team, existing.team.take());
    preserve_missing(&mut update.status_text, existing.status_text.take());
    preserve_missing(&mut update.status_emoji, existing.status_emoji.take());
    preserve_missing(
        &mut update.status_expiration,
        existing.status_expiration.take(),
    );
    if update.fields.is_empty() {
        update.fields = existing.fields;
    }
    for (key, value) in existing.extra {
        update.extra.entry(key).or_insert(value);
    }
    update
}

fn preserve_blank_string(update: &mut Option<String>, existing: Option<String>) {
    if update
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        *update = existing;
    }
}

fn preserve_missing<T>(update: &mut Option<T>, existing: Option<T>) {
    if update.is_none() {
        *update = existing;
    }
}

mod helpers;
pub use helpers::*;
use helpers::{append_unique, custom_emoji_url, linked_list_order, merge_channel_metadata};
pub(crate) use helpers::{apply_message_reaction, merge_message};

mod sidebar;
pub use sidebar::*;
mod notifications;
pub use notifications::{DockAttention, NotificationConfig, NotificationOptions};
mod read_preferences;
pub use read_preferences::*;
pub(crate) mod read_shortcuts;
pub use read_shortcuts::*;
mod sidebar_layout;
pub use sidebar_layout::*;
mod sidebar_prefs;
pub use sidebar_prefs::*;
pub mod bots;
pub mod cfm_payload;
mod channel_lifecycle;
pub mod directory_channels;
pub mod directory_members;
pub mod directory_usergroups;
mod member_list_changes;
pub mod posting_metadata;
pub use channel_lifecycle::ChannelLifecycle;
mod activity_counts;
mod dock_badge;
pub use dock_badge::{
    DockBadge, DockBadgeContribution, DockBadgeProjection, DockCountsState, DockWorkspaceContext,
};
mod channel_sync;
mod thread_counts;
mod unreads;
mod vip;
pub use activity_counts::{ActivityCountsState, BADGE_EXPERIMENT_NAMES, BadgeExperiments};
pub use channel_sync::*;
pub use thread_counts::ThreadCountsState;

#[cfg(test)]
mod tests;

/// `latest` is a ts string in `client.counts`, but a whole message object in
/// `conversations.info`.
fn latest_ts(value: &serde_json::Value) -> Option<MessageTs> {
    value
        .as_str()
        .or_else(|| value.get("ts").and_then(serde_json::Value::as_str))
        .filter(|ts| !ts.is_empty())
        .map(str::to_owned)
}
