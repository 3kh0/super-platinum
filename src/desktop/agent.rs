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
    crate::blocks::check_button_metadata_native(desktop.clone());
    crate::app_modal::check::start(state, desktop.clone());
    crate::self_status::check::start(state, desktop.clone());
    crate::slack_lists::check::start(state, desktop.clone());
    crate::slack_lists::check::delete::start(state, desktop.clone());
    crate::message_buttons::check::start(state, desktop.clone());
    crate::message_buttons::select_check::start(state, desktop.clone());
    crate::message_buttons::date_check::start(state, desktop.clone());
    crate::message_buttons::user_check::start(state, desktop.clone());
    crate::message_buttons::conversation_check::start(state, desktop.clone());
    crate::message_buttons::time_check::start(state, desktop.clone());
    crate::message_buttons::multi_check::start(state, desktop.clone());
    crate::canvas::checklist::check::start(state, desktop.clone());
    crate::canvas::text::check::start(state, desktop.clone());
    crate::message_buttons::radio_check::start(state, desktop.clone());
    crate::slack_links_check::start(state, desktop.clone());
    crate::activity_feed::check::start(state, desktop.clone());
    crate::composer_emoji::check::start(state, desktop.clone());
    crate::channel_mute::check::start(state, desktop.clone());
    crate::messaging::delivery_check::check::start(state, desktop.clone());
    crate::messaging::thread_broadcast_check::start(state, desktop.clone());
    crate::message_vm::broadcast_check::start(state, desktop.clone());
    crate::view::files::check::start(state, desktop.clone());
    // Fixture-only diagnostics bypass the Dioxus eval bridge when investigating
    // an interpreter that never reaches its initial DOM handshake.
    if crate::fixture::is_fixture() && std::env::var_os("SUPER_PLATINUM_UI_DOM_TRACE").is_some() {
        let window = desktop.clone();
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let result = window.webview.evaluate_script_with_callback(
                "JSON.stringify({url:location.href,ready:document.readyState,body:document.body?.innerHTML.slice(0,1500),scripts:[...document.scripts].map(s=>({src:s.src,chars:s.textContent.length})),interpreter:typeof window.interpreter})",
                |value| eprintln!("fixture DOM trace: {value}"),
            );
            if let Err(error) = result {
                eprintln!("fixture DOM trace request failed: {error}");
            }
        });
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
            shell.open_palette();
            AgentResponse::ok(id, json!({ "palette_open": true }))
        }
        AgentCommand::ClosePalette => {
            state.write().dismiss_overlay();
            AgentResponse::ok(id, json!({ "palette_open": false }))
        }
        AgentCommand::SetQuery { query } | AgentCommand::Type { text: query } => {
            {
                let mut shell = state.write();
                shell.set_palette_query(query.clone());
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
        AgentCommand::Submit => {
            if crate::bootstrap::open_palette_entry(*state, None) {
                AgentResponse::ok(id, json!({ "submitted": true }))
            } else {
                AgentResponse::err(id, "palette has no matching entry")
            }
        }
        AgentCommand::SelectEntry { index } => {
            if crate::bootstrap::open_palette_entry(*state, Some(index)) {
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
                shell.replace_search_query(query.clone());
                shell.overlay = Some(Overlay::Search);
            }
            dioxus::prelude::spawn(crate::bootstrap::search(signal));
            AgentResponse::ok(id, json!({ "query": query }))
        }
        AgentCommand::ClearSearch => {
            state.write().dismiss_overlay();
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
        AgentCommand::OpenUsergroup { group } => {
            dioxus::prelude::spawn(crate::usergroups::open(*state, group.clone()));
            AgentResponse::ok(id, json!({ "usergroup": group }))
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
        AgentCommand::HuddleJoin { channel } => {
            let target = {
                let shell = state.read();
                match channel.as_deref() {
                    Some(channel) => shell
                        .channels
                        .iter()
                        .find(|candidate| {
                            candidate.id == channel || candidate.name.eq_ignore_ascii_case(channel)
                        })
                        .map(|candidate| candidate.id.clone()),
                    None => shell.core.active_channel.clone(),
                }
            };
            match target {
                Some(channel) => {
                    dioxus::prelude::spawn(crate::huddle::join(*state, channel.clone()));
                    AgentResponse::ok(id, json!({ "joining": channel }))
                }
                None => AgentResponse::err(id, "no such conversation"),
            }
        }
        AgentCommand::HuddleLeave => {
            crate::huddle::leave(*state);
            AgentResponse::ok(id, json!({ "left": true }))
        }
        AgentCommand::HuddleMute => {
            crate::huddle::toggle_mute(*state);
            let muted = state
                .read()
                .core
                .huddle
                .call
                .as_ref()
                .map(|call| call.muted);
            AgentResponse::ok(id, json!({ "muted": muted }))
        }
        AgentCommand::Toast { text } => {
            state.write().show_toast(text.clone());
            AgentResponse::ok(id, json!({ "toast": text }))
        }
        AgentCommand::NotificationPreview => {
            dioxus::prelude::spawn(crate::notification::show_preview());
            AgentResponse::ok(id, json!({ "notification_preview": true }))
        }
        AgentCommand::MainView { view } => {
            let target = match view.as_str() {
                "home" => MainView::Home,
                "unreads" => MainView::Unreads,
                "threads" => MainView::Threads,
                "activity" => MainView::Activity,
                "dms" => MainView::Dms,
                "later" => MainView::Later,
                "files" => MainView::Files,
                "scheduled" => MainView::Scheduled,
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
                        crate::activity_feed::thread_destination(item),
                    ))
                });
            let Some((key, channel, ts, thread)) = target else {
                return AgentResponse::err(id, format!("no activity item at {index}"));
            };
            let (opened, navigation) = {
                let mut shell = state.write();
                let opened = shell.select_activity_item(
                    key.clone(),
                    Some(&channel),
                    Some(&ts),
                    thread.as_ref().map(|destination| destination.root.as_str()),
                );
                let navigation = opened
                    .then(|| crate::activity_feed::Navigation::capture(&shell, &key))
                    .flatten();
                (opened, navigation)
            };
            if !opened {
                return AgentResponse::err(
                    id,
                    format!("activity channel is not loaded: {channel}"),
                );
            }
            if let Some(navigation) = navigation {
                dioxus::prelude::spawn(navigation.run(*state));
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
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
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
    let palette_matches = if state.overlay == Some(Overlay::Palette) {
        state.palette_matches()
    } else {
        Vec::new()
    };
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
            "updating_vip": crate::profile_vip::busy(state, user_id),
            "opening_dm": state.profile_dm_opening.as_ref() == Some(user_id),
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
    let search = (state.overlay == Some(Overlay::Search)).then(|| {
        json!({
            "query": state.search_query,
            "loading": state.search_pager.loading,
            "sort": state.search_pager.sort.label(),
            "filters_open": state.search_filters.is_some(),
            "opening": state.search_opening.is_some(),
            "filter_error": state.search_filter_error,
            "page": state.search_pager.page,
            "total": state.search_pager.total,
            "has_more": state.search_pager.has_more(),
            "failed": state.search_pager.failed,
            "hit_count": state.search_results.len(),
            "hits": state.search_results.iter().map(|hit| json!({
                "channel": hit.channel_id,
                "channel_name": hit.channel_name,
                "ts": hit.ts,
                "thread_ts": hit.thread_ts,
                "author": hit.author,
                "text": hit.text,
            })).collect::<Vec<_>>(),
        })
    });
    let performance = json!({
        "startup": crate::performance::startup_snapshot(),
        "channel_switch_ms": state.performance.channel_switch_ms,
        "realtime_insert_ms": state.performance.realtime_insert_ms,
        "scroll_frame_ms": state.performance.scroll_frame_ms,
        "thread_scroll_frame_ms": state.performance.thread_scroll_frame_ms,
        "scroll_start_delay_ms": state.performance.scroll_start_delay_ms,
        "thread_scroll_start_delay_ms": state.performance.thread_scroll_start_delay_ms,
        "summaries": {
            "channel_frames": crate::performance::sample_summary(&state.performance.scroll_frame_ms),
            "thread_frames": crate::performance::sample_summary(&state.performance.thread_scroll_frame_ms),
            "channel_start_delays": crate::performance::sample_summary(&state.performance.scroll_start_delay_ms),
            "thread_start_delays": crate::performance::sample_summary(&state.performance.thread_scroll_start_delay_ms),
        },
    });
    let mut snapshot = json!({
        "screen": screen,
        "signed_in": state.signed_in,
        "main_view": main_view_label(state.main_view),
        "channel_tab": format!("{:?}", state.channel_tab),
        "channel_files": state.core.active_channel.as_deref().and_then(|channel|crate::files::visible_for(state,Some(channel))).map(|files|json!({"query":files.query.text,"channel":files.query.channel,"total":files.total,"page":files.page,"pages":files.page_count,"loading":files.loading,"error":files.error,"ids":files.items.iter().filter_map(|f|f.id.as_ref()).collect::<Vec<_>>()})),
        "channel_links": state.core.active_channel.as_deref().and_then(|channel|crate::channel_resources::visible_links(state,channel)).map(|links|json!({"query":links.query,"total":links.total,"page":links.page,"pages":links.page_count,"loading":links.loading,"error":links.error,"timestamps":links.items.iter().map(|l|&l.timestamp).collect::<Vec<_>>()})),
        "files": crate::files::visible_state(state).map(|files| json!({
            "query": files.query.text,
            "kind": files.query.kind.label(),
            "owner": files.query.owner.label(),
            "sort": files.query.sort.label(),
            "loading": files.loading,
            "loaded": files.loaded,
            "error": files.error,
            "page": files.page,
            "pages": files.page_count,
            "total": files.total,
            "items": files.items.iter().map(|file|json!({"id":file.id,"title":super_platinum_core::state::file_title(file)})).collect::<Vec<_>>(),
        })),
        "active_team": state.workspaces.get(state.active_workspace).map(|workspace| &workspace.id),
        "active_channel": active_channel.map(|channel| &channel.id),
        "read_state": state.core.active_team.as_ref().and_then(|team|state.core.workspaces.get(team)).and_then(|workspace|state.core.active_channel.as_ref().map(|channel|json!({
            "focused":crate::read_visibility::focused(),
            "reading_latest":state.reading_latest(crate::read_visibility::focused()),
            "manual_unread":crate::unread::held(state),
            "changing":crate::unread::busy(state, channel),
            "last_read":workspace.read_cursor(channel),
            "unread_count":workspace.messages.get(channel).map(|messages|messages.unread_count),
            "mention_count":workspace.messages.get(channel).map(|messages|messages.mention_count),
        }))),
        "active_channel_name": active_channel.map(|channel| &channel.name),
        "thread_open": state.thread_root.is_some(),
        "active_thread": state.thread_root,
        "thread_timeline": json!({
            "messages":state.thread_messages.len(),
            "compact":state.thread_messages.iter().filter(|message|message.compact && !message.pending).count(),
            "pending":state.thread_messages.iter().filter(|message|message.pending).count(),
            "target":state.thread_target,
            "at_bottom":state.thread_at_bottom,
            "loading":state.thread_loading,
            "failed":state.thread_error.is_some(),
            "earlier":state.thread_history.earlier.is_some(),
            "newer":state.thread_history.newer.is_some(),
            "page_loading":state.thread_history.loading.is_some(),
            "page_failed":state.thread_history.failed.is_some(),
        }),
        "profile": profile,
        "usergroup": state.group_panel.as_ref().map(|panel| json!({
            "id": panel.id, "loading": panel.loading, "error": panel.error,
            "members": state.core.active_team.as_ref().and_then(|team| state.core.workspaces.get(team)).and_then(|w|w.usergroups.get(&panel.id)).filter(|g|g.members_loaded).map(|g|g.users.len()),
        })),
        "profile_hover": state.profile_hover.as_ref().map(|hover| &hover.user_id),
        "palette_open": state.overlay == Some(Overlay::Palette),
        "palette": (state.overlay == Some(Overlay::Palette)).then(|| json!({
            "query": state.palette_query,
            "selected": state.palette_selected,
            "entry_count": palette_matches.len(),
            "opening": state.palette_opening.is_some(),
            "entries": palette_matches.iter().enumerate().map(|(selection, entry)| json!({
                "index": selection,
                "label": entry.row.name,
                "target": match &entry.target {
                    super_platinum_core::palette::PaletteTarget::Channel(id) => json!({"kind":"channel","id":id}),
                    super_platinum_core::palette::PaletteTarget::User {user,dm} => json!({"kind":"user","id":user,"channel":dm}),
                },
                "selected": selection == state.palette_selected,
            })).collect::<Vec<_>>()
        })),
        "self_menu_open": state.overlay == Some(Overlay::SelfMenu),
        "accounts_open": state.overlay == Some(Overlay::Accounts),
        "settings_open": state.overlay == Some(Overlay::Settings),
        "settings_section": settings_section,
        "storage": storage,
        "search_input": state.search_query,
        "search": search,
        "workspaces": state.workspaces.iter().map(|workspace| json!({
            "team_id": workspace.id,
            "name": workspace.name,
            "channel_count": state.channels.len(),
            "rt_connected": state.core.workspaces.get(&workspace.id).is_some_and(|workspace| matches!(workspace.rt, super_platinum_core::state::RealtimeStatus::Connected(_))),
        })).collect::<Vec<_>>(),
        // The sidebar as laid out: section order, collapse state, and rows.
        "notifications": state.core.active_team.as_ref().and_then(|team| state.core.workspaces.get(team)).map(|workspace| json!({
            "preferences_loaded": workspace.notifications.loaded,
            "active_channel_mute": state.core.active_channel.as_ref().map(|channel| json!({
                "muted": workspace.notifications.channels.get(channel).and_then(|prefs| prefs.get("muted")),
                "temporary_expiry": workspace.notifications.temporary_mute_expiration(channel),
                "effective_muted": workspace.notifications.muted(channel, super_platinum_core::state::now_secs()),
            })),
            "channel_overrides": workspace.notifications.channels.len(),
            "global_desktop_push_enabled": workspace.notifications.global.get("global_desktop_push_enabled"),
            "threads_everything": workspace.notifications.global.get("threads_everything"),
            "no_text_in_notifications": workspace.notifications.global.get("no_text_in_notifications"),
            "desktop_sound": workspace.notifications.global.get("desktop_sound"),
            "priority_desktop_sound": workspace.notifications.global.get("priority_desktop_sound"),
            "mute_sounds": workspace.notifications.user.get("mute_sounds"),
        })),
        "sidebar": state.sidebar_sections.iter().map(|section| json!({
            "id": section.id,
            "title": section.title,
            "collapsed": section.collapsed,
            "rows": section.channel_indices.iter().filter_map(|index| state.channels.get(*index)).map(|channel| json!({
                "id": channel.id,
                "name": channel.name,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        // The rendered slice, so a blank transcript can be told apart from an
        // empty one without a screenshot.
        "timeline": json!({
            "messages": state.channel_rows.len(),
            "projected_messages": state.messages.len(),
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
        "huddle": huddle_snapshot(&state.core.huddle),
        "huddle_view": {
            "visible": state.huddle_ui.visible,
            "expanded": state.huddle_ui.expanded,
            "settings": state.huddle_ui.settings,
            "rect": state.huddle_ui.rect,
            "screen_supported": state.huddle_ui.devices.screen_supported,
            "output_supported": state.huddle_ui.devices.output_supported,
        },
        "toasts": state.toast.iter().map(|toast| toast.text.clone()).collect::<Vec<_>>(),
        "connection": state.connection.status.key(),
        "allow_destructive": allow_destructive(),
        "performance": performance,
        "agent_socket": agent_endpoint(),
    });
    snapshot["recent_messages_scope"] = json!("rendered_window");
    snapshot["thread_read_state"] = crate::thread_unread::key(state)
        .map(|key| {
            json!({
                "last_read":state.core.thread_cursor(&key),
                "unread_count":state.core.threads.get(&key).map(|bag|bag.unread_count),
                "manual_unread":crate::thread_unread::held(state),
                "changing":crate::thread_unread::busy(state),
                "divider":state.thread_messages.iter().find(|m|m.show_unread_divider).map(|m|&m.ts),
            })
        })
        .unwrap_or(Value::Null);
    snapshot["activity_feed"] = json!({
        "items": state.core.activity.items.len(),
        "unread": state.core.activity.unread_count(),
        "loading": state.core.activity.loading,
        "loaded": state.core.activity.loaded,
        "failed": state.core.activity.failed,
        "has_more": state.core.activity.next_cursor.is_some(),
        "sequence": state.core.activity.load_seq,
        "hydrated": state.core.activity.hydrated.len()
    });
    snapshot["thread_subscription"] = json!({"active":crate::thread_subscription::active(state),"pending":crate::thread_subscription::busy(state)});
    snapshot["thread_feed"] = json!({"items":state.core.threads_view.items.len(),"loading":state.core.threads_view.loading,"loaded":state.core.threads_view.loaded,"has_more":state.core.threads_view.has_more,"failed":state.core.threads_view.failed,"dirty":state.core.threads_view.dirty,"next_cursor":state.core.threads_view.next_cursor,"active_channel_items":state.core.threads_view.items.iter().filter(|i| i.channel()==state.core.active_channel.as_ref()).map(|i|json!({"root":i.root_ts(),"replies":i.root_msg.reply_count,"unread":i.unread_replies.len()})).collect::<Vec<_>>()});
    snapshot["thread_totals"] = state.core.active_team.as_ref().and_then(|team|state.core.workspaces.get(team)).map(|w|json!({
        "has_unreads":w.thread_counts.snapshot.as_ref().map(|counts|counts.has_unreads),
        "mention_count":w.thread_counts.snapshot.as_ref().map(|counts|counts.mention_count),
        "vip_count":w.thread_counts.snapshot.as_ref().map(|counts|counts.vip_count),
        "channel_unreads":w.thread_counts.snapshot.as_ref().and_then(|counts|state.core.active_channel.as_ref().map(|channel|counts.unread_count_by_channel.get(channel).copied().unwrap_or(0))),
    })).unwrap_or(Value::Null);
    snapshot
}

/// The call as the shell sees it. Never includes Chime credentials: the join
/// token only ever travels from `rooms.join` straight into the WebView.
fn huddle_snapshot(huddle: &super_platinum_core::huddle::HuddleState) -> Value {
    json!({
        "call": huddle.call.as_ref().map(|call| json!({
            "team": call.team,
            "channel": call.channel,
            "phase": format!("{:?}", call.phase).to_lowercase(),
            "room": call.room,
            "muted": call.muted,
            "camera_on": call.camera_on,
            "screen_sharing": call.screen_sharing,
            "camera_pending": call.camera_pending,
            "share_pending": call.share_pending,
            "tiles": call.tiles.iter().map(|tile| json!({"id": tile.id, "user": tile.user, "local": tile.local, "content": tile.content})).collect::<Vec<_>>(),
            "participants": call.participants.iter().map(|row| json!({
                "user": row.user,
                "speaking": row.speaking,
                "muted": row.muted,
            })).collect::<Vec<_>>(),
        })),
        "invites": huddle.invites.iter().map(|pending| json!({
            "team": pending.team,
            "channel": pending.invite.channel_id,
            "room": pending.invite.call_id,
            "sender": pending.invite.sender_user_id,
        })).collect::<Vec<_>>(),
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
        MainView::Later => "later",
        MainView::Files => "files",
        MainView::Scheduled => "scheduled",
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
            {"cmd": "notification-preview"},
            {"cmd": "open-profile"}, {"cmd": "close-profile"}, {"cmd": "open-usergroup"},
            {"cmd": "allow-destructive"}, {"cmd": "send"},
            {"cmd": "huddle-join"}, {"cmd": "huddle-leave"}, {"cmd": "huddle-mute"}
        ]
    })
}
