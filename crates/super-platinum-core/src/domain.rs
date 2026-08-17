//! Renderer-neutral application state shared by desktop shells.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Instant;

use crate::slack::models::{
    ActivityItem, ChannelId, DmEntry, HistoryPage, Message, MessageTs, TeamId, ThreadViewItem,
    UserId,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerTarget {
    Channel,
    Thread,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachTarget {
    Channel,
    Thread,
}

pub type ActiveThreadKey = (ChannelId, MessageTs);
pub type ThreadKey = (TeamId, ChannelId, MessageTs);
pub type MessageListKey = (TeamId, ChannelId, Option<MessageTs>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReadTarget {
    Conversation {
        team: TeamId,
        channel: ChannelId,
    },
    Thread {
        team: TeamId,
        channel: ChannelId,
        root_ts: MessageTs,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingScrollTarget {
    Message(MessageTs),
    FirstUnreadAfter(MessageTs),
    Latest,
}

impl AttachTarget {
    pub fn composer(self) -> ComposerTarget {
        match self {
            Self::Channel => ComposerTarget::Channel,
            Self::Thread => ComposerTarget::Thread,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSelectionSurface {
    Channel {
        channel: ChannelId,
    },
    Thread {
        channel: ChannelId,
        root_ts: MessageTs,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelectionPoint {
    pub surface: TextSelectionSurface,
    pub message_ts: MessageTs,
    pub message_index: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: TextSelectionPoint,
    pub focus: TextSelectionPoint,
    pub dragging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryLoadKind {
    Latest,
    Since,
    Around,
    Older,
}

#[derive(Debug, Clone)]
pub struct LoadedHistory {
    pub page: HistoryPage,
    pub replace_cached: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFetchAuth {
    Slack,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaViewerKind {
    Image,
    Video,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageViewerSource {
    pub kind: MediaViewerKind,
    pub preview_key: String,
    pub full_url: String,
    pub download_url: String,
    pub fetch_auth: ImageFetchAuth,
    pub filename: String,
    pub author_name: String,
    pub avatar_key: Option<String>,
    pub timestamp: String,
    pub conversation: String,
}

#[derive(Debug, Clone, Default)]
pub struct ActivityState {
    pub items: Vec<ActivityItem>,
    pub hydrated: HashMap<(ChannelId, MessageTs), Message>,
    pub next_cursor: Option<String>,
    pub load_seq: u64,
    pub loading: bool,
    pub loaded: bool,
    pub selected: Option<String>,
    pub unread_only: bool,
}

impl ActivityState {
    pub fn upsert(&mut self, item: ActivityItem) {
        let identity = item.identity();
        if let Some(existing) = self.items.iter_mut().find(|i| i.identity() == identity) {
            if crate::state::cmp_ts(Some(&item.feed_ts), Some(&existing.feed_ts)).is_lt() {
                return;
            }
            if self.selected.as_deref() == Some(existing.key.as_str()) {
                self.selected = Some(item.key.clone());
            }
            *existing = item;
        } else {
            self.items.push(item);
        }
        self.items
            .sort_by(|a, b| crate::state::cmp_ts(Some(&b.feed_ts), Some(&a.feed_ts)));
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnreadsSort {
    #[default]
    Newest,
    Oldest,
}

#[derive(Debug, Clone, Default)]
pub struct UnreadsState {
    pub loading: HashMap<ChannelId, u64>,
    pub loaded: HashSet<ChannelId>,
    pub has_more: HashSet<ChannelId>,
    pub failed: HashSet<ChannelId>,
    pub collapsed: HashSet<ChannelId>,
    pub mark_when_loaded: HashSet<ChannelId>,
    pub focused: Option<ChannelId>,
    pub sort: UnreadsSort,
    pub load_seq: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadsState {
    pub items: Vec<ThreadViewItem>,
    pub max_ts: Option<MessageTs>,
    pub has_more: bool,
    pub load_seq: u64,
    pub loading: bool,
    pub loaded: bool,
    pub vip_only: bool,
    pub selected: Option<(ChannelId, MessageTs)>,
}

impl ThreadsState {
    pub fn upsert(&mut self, item: ThreadViewItem) {
        let identity = item.channel().cloned().zip(item.root_ts().cloned());
        if let Some((channel, root_ts)) = identity {
            if let Some(existing) = self.items.iter_mut().find(|existing| {
                existing.channel() == Some(&channel) && existing.root_ts() == Some(&root_ts)
            }) {
                *existing = item;
            } else {
                self.items.push(item);
            }
        }
        self.items.sort_by(|a, b| {
            crate::state::cmp_ts(
                b.latest_ts().map(String::as_str),
                a.latest_ts().map(String::as_str),
            )
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct DmsState {
    pub entries: Vec<DmEntry>,
    pub next_cursor: Option<String>,
    pub load_seq: u64,
    pub loading: bool,
    pub loaded: bool,
    pub unread_only: bool,
    pub filter: String,
}

impl DmsState {
    pub fn upsert(&mut self, entry: DmEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
            if crate::state::cmp_ts(entry.latest.as_deref(), existing.latest.as_deref()).is_lt() {
                return;
            }
            let channel = entry.channel.clone().or_else(|| existing.channel.take());
            *existing = entry;
            existing.channel = channel;
        } else {
            self.entries.push(entry);
        }
        self.sort();
    }

    pub fn touch(&mut self, channel: &str, message: Message) {
        let Some(ts) = message.ts.clone() else {
            return;
        };
        if let Some(existing) = self.entries.iter_mut().find(|e| e.id == channel) {
            if crate::state::cmp_ts(Some(&ts), existing.latest.as_deref()).is_lt() {
                return;
            }
            existing.latest = Some(ts);
            existing.message = Some(message);
        } else {
            self.entries.push(DmEntry {
                id: channel.to_owned(),
                latest: Some(ts),
                message: Some(message),
                ..Default::default()
            });
        }
        self.sort();
    }

    fn sort(&mut self) {
        self.entries
            .sort_by(|a, b| crate::state::cmp_ts(b.latest.as_deref(), a.latest.as_deref()));
    }
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub channel: ChannelId,
    pub channel_label: String,
    pub message: Message,
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
    pub team: TeamId,
    pub page: u32,
    pub page_count: u32,
    pub total: u64,
    pub hits: Vec<SearchHit>,
    pub loading: bool,
}

#[derive(Debug, Clone)]
pub struct ComposerAttachment {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    pub uploading: bool,
    pub upload_started: Option<Instant>,
    pub upload_cancel: Option<Arc<AtomicBool>>,
    pub upload_progress: Option<Arc<AtomicU64>>,
    pub preview_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PendingFileMessage {
    pub team: TeamId,
    pub channel: ChannelId,
    pub thread_ts: Option<MessageTs>,
    pub message_ts: MessageTs,
    pub client_msg_id: String,
    pub text: String,
    pub attachments: Vec<ComposerAttachment>,
}

#[derive(Debug, Clone)]
pub struct ProfilePaneState {
    pub user: UserId,
    pub loading: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(ts: &str, text: &str) -> Message {
        Message {
            ts: Some(ts.to_owned()),
            text: Some(text.to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn dms_touch_keeps_the_newest_message_and_order() {
        let mut state = DmsState::default();
        state.touch("D1", message("100.0", "first"));
        state.touch("D2", message("200.0", "second"));
        state.touch("D1", message("050.0", "stale"));

        assert_eq!(state.entries[0].id, "D2");
        assert_eq!(
            state.entries[1].message.as_ref().unwrap().text.as_deref(),
            Some("first")
        );
    }

    #[test]
    fn dms_upsert_preserves_an_existing_channel_mapping() {
        let mut state = DmsState::default();
        state.upsert(DmEntry {
            id: "U1".to_owned(),
            channel: Some(crate::slack::models::Channel {
                id: "D1".to_owned(),
                ..Default::default()
            }),
            latest: Some("100.0".to_owned()),
            ..Default::default()
        });
        state.upsert(DmEntry {
            id: "U1".to_owned(),
            latest: Some("200.0".to_owned()),
            ..Default::default()
        });

        assert_eq!(
            state.entries[0]
                .channel
                .as_ref()
                .map(|channel| channel.id.as_str()),
            Some("D1")
        );
        assert_eq!(state.entries[0].latest.as_deref(), Some("200.0"));
    }
}
