//! Native async bootstrap and Slack hydration for the desktop shell.

mod common;
mod discovery;
pub(crate) mod dms;
mod history;
pub(crate) mod ordinary_history;
mod palette_navigation;
mod persist;
pub(crate) mod posting_ingress;
mod profile_navigation;
pub(crate) mod profile_search;
pub(crate) mod reconnect;
mod search;
mod search_navigation;
mod self_menu;
mod session;
mod startup;
mod storage;
#[cfg(test)]
pub(crate) mod test_support;
mod thread;
#[path = "search/user_names.rs"]
pub(crate) mod user_names;
pub(crate) use startup::initialize;

pub use crate::messaging::{
    delete_message, save_edit, send_composer, send_thread_composer, toggle_reaction,
};

pub use crate::profile_vip::toggle_profile_vip;
pub(crate) use common::{credentials, persist_workspace, persist_workspace_metadata};
pub use discovery::*;
pub use history::*;
pub(crate) use palette_navigation::{open_palette_entry, open_palette_target};
pub use persist::*;
pub use profile_navigation::open_profile_dm;
pub use reconnect::reload_after_outage;
pub(crate) use search::control_memory as search_control_memory;
pub(crate) use search::file_pages as search_file_pages;
#[cfg(test)]
pub(crate) use search::file_type_io as search_file_type_io;
pub(crate) use search::files as search_files;
pub(crate) use search::message_pages as search_message_pages;
#[cfg(test)]
pub(crate) use search::message_scope_io as search_message_scope_io;
pub(crate) use search::people as search_people;
pub use search::*;
pub use search_navigation::open_search_hit;
pub use self_menu::*;
pub use session::*;
pub use storage::*;
pub use thread::*;

pub(crate) use history::read_opening::{
    check as unread_check, context::Owner as UnreadOwner, fixture as unread_fixture,
};
