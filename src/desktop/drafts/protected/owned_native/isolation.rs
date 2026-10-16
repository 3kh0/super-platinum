//! Bound runner directory, never a path supplied to the actual Owned factory.
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
        return Err("Owned fixture opt-in missing".into());
    }
    let root = std::env::var_os("SUPER_PLATINUM_PROTECTED_OWNED_SEND_ROOT")
        .ok_or("Missing owned root")?
        .into();
    let captures = std::env::var_os("SUPER_PLATINUM_PROTECTED_OWNED_SEND_CAPTURE_DIR")
        .ok_or("Missing captures")?
        .into();
    let nonce =
        std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_SEND_NONCE").map_err(|_| "Missing nonce")?;
    let scenario = std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_SEND_SCENARIO")
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
            != Some(format!("protected-owned-send-{}", c.nonce).as_str())
        || c.captures.file_name().and_then(|v| v.to_str())
            != Some(format!("{alias}-{}", c.scenario).as_str())
    {
        return Err("Invalid owned fixture configuration".into());
    }
    Ok(c)
}
fn regular(path: &std::path::Path, directory: bool) -> Result<std::fs::Metadata, String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "Owned path missing")?;
    if m.file_type().is_symlink() || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err("Owned path type refused".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("Owned reparse path refused".into());
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
        return Err("Oversized owned marker".into());
    }
    let value: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read owner")?)
            .map_err(|_| "Invalid owner JSON")?;
    if value != json!({"version":1,"nonce":c.nonce}) {
        return Err("Wrong owned marker".into());
    }
    Ok(())
}
fn config(b: &Run) -> Config {
    Config {
        root: b.root.clone(),
        captures: b.captures.clone(),
        nonce: b.nonce.clone(),
        scenario: b.scenario.clone(),
    }
}
pub(super) async fn capture_path(b: &Run, stage: &str) -> Result<std::path::PathBuf, String> {
    if !["initial", "input", "cleared", "newer", "result"].contains(&stage) {
        return Err("Unknown capture stage".into());
    }
    let c = config(b);
    let path = b.captures.join(format!("{stage}.png"));
    tokio::task::spawn_blocking(move || {
        validate(&c)?;
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(path),
            _ => Err("Capture destination not fresh".into()),
        }
    })
    .await
    .map_err(|_| "Capture ownership task failed")?
}
pub(super) async fn wait_runner(b: &Run) -> Result<(), String> {
    let expected = json!({"version":1,"nonce":b.nonce,"pid":std::process::id(),"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").ok(),"scenario":b.scenario,"phase":"exercise"});
    let c = config(b);
    tokio::task::spawn_blocking(move || {
        validate(&c)?;
        if c.root.join("ready.json").exists() {
            return Err("Readiness must be fresh".into());
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "Readiness ownership task failed")??;
    eprintln!("protected-owned-send-waiting: {expected}");
    for _ in 0..300 {
        let c = config(b);
        let expected = expected.clone();
        let ready = tokio::task::spawn_blocking(move || {
            validate(&c)?;
            let path = c.root.join("ready.json");
            match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(_) => return Err("Readiness inspection failed".into()),
                Ok(_) => (),
            }
            if regular(&path, false)?.len() > 1024 {
                return Err("Readiness oversized".into());
            }
            let actual: Value =
                serde_json::from_slice(&std::fs::read(path).map_err(|_| "Readiness read failed")?)
                    .map_err(|_| "Readiness JSON invalid")?;
            if actual != expected {
                return Err("Wrong readiness process identity".into());
            }
            Ok::<_, String>(true)
        })
        .await
        .map_err(|_| "Readiness task failed")??;
        if ready {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("Owned readiness timeout".into())
}
