use dioxus::prelude::*;

mod agent;
mod appearance;
mod auth;
mod blocks;
mod bootstrap;
mod channel_resources;
mod channel_vm;
mod clipboard;
mod connection;
mod download;
mod files;
mod fixture;
mod fonts;
mod forward;
mod huddle;
mod icons;
mod interactions;
mod later;
mod media;
mod message_vm;
mod messaging;
mod model;
mod notification;
mod overlays;
mod performance;
mod pins;
mod profile;
mod reaction_picker;
mod realtime;
mod runtime;
mod scheduled;
mod state;
mod thread_vm;
mod unfurl;
mod usergroups;
mod view;

use state::ShellState;
pub(crate) use super_platinum_core::{config, slack};

fn main() {
    performance::start();
    if let Some(code) = run_auth_mode() {
        std::process::exit(code);
    }
    if let Err(error) = notification::ensure_identity() {
        eprintln!("super-platinum: could not register notification identity: {error}");
    }
    let media = media::MediaRegistry::with_persistence();
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
    let media = consume_context::<media::MediaRegistry>();
    let mut state = use_signal(move || {
        if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
            ShellState::from_environment(media.clone())
        } else {
            ShellState::opening(media.clone())
        }
    });
    let desktop = dioxus::desktop::window();
    use_context_provider(|| state);
    let timeline_identity = use_memo(move || {
        let state = state.read();
        (
            state.timeline_scope(),
            state.messages.len(),
            state.stick_to_bottom,
            state.loading,
            state.signed_in,
        )
    });
    use_effect(move || {
        let (scope, _, stick_to_bottom, _, _) = timeline_identity();
        if stick_to_bottom {
            spawn(async move {
                if state.peek().timeline_scope() != scope || !state.peek().stick_to_bottom {
                    return;
                }
                crate::bootstrap::scroll_to_pending(
                    state,
                    super_platinum_core::domain::PendingScrollTarget::Latest,
                )
                .await;
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
            state.thread_messages.len(),
            state.thread_at_bottom,
            state.thread_target.clone(),
        )
    });
    use_effect(move || {
        let (root, scope, _, at_bottom, target) = thread_identity();
        if root.is_some() && (at_bottom || target.is_some()) {
            spawn(async move {
                if state.peek().thread_scope() != scope {
                    return;
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
    use_future(move || huddle::bridge(state));
    use_future(move || bootstrap::initialize(state));
    use_future(move || clipboard::watch(state));
    use_future(move || runtime::ticks(state));
    use_future(move || performance::watch_scroll(state));
    use_future(move || async move {
        // Global shortcuts: ⌘/Ctrl+K palette, Esc closes overlays/hover.
        let mut bridge = dioxus::document::eval(
            r#"window.addEventListener('keydown', event => {
                 const meta = event.metaKey || event.ctrlKey;
                 if (meta && (event.key === 'k' || event.key === 'K')) {
                   event.preventDefault();
                   dioxus.send({type:'palette'});
                 } else if (event.key === 'Escape') {
                   dioxus.send({type:'escape', huddle: !!event.target.closest?.('[data-huddle-stage]')});
                 }
               });"#,
        );
        while let Ok(payload) = bridge.recv::<serde_json::Value>().await {
            let kind = payload
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            match kind {
                "palette" => {
                    let mut shell = state.write();
                    if shell.signed_in && !shell.loading {
                        shell.overlay = Some(state::Overlay::Palette);
                        shell.profile_hover = None;
                    }
                }
                "escape" => {
                    let mut shell = state.write();
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
                    if shell.overlay.take().is_some() {
                        continue;
                    }
                    if shell.profile_user.is_some() || shell.group_panel.is_some() {
                        shell.close_profile();
                        continue;
                    }
                    if shell.thread_root.is_some() {
                        // Esc out of an Activity thread returns to the empty
                        // pane; elsewhere the thread is a side panel and the
                        // conversation behind it stays.
                        if shell.main_view == crate::state::MainView::Activity {
                            shell.forget_surface();
                        } else {
                            shell.close_thread();
                        }
                    }
                }
                _ => {}
            }
        }
    });
    view::shell()
}
