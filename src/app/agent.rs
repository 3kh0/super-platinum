use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
#[cfg(unix)]
use std::time::Duration;

use iced::Subscription;
#[cfg(unix)]
use iced::futures::SinkExt;
#[cfg(unix)]
use iced::futures::channel::mpsc as iced_mpsc;
use iced::window;
use iced::window::Screenshot;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::oneshot;

use super::palette::PaletteTarget;
use super::update::update;
use super::{App, Message};
use crate::state::Screen;

mod handler;
mod protocol;
mod server;
mod snapshot;

pub use handler::{handle, handle_screenshot};
pub use protocol::{AgentCommand, AgentRequest, AgentResponse};
pub use server::{
    allow_destructive, complete, enabled, set_allow_destructive, socket_path, subscription,
};
pub use snapshot::dump_state;

use handler::main_view_label;
use snapshot::{help_data, palette_entries_json};

#[cfg(test)]
mod tests;
