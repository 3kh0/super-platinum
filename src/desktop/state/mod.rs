//! Serial shell state and mutation boundary for the Dioxus desktop shell.

mod boot;
mod huddle;
pub use huddle::*;
mod composer;
mod shell;
mod timeline;

#[cfg(test)]
mod tests;

pub use composer::ComposerTarget;
pub use shell::*;
