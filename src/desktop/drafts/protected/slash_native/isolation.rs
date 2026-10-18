//! Bound runner directory, never a path supplied to the actual Slash factory.
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
        return Err("Slash fixture opt-in missing".into());
    }
    let root = std::env::var_os("SUPER_PLATINUM_PROTECTED_OWNED_SLASH_ROOT")
        .ok_or("Missing slash root")?
        .into();
    let captures = std::env::var_os("SUPER_PLATINUM_PROTECTED_OWNED_SLASH_CAPTURE_DIR")
        .ok_or("Missing captures")?
        .into();
    let nonce =
        std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_SLASH_NONCE").map_err(|_| "Missing nonce")?;
    let scenario = std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_SLASH_SCENARIO")
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
            != Some(format!("protected-slash-slash-{}", c.nonce).as_str())
        || c.captures.file_name().and_then(|v| v.to_str())
            != Some(format!("{alias}-{}", c.scenario).as_str())
    {
        return Err("Invalid slash fixture configuration".into());
    }
    Ok(c)
}
fn regular(path: &std::path::Path, directory: bool) -> Result<std::fs::Metadata, String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "Slash path missing")?;
    if m.file_type().is_symlink() || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err("Slash path type refused".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("Slash reparse path refused".into());
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
        return Err("Oversized slash marker".into());
    }
    let value: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read owner")?)
            .map_err(|_| "Invalid owner JSON")?;
    if value != json!({"version":1,"nonce":c.nonce}) {
        return Err("Wrong slash marker".into());
    }
    Ok(())
}
