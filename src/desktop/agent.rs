use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use serde_json::{Value, json};
use super_platinum_core::agent_protocol::{AgentCommand, AgentRequest, AgentResponse};
#[cfg(any(unix, target_os = "windows"))]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(target_os = "windows")]
use tokio::net::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

use crate::state::{MainView, Overlay, ShellState};

static ALLOW_DESTRUCTIVE: AtomicBool = AtomicBool::new(false);
static NEXT_FALLBACK_ID: AtomicU64 = AtomicU64::new(1);

pub async fn serve(
    state: Signal<ShellState>,
    desktop: dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    if std::env::var_os("SUPER_PLATINUM_AGENT").is_none() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        serve_unix(state, desktop).await
    }
    #[cfg(target_os = "windows")]
    {
        serve_windows(state, desktop).await
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = (state, desktop);
        Err("agent control is unsupported on this platform".into())
    }
}

pub fn socket_path() -> PathBuf {
    std::env::var("SUPER_PLATINUM_AGENT_SOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("super-platinum-agent.sock"))
}

#[cfg(unix)]
async fn serve_unix(
    state: Signal<ShellState>,
    desktop: dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let path = socket_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| format!("remove stale socket: {error}"))?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create socket dir: {error}"))?;
    }
    let listener =
        UnixListener::bind(&path).map_err(|error| format!("bind {}: {error}", path.display()))?;
    let marker = std::env::temp_dir().join("super-platinum-agent.sock.path");
    let _ = std::fs::write(marker, path.to_string_lossy().as_bytes());

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|error| format!("accept: {error}"))?;
        if let Err(error) = handle_client(stream, state, &desktop).await {
            eprintln!("super-platinum: agent client disconnected: {error}");
        }
    }
}

#[cfg(unix)]
async fn handle_client(
    stream: UnixStream,
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| format!("read: {error}"))?
    {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<AgentRequest>(&line) {
            Ok(request) => dispatch(&mut state, request, desktop),
            Err(error) => AgentResponse::err(0, format!("invalid request json: {error}")),
        };
        let payload = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        writer
            .write_all(&payload)
            .await
            .map_err(|error| format!("write: {error}"))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|error| format!("write: {error}"))?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
async fn serve_windows(
    state: Signal<ShellState>,
    desktop: dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let address =
        std::env::var("SUPER_PLATINUM_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:0".into());
    let listener = TcpListener::bind(&address)
        .await
        .map_err(|error| format!("bind {address}: {error}"))?;
    let endpoint = format!(
        "tcp://{}",
        listener.local_addr().map_err(|error| error.to_string())?
    );
    std::fs::write(
        std::env::temp_dir().join("super-platinum-agent.sock.path"),
        &endpoint,
    )
    .map_err(|error| error.to_string())?;
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|error| format!("accept: {error}"))?;
        if let Err(error) = handle_tcp_client(stream, state, &desktop).await {
            eprintln!("super-platinum: agent client disconnected: {error}");
        }
    }
}

#[cfg(target_os = "windows")]
async fn handle_tcp_client(
    stream: TcpStream,
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| format!("read: {error}"))?
    {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<AgentRequest>(&line) {
            Ok(request) => dispatch(&mut state, request, desktop),
            Err(error) => AgentResponse::err(0, format!("invalid request json: {error}")),
        };
        let mut payload = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        payload.push(b'\n');
        writer
            .write_all(&payload)
            .await
            .map_err(|error| format!("write: {error}"))?;
    }
    Ok(())
}

fn dispatch(
    state: &mut Signal<ShellState>,
    request: AgentRequest,
    desktop: &dioxus::desktop::DesktopContext,
) -> AgentResponse {
    let id = if request.id == 0 {
        NEXT_FALLBACK_ID.fetch_add(1, Ordering::Relaxed)
    } else {
        request.id
    };
    if request.cmd.is_destructive() && !allow_destructive() {
        return AgentResponse::err(
            id,
            "destructive agent commands are disabled; run allow-destructive first",
        );
    }
    match request.cmd {
        AgentCommand::Ping => AgentResponse::ok(id, json!({ "pong": true })),
        AgentCommand::Help => AgentResponse::ok(id, help_data()),
        AgentCommand::State => AgentResponse::ok(id, state_snapshot(&state.read())),
        AgentCommand::OpenPalette => {
            let mut shell = state.write();
            shell.overlay = Some(Overlay::Palette);
            shell.palette_selected = 0;
            AgentResponse::ok(id, json!({ "palette_open": true }))
        }
        AgentCommand::ClosePalette => {
            state.write().overlay = None;
            AgentResponse::ok(id, json!({ "palette_open": false }))
        }
        AgentCommand::SetQuery { query } | AgentCommand::Type { text: query } => {
            {
                let mut shell = state.write();
                shell.palette_query = query.clone();
                shell.palette_selected = 0;
            }
            // The field renders `initial_value`, so a driven query has to be
            // typed into the DOM as well, or screenshots show an empty box.
            crate::view::composer::set_field_text("overlay-input", &query);
            AgentResponse::ok(id, json!({ "query": query }))
        }
        AgentCommand::Move { delta } => {
            state.write().move_palette(delta);
            AgentResponse::ok(id, json!({ "selected": state.read().palette_selected }))
        }
        AgentCommand::Submit | AgentCommand::SelectEntry { index: 0 } => {
            if state.write().submit_palette() {
                dioxus::prelude::spawn(crate::bootstrap::refresh_selected_channel(*state));
                AgentResponse::ok(id, json!({ "submitted": true }))
            } else {
                AgentResponse::err(id, "palette has no matching entry")
            }
        }
        AgentCommand::SelectEntry { index } => {
            let channel_index = state.read().palette_matches().get(index).copied();
            if let Some(channel_index) = channel_index {
                state
                    .write()
                    .select_channel(channel_index, crate::state::ChannelOpen::Global);
                dioxus::prelude::spawn(crate::bootstrap::refresh_selected_channel(*state));
                AgentResponse::ok(id, json!({ "index": index }))
            } else {
                AgentResponse::err(id, format!("no palette entry at {index}"))
            }
        }
        AgentCommand::SelectChannel { channel } => {
            let index = state.read().channels.iter().position(|candidate| {
                candidate.id == channel || candidate.name.eq_ignore_ascii_case(&channel)
            });
            match index {
                Some(index) => {
                    state
                        .write()
                        .select_channel(index, crate::state::ChannelOpen::Global);
                    dioxus::prelude::spawn(crate::bootstrap::refresh_selected_channel(*state));
                    AgentResponse::ok(id, json!({ "channel": channel }))
                }
                None => AgentResponse::err(id, format!("channel not found: {channel}")),
            }
        }
        AgentCommand::SelectWorkspace { team } => {
            let index = state
                .read()
                .workspaces
                .iter()
                .position(|workspace| workspace.id == team || workspace.name == team);
            match index {
                Some(index) => {
                    state.write().select_workspace(index);
                    dioxus::prelude::spawn(crate::bootstrap::refresh_selected_channel(*state));
                    AgentResponse::ok(id, json!({ "team": team }))
                }
                None => AgentResponse::err(id, format!("workspace not found: {team}")),
            }
        }
        AgentCommand::Search { query } => {
            let signal = *state;
            {
                let mut shell = state.write();
                shell.search_query = query.clone();
                shell.overlay = Some(Overlay::Search);
            }
            dioxus::prelude::spawn(crate::bootstrap::search(signal));
            AgentResponse::ok(id, json!({ "query": query }))
        }
        AgentCommand::ClearSearch => {
            state.write().overlay = None;
            AgentResponse::ok(id, json!({ "search_active": false }))
        }
        AgentCommand::OpenSettings => {
            let mut shell = state.write();
            if shell.overlay != Some(Overlay::Settings) {
                shell.settings_section = crate::state::SettingsSection::Appearance;
            }
            shell.overlay = Some(Overlay::Settings);
            AgentResponse::ok(id, json!({ "settings_open": true }))
        }
        AgentCommand::CloseSettings => {
            state.write().overlay = None;
            AgentResponse::ok(id, json!({ "closed": true }))
        }
        AgentCommand::CloseProfile => {
            state.write().close_profile();
            AgentResponse::ok(id, json!({ "closed": true }))
        }
        AgentCommand::OpenProfile { user } => {
            dioxus::prelude::spawn(crate::bootstrap::open_profile(*state, user.clone()));
            AgentResponse::ok(id, json!({ "profile_user": user }))
        }
        AgentCommand::Screenshot { path } => match capture_window(path, desktop) {
            Ok((path, width, height)) => AgentResponse::ok(
                id,
                json!({ "path": path.display().to_string(), "width": width, "height": height }),
            ),
            Err(error) => AgentResponse::err(id, error),
        },
        AgentCommand::AllowDestructive { enabled } => {
            ALLOW_DESTRUCTIVE.store(enabled, Ordering::Relaxed);
            AgentResponse::ok(id, json!({ "allow_destructive": enabled }))
        }
        AgentCommand::Send => {
            dioxus::prelude::spawn(crate::bootstrap::send_composer(*state));
            AgentResponse::ok(id, json!({ "sent": true }))
        }
        AgentCommand::Toast { text } => {
            state.write().show_toast(text.clone());
            AgentResponse::ok(id, json!({ "toast": text }))
        }
        AgentCommand::MainView { view } => {
            let target = match view.as_str() {
                "home" => MainView::Home,
                "unreads" => MainView::Unreads,
                "threads" => MainView::Threads,
                "activity" => MainView::Activity,
                "dms" => MainView::Dms,
                other => return AgentResponse::err(id, format!("unknown main view: {other}")),
            };
            dioxus::prelude::spawn(crate::bootstrap::load_main_view(*state, target));
            AgentResponse::ok(id, json!({ "main_view": view }))
        }
        AgentCommand::ActivitySelect { index } => {
            let target = state
                .read()
                .core
                .activity
                .items
                .get(index)
                .and_then(|item| {
                    Some((
                        item.key.clone(),
                        item.channel()?.to_owned(),
                        item.ts()?.to_owned(),
                        item.thread_ts().map(str::to_owned),
                    ))
                });
            let Some((key, channel, ts, thread_ts)) = target else {
                return AgentResponse::err(id, format!("no activity item at {index}"));
            };
            let opened = state.write().select_activity_item(
                key.clone(),
                Some(&channel),
                Some(&ts),
                thread_ts.as_deref(),
            );
            dioxus::prelude::spawn(crate::bootstrap::mark_activity_read(*state, key));
            if !opened {
                return AgentResponse::err(
                    id,
                    format!("activity channel is not loaded: {channel}"),
                );
            }
            if let Some(root) = thread_ts {
                dioxus::prelude::spawn(crate::bootstrap::open_thread(
                    *state,
                    channel.clone(),
                    root,
                ));
            } else {
                dioxus::prelude::spawn(crate::bootstrap::refresh_selected_channel(*state));
            }
            AgentResponse::ok(id, json!({ "index": index, "channel": channel, "ts": ts }))
        }
    }
}

fn resolve_screenshot_path(path: Option<String>) -> PathBuf {
    match path {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            PathBuf::from("tmp/agent-ui").join(format!("dioxus-{stamp}.png"))
        }
    }
}

#[cfg(target_os = "macos")]
fn capture_window(
    path: Option<String>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(PathBuf, u32, u32), String> {
    use dioxus::desktop::wry::WebViewExtMacOS;

    let path = resolve_screenshot_path(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let window_number = desktop.webview.ns_window().windowNumber();
    let status = std::process::Command::new("screencapture")
        .arg("-x")
        .arg("-o")
        .arg("-l")
        .arg(window_number.to_string())
        .arg(&path)
        .status()
        .map_err(|error| format!("start screencapture: {error}"))?;
    if !status.success() {
        return Err(format!("screencapture exited with {status}"));
    }
    let (width, height) = png_dimensions(&path)?;
    Ok((path, width, height))
}

#[cfg(target_os = "linux")]
fn capture_window(
    path: Option<String>,
    _desktop: &dioxus::desktop::DesktopContext,
) -> Result<(PathBuf, u32, u32), String> {
    let path = resolve_screenshot_path(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let status = std::process::Command::new("gnome-screenshot")
        .args(["-w", "-f"])
        .arg(&path)
        .status();
    let captured = status.is_ok_and(|status| status.success()) || {
        let window = std::process::Command::new("xdotool")
            .arg("getactivewindow")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|window| window.trim().to_owned());
        window.is_some_and(|window| {
            std::process::Command::new("import")
                .args(["-window", &window])
                .arg(&path)
                .status()
                .is_ok_and(|status| status.success())
        })
    };
    if !captured {
        return Err(
            "neither gnome-screenshot nor ImageMagick could capture the active Super Platinum window".into(),
        );
    }
    let (width, height) = png_dimensions(&path)?;
    Ok((path, width, height))
}

#[cfg(target_os = "windows")]
fn capture_window(
    path: Option<String>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(PathBuf, u32, u32), String> {
    use tao::platform::windows::WindowExtWindows;
    let path = resolve_screenshot_path(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let script = r#"Add-Type @'
using System; using System.Runtime.InteropServices;
public class SuperPlatinumCapture { [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; } [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r); }
'@; $r=New-Object SuperPlatinumCapture+RECT; [SuperPlatinumCapture]::GetWindowRect([IntPtr]::new([int64]$args[0]),[ref]$r)|Out-Null; Add-Type -AssemblyName System.Drawing; $b=New-Object System.Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T); $g=[System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($r.L,$r.T,0,0,$b.Size); $b.Save($args[1],[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $b.Dispose()"#;
    let status = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
            &desktop.window.hwnd().to_string(),
        ])
        .arg(&path)
        .status()
        .map_err(|error| format!("start PowerShell capture: {error}"))?;
    if !status.success() {
        return Err(format!("PowerShell capture exited with {status}"));
    }
    let (width, height) = png_dimensions(&path)?;
    Ok((path, width, height))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn capture_window(
    _path: Option<String>,
    _desktop: &dioxus::desktop::DesktopContext,
) -> Result<(PathBuf, u32, u32), String> {
    Err("native capture is unsupported on this platform".into())
}

fn png_dimensions(path: &std::path::Path) -> Result<(u32, u32), String> {
    use std::io::Read;
    let mut header = [0_u8; 24];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| format!("read screenshot: {error}"))?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return Err("screenshot is not a PNG".into());
    }
    Ok((
        u32::from_be_bytes(header[16..20].try_into().expect("four width bytes")),
        u32::from_be_bytes(header[20..24].try_into().expect("four height bytes")),
    ))
}

fn allow_destructive() -> bool {
    ALLOW_DESTRUCTIVE.load(Ordering::Relaxed)
        || std::env::var_os("SUPER_PLATINUM_AGENT_ALLOW_DESTRUCTIVE").is_some()
}

fn state_snapshot(state: &ShellState) -> Value {
    let screen = if !state.signed_in {
        "login"
    } else if state.loading {
        "loading"
    } else {
        "main"
    };
    let active_channel = state.channels.get(state.active_channel);
    let palette_matches = state.palette_matches();
    let profile = state.profile_user.as_ref().map(|user_id| {
        let workspace = state
            .core
            .active_team
            .as_ref()
            .and_then(|team| state.core.workspaces.get(team));
        let user = workspace.and_then(|workspace| workspace.users.get(user_id));
        let profile = user.and_then(|user| user.profile.as_ref());
        json!({
            "user": user_id,
            "name": user.map(|user| super_platinum_core::state::display_name(Some(user), user_id)),
            "loading": state.core.profile_pane.as_ref().is_some_and(|pane| pane.user == *user_id && pane.loading),
            "is_vip": workspace.is_some_and(|workspace| workspace.vip_users.contains(user_id)),
            "recent_dm_count": user.map(|user| user.im_mpim_ids.len()).unwrap_or(0),
            "custom_field_count": profile.map(|profile| profile.fields.len()).unwrap_or(0),
            "pane_width": state.profile_pane_width,
        })
    });
    let settings_section = match state.settings_section {
        crate::state::SettingsSection::Appearance => "appearance",
        crate::state::SettingsSection::Storage => "storage",
    };
    let storage = {
        let usage = state.storage.usage;
        json!({
            "scanning": state.storage.scanning,
            "avatars": usage.avatars,
            "emoji": usage.emoji,
            "icons": usage.icons,
            "other": usage.other,
            "workspace": usage.workspace,
            "pictures": usage.pictures(),
            "total": usage.total(),
            "selected_pictures": state.storage.selected_picture_bytes(),
        })
    };
    json!({
        "screen": screen,
        "signed_in": state.signed_in,
        "main_view": main_view_label(state.main_view),
        "active_team": state.workspaces.get(state.active_workspace).map(|workspace| &workspace.id),
        "active_channel": active_channel.map(|channel| &channel.id),
        "active_channel_name": active_channel.map(|channel| &channel.name),
        "thread_open": state.thread_root.is_some(),
        "active_thread": state.thread_root,
        "profile": profile,
        "profile_hover": state.profile_hover.as_ref().map(|hover| &hover.user_id),
        "palette_open": state.overlay == Some(Overlay::Palette),
        "palette": (state.overlay == Some(Overlay::Palette)).then(|| json!({
            "query": state.palette_query,
            "selected": state.palette_selected,
            "entry_count": palette_matches.len(),
            "entries": palette_matches.iter().enumerate().map(|(selection, index)| json!({
                "index": selection,
                "label": state.channels[*index].name,
                "target": { "kind": "channel", "id": state.channels[*index].id },
                "selected": selection == state.palette_selected,
            })).collect::<Vec<_>>()
        })),
        "self_menu_open": state.overlay == Some(Overlay::SelfMenu),
        "accounts_open": state.overlay == Some(Overlay::Accounts),
        "settings_open": state.overlay == Some(Overlay::Settings),
        "settings_section": settings_section,
        "storage": storage,
        "search_input": state.search_query,
        "search": (state.overlay == Some(Overlay::Search)).then(|| json!({
            "query": state.search_query,
            "loading": state.search_loading,
            "hit_count": state.search_results.len(),
            "hits": state.search_results.iter().map(|hit| json!({
                "channel": hit.channel_id,
                "channel_name": hit.channel_name,
                "ts": hit.ts,
                "author": hit.author,
                "text": hit.text,
            })).collect::<Vec<_>>(),
        })),
        "workspaces": state.workspaces.iter().map(|workspace| json!({
            "team_id": workspace.id,
            "name": workspace.name,
            "channel_count": state.channels.len(),
            "rt_connected": state.core.workspaces.get(&workspace.id).is_some_and(|workspace| matches!(workspace.rt, super_platinum_core::state::RealtimeStatus::Connected(_))),
        })).collect::<Vec<_>>(),
        // The rendered slice, so a blank transcript can be told apart from an
        // empty one without a screenshot.
        "timeline": json!({
            "messages": state.messages.len(),
            "start": state.timeline_start,
            "end": state.timeline_end,
            "stick_to_bottom": state.stick_to_bottom,
            "pending_scroll_to": state.core.pending_scroll_to.as_ref().map(|(channel, _)| channel.clone()),
        }),
        "recent_messages": state.messages.iter().rev().take(12).rev().map(|message| json!({
            "ts": message.id,
            "author": message.author,
            "text": message_preview_text(message),
            // Reactions ride along so a live check can tell "the pill is not on
            // screen" from "the reaction never reached the state".
            "reactions": message.reactions.iter().map(|reaction| format!(
                ":{}:x{}{}",
                reaction.name,
                reaction.count,
                if reaction.own { " (own)" } else { "" },
            )).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "toasts": state.toast.iter().map(|toast| toast.text.clone()).collect::<Vec<_>>(),
        "connection": state.connection.status.key(),
        "allow_destructive": allow_destructive(),
        "performance": {
            "channel_switch_ms": state.performance.channel_switch_ms,
            "realtime_insert_ms": state.performance.realtime_insert_ms,
            "scroll_frame_ms": state.performance.scroll_frame_ms,
        },
        "agent_socket": agent_endpoint(),
    })
}

fn agent_endpoint() -> String {
    #[cfg(unix)]
    {
        socket_path().display().to_string()
    }
    #[cfg(target_os = "windows")]
    {
        std::fs::read_to_string(std::env::temp_dir().join("super-platinum-agent.sock.path"))
            .unwrap_or_else(|_| "tcp://127.0.0.1:0".into())
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        String::new()
    }
}

fn main_view_label(view: MainView) -> &'static str {
    match view {
        MainView::Home => "home",
        MainView::Unreads => "unreads",
        MainView::Threads => "threads",
        MainView::Activity => "activity",
        MainView::Dms => "dms",
    }
}

fn message_preview_text(message: &crate::state::MessageVm) -> String {
    let text = message
        .body
        .iter()
        .map(crate::state::RichNode::plain_text)
        .collect::<Vec<_>>()
        .join(" ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn help_data() -> Value {
    json!({
        "commands": [
            {"cmd": "ping"}, {"cmd": "state"}, {"cmd": "open-palette"},
            {"cmd": "set-query"}, {"cmd": "submit"}, {"cmd": "select-channel"},
            {"cmd": "search"}, {"cmd": "open-settings"}, {"cmd": "screenshot"},
            {"cmd": "open-profile"}, {"cmd": "close-profile"},
            {"cmd": "allow-destructive"}, {"cmd": "send"}
        ]
    })
}
