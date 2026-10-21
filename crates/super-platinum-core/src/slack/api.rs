use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::config::WorkspaceSession;

use super::Error;
use super::client::{PreparedRequest, SlackClient};
use super::models::{
    ActivityFeedPage, BootData, Channel, ChannelId, ChannelSectionsPage,
    ChannelTeamConnectionsPage, ClientDmsPage, CountsPage, DndInfo, EdgeResults, Emoji,
    GenericInfoPage, HistoryPage, MessageTs, MessagesListPage, OpenedConversation,
    ProfileExtrasPage, SearchInlinePage, SearchMessagesPage, SentMessage, SidebarDmsPage, Team,
    TeamProfileField, TeamProfilePage, ThreadsViewPage, User, UserId, UserProfile, UserProfilePage,
};
use super::transport::Transport;
pub mod avatar;
mod block_actions;
mod block_suggestions;
mod block_views;
pub use block_suggestions::*;
mod bookmarks;
mod bots;
mod canvas;
mod canvas_collab;
mod canvas_comments;
pub use block_actions::*;
pub use block_views::*;
pub use bookmarks::*;
pub use bots::*;
pub use canvas::*;
pub use canvas_collab::*;
pub use canvas_comments::*;
mod channel_admin;
pub use channel_admin::*;
mod channel_details;
mod channel_members_admin;
mod channel_membership;
pub use channel_members_admin::*;
pub use channel_membership::*;
mod direct_recovery;
pub use direct_recovery::*;
mod drafts;
mod drafts_info;
mod execute;
mod execute_readbacks;
mod experiments;
pub mod file_share;
mod files;
mod hosted_picker;
pub use hosted_picker::*;
mod forward;
mod lists;
mod notification_schedule;
mod notifications;
pub use notification_schedule::*;
pub use notifications::*;
mod read_preferences;
pub use read_preferences::*;
pub(crate) mod read_shortcuts;
pub use read_shortcuts::*;
mod own_profile_text;
pub use own_profile_text::*;
mod pins;
mod profile_edit;
mod profile_sections;
pub use profile_edit::*;
pub use profile_sections::*;
mod request;
mod saved;
mod scheduled;
mod thread_subscriptions;

use request::decode;

pub use channel_details::*;
pub use drafts::*;
pub use drafts_info::{DraftInfo, decode_drafts_info, drafts_info};
pub use execute::*;
pub use execute_readbacks::*;
pub use experiments::*;
pub use files::*;
pub use forward::*;
pub use lists::*;
pub use pins::*;
pub use request::*;
pub use saved::*;
pub use scheduled::*;
pub use thread_subscriptions::*;

#[cfg(test)]
mod tests;
