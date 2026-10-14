//! Native IDs are read on the UI thread; capture processes never block it.
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    time::Instant,
};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) async fn window(
    path: Option<String>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(PathBuf, u32, u32), String> {
    // AppKit/Win32 handles are obtained before awaiting, on the Dioxus thread.
    #[cfg(target_os = "macos")]
    let native_id = {
        use dioxus::desktop::wry::WebViewExtMacOS;
        desktop.webview.ns_window().windowNumber().to_string()
    };
    #[cfg(target_os = "windows")]
    let native_id = {
        use dioxus::desktop::tao::platform::windows::WindowExtWindows;
        desktop.window.hwnd().to_string()
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = desktop;
    let path = resolve_path(path);
    let directory_path = path.clone();
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = directory_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    let deadline = Instant::now() + CAPTURE_TIMEOUT;
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("screencapture");
        command.args(["-x", "-o", "-l", &native_id]).arg(&path);
        run(&mut command, deadline).await?;
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = windows_command(WINDOWS_SCRIPT, &native_id, &path);
        run(&mut command, deadline).await?;
    }
    #[cfg(target_os = "linux")]
    {
        // Preserve the existing active-window fallback on Linux.
        let mut command = Command::new("gnome-screenshot");
        command.args(["-w", "-f"]).arg(&path);
        if run(&mut command, deadline).await.is_err() {
            let mut id_command = Command::new("xdotool");
            id_command.arg("getactivewindow");
            let id = String::from_utf8(run(&mut id_command, deadline).await?)
                .map_err(|e| e.to_string())?;
            let mut fallback = Command::new("import");
            fallback.args(["-window", id.trim()]).arg(&path);
            run(&mut fallback, deadline).await?;
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return Err("native capture is unsupported on this platform".into());
    let read_path = path.clone();
    let (width, height) = tokio::task::spawn_blocking(move || dimensions(&read_path))
        .await
        .map_err(|e| e.to_string())??;
    Ok((path, width, height))
}

// Cancellation transfers the child to an independent Tokio cleanup task.
// kill_on_drop remains a fallback if runtime shutdown aborts that task.
struct ReapOnDrop(Option<Child>);
impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.start_kill();
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
        }
    }
}

async fn run(command: &mut Command, deadline: Instant) -> Result<Vec<u8>, String> {
    if Instant::now() >= deadline {
        return Err("screenshot process timed out".into());
    }
    let child = command
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("start screenshot process: {e}"))?;
    let mut guard = ReapOnDrop(Some(child));
    let child = guard.0.as_mut().expect("owned child");
    let stdout = child.stdout.take().expect("piped stdout");
    let output = async move {
        let mut bytes = Vec::new();
        stdout.take(4096).read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    };
    let result = tokio::time::timeout_at(deadline, async {
        let (status, output) = tokio::join!(child.wait(), output);
        Ok::<_, std::io::Error>((status?, output?))
    })
    .await;
    match result {
        Ok(Ok((status, bytes))) => {
            drop(guard.0.take()); // wait completed and reaped this exact child.
            if status.success() {
                Ok(bytes)
            } else {
                Err(format!("screenshot process exited with {status}"))
            }
        }
        result => {
            let child = guard.0.as_mut().expect("owned child");
            let _ = child.start_kill();
            let _ = child.wait().await;
            drop(guard.0.take());
            match result {
                Err(_) => Err("screenshot process timed out".into()),
                Ok(Err(e)) => Err(format!("wait for screenshot process: {e}")),
                _ => unreachable!(),
            }
        }
    }
}

fn resolve_path(path: Option<String>) -> PathBuf {
    match path {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            PathBuf::from("tmp/agent-ui").join(format!("dioxus-{stamp}.png"))
        }
    }
}
fn dimensions(path: &std::path::Path) -> Result<(u32, u32), String> {
    use std::io::Read;
    if !std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("screenshot is not a regular file".into());
    }
    let mut header = [0_u8; 24];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|e| format!("read screenshot: {e}"))?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return Err("screenshot is not a PNG".into());
    }
    Ok((
        u32::from_be_bytes(header[16..20].try_into().unwrap()),
        u32::from_be_bytes(header[20..24].try_into().unwrap()),
    ))
}

#[cfg(target_os = "windows")]
fn windows_command(script: &str, native_id: &str, path: &std::path::Path) -> Command {
    let mut command = Command::new("powershell.exe");
    // PowerShell -Command treats trailing arguments as source code. Keep the
    // handle and arbitrary output path as child-only data, never shell syntax.
    command
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("SUPER_PLATINUM_CAPTURE_HWND", native_id)
        .env("SUPER_PLATINUM_CAPTURE_PATH", path);
    command
}

#[cfg(target_os = "windows")]
const WINDOWS_SCRIPT: &str = r#"$ErrorActionPreference='Stop'; Add-Type @'
using System; using System.Runtime.InteropServices;
public class SuperPlatinumCapture { [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; } [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r); [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context); }
'@; $priorDpi=[SuperPlatinumCapture]::SetThreadDpiAwarenessContext([IntPtr]::new(-4)); $r=New-Object SuperPlatinumCapture+RECT; if (-not [SuperPlatinumCapture]::GetWindowRect([IntPtr]::new([int64]$env:SUPER_PLATINUM_CAPTURE_HWND),[ref]$r)) { throw 'Cannot locate capture window' }; Add-Type -AssemblyName System.Drawing; $b=New-Object System.Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T); try { $g=[System.Drawing.Graphics]::FromImage($b); try { $g.CopyFromScreen($r.L,$r.T,0,0,$b.Size); $b.Save($env:SUPER_PLATINUM_CAPTURE_PATH,[System.Drawing.Imaging.ImageFormat]::Png) } finally { $g.Dispose() } } finally { $b.Dispose(); [SuperPlatinumCapture]::SetThreadDpiAwarenessContext($priorDpi)|Out-Null }"#;

#[cfg(test)]
mod tests;
