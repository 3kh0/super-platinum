//! Native async bootstrap and Slack hydration for the desktop shell.

mod common;
mod discovery;
mod history;
mod palette_navigation;
mod persist;
mod profile_navigation;
mod search;
mod search_navigation;
mod self_menu;
mod session;
mod startup;
mod storage;
mod thread;
pub(crate) use startup::initialize;

pub use crate::messaging::{
    delete_message, save_edit, send_composer, send_thread_composer, toggle_reaction,
};

pub use crate::profile_vip::toggle_profile_vip;
pub use common::reload_after_outage;
pub(crate) use common::{credentials, persist_workspace};
pub use discovery::*;
pub use history::*;
pub(crate) use palette_navigation::{open_palette_entry, open_palette_target};
pub use persist::*;
pub use profile_navigation::open_profile_dm;
pub use search::*;
pub use search_navigation::open_search_hit;
pub use self_menu::*;
pub use session::*;
pub use storage::*;
pub use thread::*;
