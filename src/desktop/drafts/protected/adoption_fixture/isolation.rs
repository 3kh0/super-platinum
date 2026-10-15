//! Fresh parent ownership plus a separate core-owned isolated worker child.
use super::*;
/// Read-only main-thread startup gate, before native setup or worker creation.
/// Seed still repeats configuration/path checks before Isolated::create.
pub(crate) fn preflight() -> Result<(), String> {
    let (root, _, nonce, _) = configuration()?;
    validate_directory(&root, &nonce)?;
    match std::fs::symlink_metadata(root.join("worker")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Isolated worker destination must be fresh".into()),
    }
}
pub(super) fn configuration() -> Result<(PathBuf, PathBuf, String, String), String> {
    if !enabled() {
        return Err("Adoption fixture opt-in missing".into());
    }
    let root = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_PROTECTED_ADOPTION_ROOT").ok_or("Missing private root")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_PROTECTED_ADOPTION_NONCE").map_err(|_| "Missing nonce")?;
    let scenario = std::env::var("SUPER_PLATINUM_PROTECTED_ADOPTION_SCENARIO")
        .map_err(|_| "Missing scenario")?;
    let captures = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_PROTECTED_ADOPTION_CAPTURE_DIR")
            .ok_or("Missing capture directory")?,
    );
    if !root.is_absolute()
        || !captures.is_absolute()
        || root.parent() != Some(captures.as_path())
        || [&root, &captures].into_iter().any(|path| {
            path.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )
            })
        })
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !SCENARIOS.contains(&scenario.as_str())
    {
        return Err("Invalid protected fixture configuration".into());
    }
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture alias")?;
    if captures.file_name().and_then(|v| v.to_str()) != Some(format!("{alias}-{scenario}").as_str())
    {
        return Err("Wrong owned capture directory".into());
    }
    Ok((root, captures, nonce, scenario))
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
        != Some(format!("protected-adoption-{nonce}").as_str())
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
pub(super) async fn capture_path(b: &Binding, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || !name.bytes().all(|v| v.is_ascii_lowercase() || v == b'-') {
        return Err("Invalid capture stage".into());
    }
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture alias")?;
    if !matches!(
        alias.as_str(),
        "protected-adoption" | "protected-adoption-narrow"
    ) {
        return Err("Capture fixture alias changed".into());
    }
    let expected_parent = format!("{alias}-{}", b.scenario);
    let filename = format!("{alias}-{}-exercise-{name}.png", b.scenario);
    let (root, captures, nonce) = (b.root.clone(), b.capture_dir.clone(), b.nonce.clone());
    tokio::task::spawn_blocking(move || {
        validate_directory(&root, &nonce)?;
        if root.parent() != Some(captures.as_path())
            || captures.file_name().and_then(|v| v.to_str()) != Some(expected_parent.as_str())
        {
            return Err("Capture ownership changed".into());
        }
        let path = captures.join(filename);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
            _ => Err("Capture destination must be fresh".into()),
        }
    })
    .await
    .map_err(|_| "Capture ownership task failed")?
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
    eprintln!("protected-adoption-waiting: {expected}");
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
            return Err("Adoption readiness timeout".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
