//! Native async bootstrap and Slack hydration for the desktop shell.

mod common;
mod discovery;
mod history;
mod persist;
mod session;

pub use crate::messaging::{
    delete_message, save_edit, send_composer, send_thread_composer, toggle_reaction,
};

pub(crate) use common::{credentials, persist_workspace, refresh_history};
pub use discovery::*;
pub use history::*;
pub use persist::*;
pub use session::*;
