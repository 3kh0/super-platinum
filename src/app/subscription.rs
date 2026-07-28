use std::collections::HashSet;
use std::time::Duration;

use iced::Subscription;

use crate::slack::models::TeamId;
use crate::slack::realtime::{self, ConnectParams, RtUpdate};

use super::{App, FilePreview, Message};

pub(super) fn subscription(app: &App) -> Subscription<Message> {
    let needs_tick =
        !app.cache_dirty.is_empty() || app.workspaces.values().any(|ws| !ws.typing.is_empty());
    let mut subs = Vec::new();
    if needs_tick {
        subs.push(
            iced::time::every(Duration::from_secs(1))
                .map(|_| Message::Runtime(crate::app::RuntimeMessage::Tick)),
        );
    }

    subs.push(iced::event::listen_with(palette_hotkey));
    subs.push(iced::event::listen_with(file_drop));
    subs.push(iced::event::listen_with(cursor_position));
    subs.push(iced::event::listen_with(
        |event, _status, _id| match event {
            iced::Event::Mouse(iced::mouse::Event::WheelScrolled { .. }) => {
                Some(Message::Runtime(crate::app::RuntimeMessage::ScrollActivity))
            }
            _ => None,
        },
    ));
    if app.text_selection.is_some() {
        subs.push(iced::event::listen_with(selection_copy_hotkey));
    }
    if app
        .text_selection
        .as_ref()
        .is_some_and(|selection| selection.dragging)
    {
        subs.push(iced::event::listen_with(selection_mouse_release));
    }
    if app.image_viewer.as_ref().is_some_and(|viewer| viewer.open) {
        subs.push(iced::event::listen_with(image_viewer_navigation));
    } else if app.palette_open {
        subs.push(iced::event::listen_with(palette_navigation));
    } else if app.profile_pane.is_some() {
        subs.push(iced::event::listen_with(profile_navigation));
    } else if app.main_view == crate::state::MainView::Unreads && app.active_thread.is_none() {
        subs.push(iced::event::listen_with(unreads_navigation));
    }
    let visible_media_animation_interval = visible_media_animation_interval(app);
    let needs_existing_animation_tick = has_pending_sends(app)
        || app
            .composer_attachments
            .iter()
            .any(|attachment| attachment.uploading)
        || app
            .thread_composer_attachments
            .iter()
            .any(|attachment| attachment.uploading)
        || app
            .pending_file_messages
            .iter()
            .flat_map(|pending| &pending.attachments)
            .any(|attachment| attachment.uploading)
        || app.scrollbar_visible_until.is_some();
    if needs_existing_animation_tick || visible_media_animation_interval.is_some() {
        let interval = match (
            needs_existing_animation_tick,
            visible_media_animation_interval,
        ) {
            (true, Some(media)) => Duration::from_millis(50).min(media),
            (true, None) => Duration::from_millis(50),
            (false, Some(media)) => media,
            (false, None) => unreachable!(),
        };
        subs.push(
            iced::time::every(interval)
                .map(|_| Message::Runtime(crate::app::RuntimeMessage::AnimationTick)),
        );
    }

    if app.sidebar_resizing {
        subs.push(iced::event::listen_with(
            |event, _status, _id| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => Some(
                    Message::Runtime(crate::app::RuntimeMessage::SidebarResizeMoved(position.x)),
                ),
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::Runtime(
                    crate::app::RuntimeMessage::SidebarResizeEnded,
                )),
                _ => None,
            },
        ));
    }

    if let Some(session) = &app.session {
        let user_agent = crate::slack::xparams::Identity::from_capture().user_agent;
        for ws in session.workspaces.values() {
            let params = ConnectParams {
                team: ws.team_id.clone(),
                ws_url: realtime::flannel_url(&ws.token, &ws.team_id),
                d_cookie: session.d_cookie.clone(),
                user_agent: user_agent.clone(),
            };
            subs.push(
                realtime::connect(params)
                    .with(app.account_epoch)
                    .map(map_scoped_rt_update),
            );
        }
    }

    if super::agent::enabled() {
        subs.push(super::agent::subscription());
    }

    Subscription::batch(subs)
}

fn unreads_navigation(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    use iced::keyboard::key::Named;
    use iced::keyboard::{Event, Key};
    match event {
        iced::Event::Keyboard(Event::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        }) => Some(Message::Runtime(
            crate::app::RuntimeMessage::UnreadsMarkFocused,
        )),
        _ => None,
    }
}

pub(super) fn visible_media_animation_interval(app: &App) -> Option<Duration> {
    if app.screen != crate::state::Screen::Main
        || app.search.is_some()
        || app.image_viewer.is_some()
    {
        return None;
    }
    let team = app.active_team.as_ref()?;
    let channel = app.active_channel.as_ref()?;
    let workspace = app.workspaces.get(team)?;
    let mut interval = None;
    let mut emoji_names = HashSet::new();

    if let Some(messages) = workspace.messages.get(channel) {
        for message in &messages.messages {
            update_message_animation_interval(app, message, &mut emoji_names, &mut interval);
        }
    }

    if let Some((thread_channel, root_ts)) = app.active_thread.as_ref() {
        if let Some(root) = workspace.messages.get(thread_channel).and_then(|messages| {
            messages
                .messages
                .iter()
                .find(|message| message.ts.as_deref() == Some(root_ts))
        }) {
            update_message_animation_interval(app, root, &mut emoji_names, &mut interval);
        }
        if let Some(replies) =
            app.threads
                .get(&(team.clone(), thread_channel.clone(), root_ts.clone()))
        {
            for message in &replies.messages {
                update_message_animation_interval(app, message, &mut emoji_names, &mut interval);
            }
        }
    }

    let emoji_prefix = format!("{team}:");
    for (key, preview) in &app.emoji_previews {
        let Some(name) = key.strip_prefix(&emoji_prefix) else {
            continue;
        };
        if emoji_names.contains(name) {
            update_preview_animation_interval(preview, &mut interval);
        }
    }

    interval.map(|interval: Duration| {
        interval.clamp(Duration::from_millis(16), Duration::from_millis(100))
    })
}

fn update_message_animation_interval<'a>(
    app: &App,
    message: &'a crate::slack::models::Message,
    emoji_names: &mut HashSet<&'a str>,
    interval: &mut Option<Duration>,
) {
    super::update::visit_message_emoji_names(message, |name| {
        emoji_names.insert(name);
    });
    let file_keys = message
        .files
        .iter()
        .filter_map(crate::state::file_preview_key_ref);
    let attachment_keys = message
        .attachments
        .iter()
        .flat_map(crate::state::attachment_images)
        .map(|image| image.preview_url);

    for key in file_keys.chain(attachment_keys) {
        let Some(preview) = app.file_previews.get(key) else {
            continue;
        };
        update_preview_animation_interval(preview, interval);
    }
}

fn update_preview_animation_interval(preview: &FilePreview, interval: &mut Option<Duration>) {
    if let FilePreview::Animated { delays, .. } = preview {
        let delay = delays
            .iter()
            .copied()
            .min()
            .unwrap_or(Duration::from_millis(50));
        *interval = Some(interval.map_or(delay, |current| current.min(delay)));
    }
}

fn image_viewer_navigation(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    use iced::keyboard::key::Named;
    use iced::keyboard::{Event, Key};
    match event {
        iced::Event::Keyboard(Event::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        }) => Some(Message::Discovery(
            crate::app::DiscoveryMessage::ImageViewerClosed,
        )),
        _ => None,
    }
}

fn cursor_position(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => Some(
            Message::Workspace(crate::app::WorkspaceMessage::CursorMoved(position)),
        ),
        _ => None,
    }
}

fn profile_navigation(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    use iced::keyboard::key::Named;
    use iced::keyboard::{Event, Key};
    match event {
        iced::Event::Keyboard(Event::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        }) => Some(Message::Workspace(
            crate::app::WorkspaceMessage::ProfileDismissed,
        )),
        _ => None,
    }
}

fn file_drop(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Window(iced::window::Event::FileDropped(path)) => Some(Message::Conversation(
            crate::app::ConversationMessage::FilesDropped(vec![path]),
        )),
        _ => None,
    }
}

fn selection_copy_hotkey(
    event: iced::Event,
    status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    use iced::keyboard::{Event, Key};
    if status == iced::event::Status::Captured {
        return None;
    }
    let iced::Event::Keyboard(Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    match key {
        Key::Character(c) if c.as_str().eq_ignore_ascii_case("c") && modifiers.command() => Some(
            Message::Workspace(crate::app::WorkspaceMessage::TextSelectionCopyRequested),
        ),
        _ => None,
    }
}

fn selection_mouse_release(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => Some(
            Message::Workspace(crate::app::WorkspaceMessage::TextSelectionEnded),
        ),
        _ => None,
    }
}

fn has_pending_sends(app: &App) -> bool {
    app.workspaces
        .values()
        .flat_map(|ws| ws.messages.values())
        .any(|cm| !cm.pending.is_empty())
        || app.threads.values().any(|cm| !cm.pending.is_empty())
}

fn palette_hotkey(
    event: iced::Event,
    status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    use iced::keyboard::{Event, Key};
    if status == iced::event::Status::Captured {
        return None;
    }
    let iced::Event::Keyboard(Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    match key {
        Key::Character(c) if c.as_str().eq_ignore_ascii_case("k") && modifiers.command() => Some(
            Message::Discovery(crate::app::DiscoveryMessage::PaletteToggled),
        ),
        _ => None,
    }
}

fn palette_navigation(
    event: iced::Event,
    _status: iced::event::Status,
    _id: iced::window::Id,
) -> Option<Message> {
    use iced::keyboard::key::Named;
    use iced::keyboard::{Event, Key};
    let iced::Event::Keyboard(Event::KeyPressed { key, .. }) = event else {
        return None;
    };
    match key {
        Key::Named(Named::ArrowUp) => Some(Message::Discovery(
            crate::app::DiscoveryMessage::PaletteMoved(-1),
        )),
        Key::Named(Named::ArrowDown) => Some(Message::Discovery(
            crate::app::DiscoveryMessage::PaletteMoved(1),
        )),
        Key::Named(Named::Escape) => Some(Message::Discovery(
            crate::app::DiscoveryMessage::PaletteClosed,
        )),
        _ => None,
    }
}

fn map_rt_update((team, update): (TeamId, RtUpdate)) -> Message {
    match update {
        RtUpdate::Connected {
            generation,
            connection,
        } => Message::Runtime(crate::app::RuntimeMessage::RtConnected(
            team, generation, connection,
        )),
        RtUpdate::Event { generation, event } => Message::Runtime(
            crate::app::RuntimeMessage::Realtime(team, generation, event),
        ),
        RtUpdate::Disconnected { generation } => {
            Message::Runtime(crate::app::RuntimeMessage::RtDisconnected(team, generation))
        }
    }
}

fn map_scoped_rt_update((epoch, update): (u64, (TeamId, RtUpdate))) -> Message {
    Message::AccountScoped(epoch, Box::new(map_rt_update(update)))
}
