use std::path::Path;

use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSSetDefaultHandlerForURLScheme(
        scheme: *const std::ffi::c_void,
        bundle: *const std::ffi::c_void,
    ) -> i32;
}

const INFO_PLIST: &[u8] = include_bytes!("../assets/macos/Info.plist");
const APP_ICON: &[u8] = include_bytes!("../assets/icons/snack.icns");

pub fn register_as_slack_handler() -> Result<(), String> {
    // SAFETY: NSString and CFString are toll-free bridged. LaunchServices only borrows both
    // strings for this call.
    let status = unsafe {
        let scheme: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c"slack".as_ptr()];
        let bundle: *mut AnyObject = msg_send![class!(NSString),
            stringWithUTF8String: c"com.echonet.snack".as_ptr()
        ];
        LSSetDefaultHandlerForURLScheme(scheme.cast(), bundle.cast())
    };
    (status == 0)
        .then_some(())
        .ok_or_else(|| format!("could not claim the slack:// URL scheme (OSStatus {status})"))
}

pub fn ensure_app_bundle() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Snack executable: {error}"))?;
    if is_app_bundle_executable(&executable) {
        return mac_usernotifications::check_bundle()
            .map_err(|error| format!("invalid macOS application bundle: {error}"));
    }

    let app = executable
        .parent()
        .ok_or_else(|| "Snack executable has no parent dir".to_owned())?
        .join("Snack.app");
    let macos = app.join("Contents/MacOS");
    let resources = app.join("Contents/Resources");
    let bundled_executable = macos.join("snack");

    if app.exists() {
        std::fs::remove_dir_all(&app)
            .map_err(|error| format!("could not refresh {}: {error}", app.display()))?;
    }
    std::fs::create_dir_all(&macos)
        .and_then(|()| std::fs::create_dir_all(&resources))
        .map_err(|error| format!("could not create {}: {error}", app.display()))?;
    std::fs::copy(&executable, &bundled_executable)
        .map_err(|error| format!("could not copy Snack into its app bundle: {error}"))?;
    std::fs::write(app.join("Contents/Info.plist"), INFO_PLIST)
        .and_then(|()| std::fs::write(resources.join("snack.icns"), APP_ICON))
        .map_err(|error| format!("could not write Snack app resources: {error}"))?;

    let status = std::process::Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .status()
        .map_err(|error| format!("could not sign {}: {error}", app.display()))?;
    if !status.success() {
        return Err(format!(
            "codesign failed for {} with {status}",
            app.display()
        ));
    }

    let status = std::process::Command::new(&bundled_executable)
        .args(std::env::args_os().skip(1))
        .status()
        .map_err(|error| format!("could not launch {}: {error}", app.display()))?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Pointer position inside the window under the cursor, in logical points from
/// that window's top-left corner, together with its content size.
///
/// macOS drag-and-drop carries no coordinates — winit forwards only the dropped
/// path — so a drop has to be placed by asking AppKit where the pointer is.
pub fn cursor_in_window() -> Option<(iced::Point, iced::Size)> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent};

    let mtm = MainThreadMarker::new()?;
    let screen = NSEvent::mouseLocation();
    let app = NSApplication::sharedApplication(mtm);
    for window in app.windows().iter() {
        let frame = window.frame();
        let inside = screen.x >= frame.origin.x
            && screen.y >= frame.origin.y
            && screen.x <= frame.origin.x + frame.size.width
            && screen.y <= frame.origin.y + frame.size.height;
        if !window.isVisible() || !inside {
            continue;
        }
        let Some(view) = window.contentView() else {
            continue;
        };
        let local = view.convertPoint_fromView(window.convertPointFromScreen(screen), None);
        let bounds = view.bounds();
        // AppKit measures from the bottom-left corner; Iced lays out from the top.
        return Some((
            iced::Point::new(local.x as f32, (bounds.size.height - local.y) as f32),
            iced::Size::new(bounds.size.width as f32, bounds.size.height as f32),
        ));
    }
    None
}

fn is_app_bundle_executable(executable: &Path) -> bool {
    executable.parent().is_some_and(|macos| {
        macos.file_name().is_some_and(|name| name == "MacOS")
            && macos
                .parent()
                .and_then(Path::parent)
                .is_some_and(|app| app.extension().is_some_and(|extension| extension == "app"))
    })
}

#[cfg(test)]
mod tests {
    use super::is_app_bundle_executable;
    use std::path::Path;

    #[test]
    fn recognizes_executable_inside_app_bundle() {
        assert!(is_app_bundle_executable(Path::new(
            "/tmp/Snack.app/Contents/MacOS/snack"
        )));
        assert!(!is_app_bundle_executable(Path::new(
            "/tmp/target/debug/snack"
        )));
    }
}
