//! Native acceptance of explicit group Retry and Profile continuation.
use super::group_recovery as recovery;
use super::*;
use crate::slack_links_check::wait_dom as native_wait_dom;
use dioxus::desktop::DesktopContext;
use serde_json::json;
use std::time::Duration;

async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_NONCE")
        .map_err(|_| "Missing profile group recovery readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid profile group recovery readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute profile group recovery readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("profile-group-recovery-{nonce}").as_str())
    {
        return Err("Wrong profile group recovery readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect profile group recovery readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid profile group recovery readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|_| "Inspect profile group recovery readiness marker")?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid profile group recovery readiness marker".into());
            }
            let bytes =
                std::fs::read(path).map_err(|_| "Read profile group recovery readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse profile group recovery readiness marker".into())
        })
        .await
        .map_err(|_| "Profile group recovery readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Profile group recovery readiness marker must be fresh".into());
    }
    eprintln!("profile-group-recovery-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong profile group recovery readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Profile group recovery readiness timed out".into())
}

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !recovery::enabled() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":recovery::case(),"error":error,"model":recovery::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Group recovery timed out","model":recovery::snapshot(&state.peek())})
            }
        };
        eprintln!("profile-group-recovery-check: {result}");
    });
}
mod native;
mod scenario;
use native::*;
use scenario::run;
