use dioxus::prelude::*;

mod agent;
mod appearance;
mod auth;
mod bootstrap;
mod channel_vm;
mod clipboard;
mod fixture;
mod icons;
mod interactions;
mod media;
mod message_vm;
mod messaging;
mod model;
mod notification;
mod overlays;
mod performance;
mod realtime;
mod runtime;
mod state;
mod view;

use state::ShellState;
pub(crate) use super_platinum_core::{config, slack};

fn main() {
    if let Some(code) = run_auth_mode() {
        std::process::exit(code);
    }
    if let Err(error) = notification::ensure_identity() {
        eprintln!("super-platinum: could not register Windows notification identity: {error}");
    }
    let media = media::MediaRegistry::default();
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
    let mut state = use_signal(move || ShellState::from_environment(media.clone()));
    let desktop = dioxus::desktop::window();
    use_context_provider(|| state);
    let timeline_identity = use_memo(move || {
        let state = state.read();
        (
            state.core.active_channel.clone(),
            state.messages.len(),
            state.stick_to_bottom,
        )
    });
    use_effect(move || {
        let (_, _, stick_to_bottom) = timeline_identity();
        if stick_to_bottom {
            dioxus::document::eval(
                "requestAnimationFrame(() => { const timeline = document.getElementById('message-timeline'); if (timeline) timeline.scrollTop = timeline.scrollHeight; });",
            );
        } else if let Some((_, target)) = state.read().core.pending_scroll_to.clone() {
            spawn(async move {
                crate::bootstrap::scroll_to_pending(target).await;
                state.write().core.pending_scroll_to = None;
            });
        }
        spawn(performance::mark_painted(state));
    });
    use_future(move || {
        let desktop = desktop.clone();
        async move {
            if let Err(error) = agent::serve(state, desktop).await {
                eprintln!("super-platinum: agent control server stopped: {error}");
            }
        }
    });
    use_future(move || bootstrap::refresh(state));
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
                   dioxus.send({type:'escape'});
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
                    if shell.profile_hover.take().is_some() {
                        continue;
                    }
                    if shell.overlay.take().is_some() {
                        continue;
                    }
                    if shell.thread_root.take().is_some() {
                        shell.thread_messages.clear();
                    }
                }
                _ => {}
            }
        }
    });
    view::shell()
}
