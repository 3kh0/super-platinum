use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::config::WorkspaceSession;

use super::Error;
use super::client::{PreparedRequest, SlackClient};
use super::models::{
    ActivityFeedPage, BootData, Channel, ChannelId, ChannelSectionsPage, ClientDmsPage, CountsPage,
    EdgeResults, Emoji, HistoryPage, MessageTs, MessagesListPage, OpenedConversation,
    ProfileExtrasPage, SearchInlinePage, SearchMessagesPage, SentMessage, SidebarDmsPage,
    TeamProfileField, TeamProfilePage, ThreadsViewPage, User, UserId, UserProfile, UserProfilePage,
};
use super::transport::Transport;
mod execute;
mod request;

use request::decode;

pub use execute::*;
pub use request::*;

#[cfg(test)]
mod tests;
