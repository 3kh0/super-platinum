//! Bound runner directory, never a path supplied to the actual Baseline factory.
use super::*;
#[derive(Clone)]
pub(super) struct Config {
    pub root: std::path::PathBuf,
    pub captures: std::path::PathBuf,
    pub nonce: String,
    pub scenario: String,
}
pub(super) fn configuration() -> Result<Config, String> {
    if !enabled() {
        return Err("Baseline fixture opt-in missing".into());
    }
    let root = std::env::var_os("SUPER_PLATINUM_PROTECTED_OWNED_BASELINE_ROOT")
        .ok_or("Missing baseline root")?
        .into();
    let captures = std::env::var_os("SUPER_PLATINUM_PROTECTED_OWNED_BASELINE_CAPTURE_DIR")
        .ok_or("Missing captures")?
        .into();
    let nonce = std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_BASELINE_NONCE")
        .map_err(|_| "Missing nonce")?;
    let scenario = std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_BASELINE_SCENARIO")
        .map_err(|_| "Missing scenario")?;
    let c = Config {
        root,
        captures,
        nonce,
        scenario,
    };
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing alias")?;
    if !c.root.is_absolute()
        || !c.captures.is_absolute()
        || c.root.parent() != Some(c.captures.as_path())
        || c.root.components().chain(c.captures.components()).any(|v| {
            matches!(
                v,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
        || c.nonce.len() != 32
        || !c
            .nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !SCENARIOS.contains(&c.scenario.as_str())
        || c.root.file_name().and_then(|v| v.to_str())
            != Some(format!("protected-baseline-{}", c.nonce).as_str())
        || c.captures.file_name().and_then(|v| v.to_str())
            != Some(format!("{alias}-{}", c.scenario).as_str())
    {
        return Err("Invalid baseline fixture configuration".into());
    }
    Ok(c)
}
fn regular(path: &std::path::Path, directory: bool) -> Result<std::fs::Metadata, String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "Baseline path missing")?;
    if m.file_type().is_symlink() || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err("Baseline path type refused".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("Baseline reparse path refused".into());
        }
    }
    Ok(m)
}
pub(super) fn validate(c: &Config) -> Result<(), String> {
    for path in c.root.ancestors() {
        regular(path, true)?;
    }
    let path = c.root.join("owner.json");
    if regular(&path, false)?.len() > 1024 {
        return Err("Oversized baseline marker".into());
    }
    let value: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read owner")?)
            .map_err(|_| "Invalid owner JSON")?;
    if value != json!({"version":1,"nonce":c.nonce}) {
        return Err("Wrong baseline marker".into());
    }
    Ok(())
}

/// Initial-only host handoff. Never a per-stage acknowledgment or store proof.
pub(super) async fn wait_ready() -> Result<(), String> {
    let c = configuration()?;
    let checked = c.clone();
    tokio::task::spawn_blocking(move || validate(&checked))
        .await
        .map_err(|_| "Ready validation task lost")??;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let expected = json!({"version":1,"nonce":c.nonce,"pid":std::process::id(),
        "scenario":c.scenario,"phase":"exercise","fixture":fixture});
    eprintln!("protected-owned-baseline-waiting: {}", expected);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);
    loop {
        let checked = c.clone();
        let actual = tokio::task::spawn_blocking(move || {
            validate(&checked)?;
            let path = checked.root.join("ready.json");
            match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(_) => Err("Cannot inspect ready marker".to_owned()),
                Ok(_) => {
                    if regular(&path, false)?.len() > 1024 {
                        return Err("Oversized ready marker".into());
                    }
                    let raw = std::fs::read(path).map_err(|_| "Cannot read ready marker")?;
                    serde_json::from_slice::<Value>(&raw)
                        .map(Some)
                        .map_err(|_| "Invalid ready marker".into())
                }
            }
        })
        .await
        .map_err(|_| "Ready inspection task lost")??;
        if let Some(actual) = actual {
            if actual != expected {
                return Err("Wrong initial host owner".into());
            }
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Initial host readiness timed out".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
