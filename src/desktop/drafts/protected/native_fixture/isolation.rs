//! Fresh parent ownership plus a separate core-owned isolated worker child.
use super::*;
pub(super) fn configuration() -> Result<(PathBuf, String, String), String> {
    if !enabled() {
        return Err("Protected fixture opt-in missing".into());
    }
    let root = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_PROTECTED_DRAFTS_ROOT").ok_or("Missing private root")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_PROTECTED_DRAFTS_NONCE").map_err(|_| "Missing nonce")?;
    let scenario = std::env::var("SUPER_PLATINUM_PROTECTED_DRAFTS_SCENARIO")
        .map_err(|_| "Missing scenario")?;
    if !root.is_absolute()
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !SCENARIOS.contains(&scenario.as_str())
    {
        return Err("Invalid protected fixture configuration".into());
    }
    Ok((root, nonce, scenario))
}
fn regular(path: &std::path::Path, directory: bool) -> Result<std::fs::Metadata, String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "Fixture owned path missing")?;
    if m.file_type().is_symlink() || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err("Fixture path type refused".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("Fixture reparse path refused".into());
        }
    }
    Ok(m)
}
pub(super) fn validate_directory(root: &std::path::Path, nonce: &str) -> Result<(), String> {
    if root.file_name().and_then(|v| v.to_str())
        != Some(format!("protected-drafts-{nonce}").as_str())
    {
        return Err("Wrong private directory name".into());
    }
    for p in root.ancestors() {
        regular(p, true)?;
    }
    let marker = root.join("owner.json");
    if regular(&marker, false)?.len() > 1024 {
        return Err("Oversized ownership marker".into());
    }
    let value: Value =
        serde_json::from_slice(&std::fs::read(marker).map_err(|_| "Unreadable ownership marker")?)
            .map_err(|_| "Invalid ownership marker")?;
    if value != json!({"version":1,"nonce":nonce}) {
        return Err("Wrong ownership marker".into());
    }
    Ok(())
}
pub(crate) async fn wait_runner(b: &Binding) -> Result<(), String> {
    let expected = json!({"version":1,"nonce":b.nonce,"pid":std::process::id(),"scenario":b.scenario,"phase":"exercise"});
    let (root, nonce) = (b.root.clone(), b.nonce.clone());
    tokio::task::spawn_blocking(move || {
        validate_directory(&root, &nonce)?;
        match std::fs::symlink_metadata(root.join("ready-exercise.json")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Readiness marker must be fresh".to_owned()),
        }
    })
    .await
    .map_err(|_| "Readiness ownership task failed")??;
    eprintln!("protected-drafts-waiting: {expected}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = b.root.join("ready-exercise.json");
        let expected = expected.clone();
        let ready = tokio::task::spawn_blocking(move || -> Result<bool, String> {
            match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(_) => return Err("Cannot inspect readiness".into()),
                Ok(_) => {}
            }
            if regular(&path, false)?.len() > 1024 {
                return Err("Oversized readiness".into());
            }
            let actual: Value =
                serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read readiness")?)
                    .map_err(|_| "Invalid readiness JSON")?;
            if actual != expected {
                return Err("Wrong readiness process/nonce/case".into());
            }
            Ok(true)
        })
        .await
        .map_err(|_| "Readiness read task failed")??;
        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Protected readiness timeout".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
