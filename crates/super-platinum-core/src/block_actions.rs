//! Renderer-neutral identity and presentation metadata for Block Kit elements.
//!
//! Preserving this data does not authorize or implement interaction dispatch.

pub mod form;
pub mod message_state;
mod metadata;
pub mod modal;
pub mod validation;

pub use metadata::{
    BlockConfirmation, BlockConfirmationState, BlockElementMetadata, BlockElementSource, BlockText,
    BlockTextFormat,
};
