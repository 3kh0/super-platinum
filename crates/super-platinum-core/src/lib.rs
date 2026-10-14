//! Renderer-neutral application primitives shared by Super Platinum desktop shells.

pub mod activity_feed;
pub mod block_actions;
pub mod bookmarks;
pub mod cache;
pub mod canvas;
pub mod composer_completion;
pub mod config;
pub mod direct_messages;
pub mod drafts;
pub mod error;
pub mod file_actions;
pub mod file_preview;
pub mod files;
pub mod forwarding;
pub mod huddle;
pub mod later;
pub mod links;
pub mod net;
pub mod operation_receipts;
pub mod palette;
pub mod pins;
pub mod profile_text;
pub mod saved_file_receipts;
pub mod scheduled;
pub mod search;
pub mod search_query;
pub mod slack;
pub mod slack_lists;
pub mod state;
pub mod thread_feed;
pub mod thread_history;
pub mod thread_reads;
pub mod thread_subscriptions;

mod app_state;
pub mod appearance;
mod command;
mod composer;
mod dispatcher;
pub mod domain;
mod events;
mod geometry;
mod media;
mod supervisor;

pub mod agent_protocol;
pub use app_state::CoreAppState;
pub use command::{
    CapturedScreenshot, Command, CommandAction, UiCommand, UiCommandRequest, UiCommandResult,
};
pub use composer::{ComposerState, FormatMark, TextRange};
pub use dispatcher::{AppDispatcher, DispatchHandle, Reducer, UiCommandExecutor};
pub use events::{ConversationMessage, DiscoveryMessage, RuntimeMessage, WorkspaceMessage};
pub use geometry::{Point, Size, Vector};
pub use media::{
    CachedImage, MediaAssetId, MediaAssetIdError, MediaAssetKind, MediaCacheKind, MediaCacheUsage,
    MediaStore, detect_image_mime,
};
pub use supervisor::{Generation, RealtimeSupervisor, SupervisorContext};
