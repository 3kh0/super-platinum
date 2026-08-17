//! Renderer-neutral application primitives shared by Super Platinum desktop shells.

pub mod cache;
pub mod config;
pub mod error;
pub mod palette;
pub mod slack;
pub mod state;

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
pub use media::{MediaAssetId, MediaAssetIdError, MediaAssetKind};
pub use supervisor::{Generation, RealtimeSupervisor, SupervisorContext};
