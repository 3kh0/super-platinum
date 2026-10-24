//! Exactly two interior splits in one owned draft; no independent second intent.
use super::*;
mod build;
mod checkpoint;
mod proof;
mod witness;
pub(super) use build::{State, build, project};
pub(in crate::canvas::edit::structure) use checkpoint::validate;
pub(super) use proof::{ack, readback};
pub(super) use witness::Second;
pub use witness::SplitSequenceTarget;
