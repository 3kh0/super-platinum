//! Serial shell state and mutation boundary for the Dioxus desktop shell.

mod boot;
mod huddle;
pub use huddle::*;
mod composer;
pub(crate) mod dms;
mod dock_badge;
mod drafts;
pub(crate) mod search_filter_types;
pub(crate) mod search_message_scopes;
mod shell;
mod timeline;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod timeline_geometry_tests;

pub use composer::ComposerTarget;
pub use shell::*;
