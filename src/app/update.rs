use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use iced::Task;
use iced::widget::image::Handle as ImageHandle;
use iced::widget::operation::{self, AbsoluteOffset, RelativeOffset};
use unicode_segmentation::UnicodeSegmentation;

use crate::cache::Cache;
use crate::config;
use crate::slack::api::{self, HistoryArgs, SearchArgs};
use crate::slack::events::RtEvent;
use crate::slack::models::{
    Channel, ChannelId, Emoji, Message as SlackMessage, MessageTs, SearchMessagesPage, TeamId,
    UserId,
};
use crate::slack::realtime;
use crate::slack::{Error as SlackError, Transport};
use crate::state::{ChannelMessages, Presence, RealtimeStatus, Screen, Workspace};
use crate::ui;

use super::palette::{self, PaletteEntry, PaletteState, PaletteTarget};
use super::{
    ActivityState, App, AttachTarget, ComposerAttachment, ComposerTarget, DesktopNotification,
    DmsState, FilePreview, HistoryLoadKind, ImageFetchAuth, ImageViewerImage, ImageViewerSource,
    ImageViewerState, MediaViewerKind, Message, PendingFileMessage, PendingScrollTarget,
    PreparedVideo, ProfileHoverState, ProfilePaneState, ReadTarget, SearchHit, SearchState,
    TextSelection, TextSelectionPoint, TextSelectionSurface, ThreadKey, ThreadsState, UnreadsSort,
    UnreadsState, VideoViewerPlayback,
};
use iced::widget::text_editor::{Action, Content, Edit};

const CACHE_SAVE_DEBOUNCE: Duration = Duration::from_millis(750);
const LOAD_OLDER_SCROLL_TOP_PX: f32 = 48.0;
const CHAT_PIN_BOTTOM_PX: f32 = 12.0;
const LOAD_OLDER_ACTIVITY_BOTTOM_PX: f32 = 96.0;
const UNREADS_BATCH_SIZE: usize = 1;

fn start_message_list_animation(app: &mut App, team: &str, channel: &str, root_ts: Option<&str>) {
    let is_active = app.active_team.as_deref() == Some(team)
        && match root_ts {
            Some(root_ts) => {
                app.thread_open
                    && app
                        .active_thread
                        .as_ref()
                        .is_some_and(|(active_channel, active_root)| {
                            active_channel == channel && active_root == root_ts
                        })
            }
            None => {
                app.active_channel.as_deref() == Some(channel)
                    && !app.chat_paused.contains_key(channel)
            }
        };
    if is_active {
        app.message_list_animations.insert(
            (
                team.to_owned(),
                channel.to_owned(),
                root_ts.map(str::to_owned),
            ),
            Instant::now(),
        );
    }
}

mod discovery;
mod dispatch;
mod media;
mod messaging;
mod settings;
mod sync;
mod workspace;

use dispatch::*;
use settings::*;
pub(super) use workspace::preferred_channel;
use workspace::*;

use discovery::*;
use media::*;
pub(super) use media::{allocate_animated_preview, visit_message_emoji_names};
#[cfg(test)]
pub(super) use media::{
    channel_open_scroll_target, emoji_preview_from_bytes, pending_target_ts,
    should_load_older_history, unique_download_path,
};
use messaging::*;
#[cfg(test)]
pub(super) use settings::import_background_sync;
use sync::*;
#[cfg(test)]
pub(super) use sync::{channel_needs_hydration, needs_user_hydration, notification_for_message};
#[cfg(test)]
pub(super) use workspace::{
    begin_mark, begin_thread_mark, is_permanent_mark_error, should_auto_load_activity,
    should_load_older_activity, thread_panel_x_range,
};

pub(super) fn update(app: &mut App, message: Message) -> Task<Message> {
    let message = match message {
        Message::AccountScoped(epoch, message) if epoch == app.account_epoch => *message,
        Message::AccountScoped(_, _) => return Task::none(),
        message => message,
    };
    let task = update_inner(app, message);
    let epoch = app.account_epoch;
    task.map(move |message| scope_message(epoch, message))
}

pub(super) fn scope_message(epoch: u64, message: Message) -> Message {
    match message {
        Message::Runtime(crate::app::RuntimeMessage::AuthenticationFinished(_))
        | Message::AccountScoped(_, _) => message,
        message => Message::AccountScoped(epoch, Box::new(message)),
    }
}
