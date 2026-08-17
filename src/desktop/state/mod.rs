//! Serial shell state and mutation boundary for the Dioxus desktop shell.

mod boot;
mod composer;
mod shell;
mod timeline;

#[cfg(test)]
mod tests;

pub use shell::*;
