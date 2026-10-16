#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use dioxus::prelude::*;

mod activity_feed;
mod agent;
mod app_modal;
mod appearance;
mod auth;
mod blocks;
mod bookmarks;
mod bookmarks_check;
mod bootstrap;
mod canvas;
mod canvas_check;
mod canvas_create;
mod channel_discovery;
mod channel_mute;
mod channel_resources;
mod channel_vm;
mod clipboard;
mod clipboard_check;
mod completion_check;
mod composer_emoji;
mod connection;
mod download;
mod drafts;
mod emoji_catalog;
mod emoji_native;
mod emoji_preferences;
mod file_actions;
mod file_actions_check;
mod file_events;
mod file_preview;
mod files;
mod fixture;
mod fonts;
mod forward;
mod global_notifications;
mod huddle;
mod icons;
mod interactions;
mod keyboard_check;
mod later;
mod media;
mod message_buttons;
mod message_vm;
mod messaging;
mod model;
mod new_dm;
mod notification;
mod notification_schedule;
mod overlays;
mod palette;
mod pdf_assets;
mod pdf_preview;
mod performance;
mod pins;
mod presentation_notifications;
mod profile;
mod profile_edit;
mod profile_title;
mod profile_vip;
mod reaction_picker;
mod read_preferences;
mod read_shortcuts;
mod read_visibility;
mod realtime;
mod runtime;
mod scheduled;
mod self_status;
mod slack_links;
mod slack_links_check;
mod slack_lists;
mod state;
mod thread_feed;
mod thread_subscription;
mod thread_unread;
mod thread_vm;
mod thread_window;
mod timeline_geometry;
mod trusted_keyboard_check;
mod unfurl;
mod unread;
mod usergroups;
mod view;
mod viewer_actions_check;

use state::ShellState;
pub(crate) use super_platinum_core::{config, slack};

fn main() {
    if fixture::protected_adoption_requested() {
        #[cfg(not(feature = "protected-native-fixture"))]
        {
            eprintln!("protected-adoption: binary lacks protected-native-fixture");
            std::process::exit(2);
        }
        #[cfg(feature = "protected-native-fixture")]
        if !drafts::protected::adoption_fixture::enabled()
            || fixture::protected_requested()
            || fixture::protected_enrollment_requested()
        {
            eprintln!("protected-adoption: exact exclusive fixture alias and CHECK=1 required");
            std::process::exit(2);
        }
        #[cfg(feature = "protected-native-fixture")]
        if let Err(error) = drafts::protected::adoption_fixture::preflight() {
            eprintln!("protected-adoption: {error}");
            std::process::exit(2);
        }
    }
    if fixture::protected_enrollment_requested() {
        #[cfg(not(feature = "protected-native-fixture"))]
        {
            eprintln!("protected-enrollment: binary lacks protected-native-fixture");
            std::process::exit(2);
        }
        #[cfg(feature = "protected-native-fixture")]
        if !drafts::protected::enrollment_fixture::enabled() || fixture::protected_requested() {
            eprintln!("protected-enrollment: exact exclusive fixture alias and CHECK=1 required");
            std::process::exit(2);
        }
    }
    if fixture::protected_owned_requested() {
        #[cfg(not(feature = "protected-native-fixture"))]
        {
            eprintln!("protected-owned-send: binary lacks protected-native-fixture");
            std::process::exit(2);
        }
        #[cfg(feature = "protected-native-fixture")]
        if !drafts::protected::owned_native::enabled()
            || fixture::protected_requested()
            || fixture::protected_enrollment_requested()
            || fixture::protected_adoption_requested()
        {
            eprintln!("protected-owned-send: exact exclusive fixture alias and CHECK=1 required");
            std::process::exit(2);
        }
    }
    // A feature-off or malformed fixture request must never reach ordinary boot.
    if fixture::protected_requested() {
        #[cfg(not(feature = "protected-native-fixture"))]
        {
            eprintln!("protected-drafts: binary lacks protected-native-fixture");
            std::process::exit(2);
        }
        #[cfg(feature = "protected-native-fixture")]
        if !drafts::protected::native_fixture::enabled() {
            eprintln!("protected-drafts: exact fixture alias and CHECK=1 required");
            std::process::exit(2);
        }
    }
    #[cfg(target_os = "windows")]
    notification::windows::prepare_startup();
    #[cfg(target_os = "windows")]
    if let Some(code) = notification::windows::cold_recovery_mode() {
        std::process::exit(code);
    }
    performance::start();
    if let Some(code) = run_auth_mode() {
        std::process::exit(code);
    }
    if let Err(error) = notification::ensure_identity() {
        eprintln!("super-platinum: could not register notification identity: {error}");
    }
    let media = if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        media::MediaRegistry::default()
    } else {
        media::MediaRegistry::with_persistence()
    };
    let config = media::desktop_config(media.clone());
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(media)
        .launch(app);
}

fn run_auth_mode() -> Option<i32> {
    let magic_arg = std::env::args().find(|argument| auth::is_magic_login_url(argument));
    let auth_mode = std::env::var_os("SUPER_PLATINUM_AUTH").is_some();
    if !auth_mode && magic_arg.is_none() {
        return None;
    }
    let add_account = std::env::var_os("SUPER_PLATINUM_AUTH_ADD").is_some();
    let magic_stdin = std::env::var_os("SUPER_PLATINUM_MAGIC_LOGIN_STDIN").and_then(|_| {
        use std::io::Read;
        let mut url = String::new();
        std::io::stdin().read_to_string(&mut url).ok()?;
        Some(url.trim().to_owned())
    });
    let result = match magic_arg.or(magic_stdin) {
        Some(url) => auth::login_magic(&url),
        None => auth::login(add_account),
    }
    .and_then(|session| {
        config::save_session(&session).map_err(|error| error.to_string())?;
        Ok(())
    });
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("super-platinum auth: {error}");
            1
        }
    })
}

fn app() -> Element {
    #[cfg(feature = "protected-native-fixture")]
    use_context_provider(
        || std::rc::Rc::new(view::composer::protected_operations::Root::default()),
    );
    let media = consume_context::<media::MediaRegistry>();
    let mut state = use_signal(move || {
        if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
            ShellState::from_environment(media.clone())
        } else {
            ShellState::opening(media.clone())
        }
    });
    let desktop = dioxus::desktop::window();
    #[cfg(feature = "emoji-retirement-observation")]
    performance::emoji_retirement::use_close_observer();
    use_context_provider(|| state);
    let attachment_operations = use_signal(scheduled::attachments::Operations::default);
    use_context_provider(|| attachment_operations);
    read_visibility::use_focus_read_marks(state);
    #[cfg(target_os = "macos")]
    notification::attention::use_cleanup();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    notification::badge::use_badge(state, desktop.clone());
    #[cfg(target_os = "windows")]
    notification::windows::use_activation(state, desktop.clone());
    let timeline_identity = use_memo(move || {
        let state = state.read();
        (
            state.timeline_scope(),
            state.channel_rows.len(),
            state.stick_to_bottom,
            state.loading,
            state.signed_in,
            state.channel_rows.last().map(|message| message.id.clone()),
            state.core.pending_scroll_to.clone(),
        )
    });
    use_effect(move || {
        let (scope, _, stick_to_bottom, _, _, _, pending) = timeline_identity();
        if stick_to_bottom {
            spawn(async move {
                if state.peek().timeline_scope() != scope || !state.peek().stick_to_bottom {
                    return;
                }
                if crate::bootstrap::scroll_to_pending(
                    state,
                    super_platinum_core::domain::PendingScrollTarget::Latest,
                )
                .await
                    && state.peek().timeline_scope() == scope
                    && state.peek().core.pending_scroll_to == pending
                    && matches!(
                        pending,
                        Some((_, super_platinum_core::domain::PendingScrollTarget::Latest))
                    )
                {
                    state.write().core.pending_scroll_to = None;
                }
            });
        } else if let Some((_, target)) = state.read().core.pending_scroll_to.clone() {
            let scope = state.peek().timeline_scope();
            let pending = state.peek().core.pending_scroll_to.clone();
            spawn(async move {
                if state.peek().timeline_scope() != scope {
                    return;
                }
                // Spend the anchor only once it lands: on a cold channel the row
                // it names has not been rendered yet.
                if crate::bootstrap::scroll_to_pending(state, target.clone()).await
                    && state.peek().timeline_scope() == scope
                    && state.peek().core.pending_scroll_to == pending
                    && (!matches!(
                        target,
                        super_platinum_core::domain::PendingScrollTarget::Message(_)
                    ) || std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some())
                {
                    state.write().core.pending_scroll_to = None;
                }
            });
        }
        spawn(performance::mark_painted(state));
    });
    // Two frames, not one: the rows that just landed have to be laid out before
    // `scrollHeight` means anything, or the pane pins to a height that is about
    // to grow and the reader is left staring at blank space.
    //
    // The thread pane opens on, and stays parked on, its newest reply — the one
    // the notification is about. `thread_at_bottom` goes false as soon as the
    // reader scrolls up, so history reading is never yanked back down.
    let thread_identity = use_memo(move || {
        let state = state.read();
        (
            state.thread_root.clone(),
            state.thread_scope(),
            state
                .thread_messages
                .last()
                .map(|message| message.id.clone()),
            state.thread_at_bottom,
            state.thread_target.clone(),
            state.thread_window.input_generation,
        )
    });
    use_effect(move || {
        let (root, scope, _, at_bottom, target, input_generation) = thread_identity();
        if root.is_some() && (at_bottom || target.is_some()) {
            spawn(async move {
                // Input may retire this intention before the spawned task starts.
                // Scope alone does not distinguish a reader scrolling this thread.
                {
                    let current = state.peek();
                    if current.thread_scope() != scope
                        || current.thread_target != target
                        || current.thread_window.input_generation != input_generation
                        || (target.is_none() && !current.thread_at_bottom)
                    {
                        return;
                    }
                }
                if crate::bootstrap::scroll_thread(state, target.clone()).await
                    && state.peek().thread_scope() == scope
                    && state.peek().thread_target == target
                    && target.is_some()
                {
                    state.write().thread_target = None;
                }
            });
        }
    });
    use_future(move || {
        let desktop = desktop.clone();
        async move {
            if let Err(error) = agent::serve(state, desktop).await {
                eprintln!("super-platinum: agent control server stopped: {error}");
            }
        }
    });
    dioxus::desktop::use_asset_handler(huddle::ASSET_ROUTE, huddle::serve_asset);
    dioxus::desktop::use_asset_handler("fonts", fonts::serve_asset);
    dioxus::desktop::use_asset_handler("notification-sounds", notification::serve_sound_asset);
    dioxus::desktop::use_asset_handler("pdf", move |request, responder| {
        pdf_preview::serve_asset(state, request, responder)
    });
    use_future(move || huddle::bridge(state));
    // Startup carries a loaded ShellState across awaits. Keep the large future
    // out of Dioxus's by-value hook/spawn frames on Windows's 1 MiB main stack.
    use_future(move || Box::pin(bootstrap::initialize(state)));
    use_future(move || clipboard::watch(state));
    use_future(move || clipboard::file_check::run(state));
    use_future(move || runtime::ticks(state));
    #[cfg(target_os = "macos")]
    use_future(move || notification::attention::check::run(state));
    #[cfg(target_os = "windows")]
    use_future(move || notification::windows::check::run(state));
    use_future(move || performance::watch_scroll(state));
    use_future(move || keyboard_check::run(state));
    use_future(move || trusted_keyboard_check::run(state));
    use_future(move || view::scheduled::clear_check::run(state));
    use_future(move || completion_check::run(state));
    use_future(move || performance::completion::run(state));
    use_future(move || clipboard_check::run(state));
    use_future(move || canvas_check::run(state));
    use_future(move || bookmarks_check::run(state));
    use_future(move || file_preview::native_check(state));
    use_future(move || file_actions_check::run(state));
    use_future(move || viewer_actions_check::run(state));
    use_future(move || pdf_preview::check::run(state));
    use_future(move || async move {
        dioxus::document::eval(include_str!("overlay_keyboard.js"));
        crate::read_shortcuts::install_keyboard();
        // Global shortcuts: K palette, G workspace search, F conversation search.
        let mut bridge = dioxus::document::eval(
            r#"window.addEventListener('keydown', event => {
                 const meta = event.metaKey || event.ctrlKey;
                 if (meta && (event.key === 'k' || event.key === 'K')) {
                   event.preventDefault();
                   window.superPlatinumOverlayKeyboard?.prepare('palette');
                   dioxus.send({type:'palette'});
                 } else if (meta && !event.altKey && !event.shiftKey && ['f','F','g','G'].includes(event.key)) {
                   event.preventDefault();
                   window.superPlatinumOverlayKeyboard?.prepare('search');
                   dioxus.send({type:'search', conversation:event.key.toLowerCase()==='f'});
                 } else if (event.key === 'Escape') {
                   if (event.defaultPrevented || event.repeat || event.isComposing || event.keyCode === 229
                       || event.ctrlKey || event.metaKey || event.altKey || !document.hasFocus()
                       || document.visibilityState !== 'visible') return;
                   if (event.target?.closest?.('[data-escape-local], [data-local-keyboard]')) return;
                   const read = window.superPlatinumReadShortcuts?.capture(event);
                   if (event.shiftKey) {
                     if (read) dioxus.send({type:'read-shortcut', read});
                   } else {
                     dioxus.send({type:'escape', read, huddle: !!event.target.closest?.('[data-huddle-stage]')});
                   }
                 }
               });"#,
        );
        while let Ok(payload) = bridge.recv::<serde_json::Value>().await {
            let kind = payload
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            match kind {
                "search" => {
                    state.write().open_search(
                        payload
                            .get("conversation")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                    );
                }
                "palette" => {
                    let mut shell = state.write();
                    if shell.signed_in && !shell.loading {
                        shell.open_palette();
                    }
                }
                "escape" => {
                    let mut shell = state.write();
                    shell.cancel_profile_dm();
                    if payload.get("huddle").and_then(serde_json::Value::as_bool) == Some(true) {
                        if shell.huddle_ui.settings || shell.huddle_ui.invite {
                            shell.huddle_ui.settings = false;
                            shell.huddle_ui.invite = false;
                        } else if shell.huddle_ui.expanded {
                            shell.huddle_ui.expanded = false;
                        } else {
                            shell.huddle_ui.visible = false;
                        }
                        continue;
                    }
                    if shell.profile_hover.take().is_some() {
                        continue;
                    }
                    if shell.overlay.is_some() {
                        shell.dismiss_overlay();
                        continue;
                    }
                    if shell.profile_user.is_some() || shell.group_panel.is_some() {
                        shell.close_profile();
                        continue;
                    }
                    drop(shell);
                    if let Some(read) = payload.get("read").cloned().and_then(|v| {
                        serde_json::from_value::<crate::read_shortcuts::Payload>(v).ok()
                    }) {
                        spawn(crate::read_shortcuts::handle(state, read));
                    }
                }
                "read-shortcut" => {
                    if let Some(read) = payload.get("read").cloned().and_then(|v| {
                        serde_json::from_value::<crate::read_shortcuts::Payload>(v).ok()
                    }) {
                        spawn(crate::read_shortcuts::handle(state, read));
                    }
                }
                _ => {}
            }
        }
    });
    view::shell()
}
