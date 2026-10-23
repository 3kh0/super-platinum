//! Fixture-only barrier: no DOM evaluation before the runner normalizes its window.
use super::*;
use std::{io::Read, path::Path};

fn plain(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !metadata.file_type().is_symlink()
}
fn marker(directory: &Path, name: &str) -> Result<Option<Value>, String> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|_| "Missing ready directory")?;
    if !directory.is_absolute() || !metadata.is_dir() || !plain(&metadata) {
        return Err("Unsafe ready directory".into());
    }
    let path = directory.join(name);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Inspect ready marker".into()),
    };
    if !metadata.is_file() || !plain(&metadata) || metadata.len() > 2048 {
        return Err("Unsafe ready marker".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "Open ready marker")?
        .take(2049)
        .read_to_end(&mut bytes)
        .map_err(|_| "Read ready marker")?;
    if bytes.len() > 2048 {
        return Err("Ready marker too large".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "Invalid ready JSON".into())
}
fn isolated(state: Signal<ShellState>) -> bool {
    data::context_enabled(&state.peek())
}
pub(super) async fn wait_runner(state: Signal<ShellState>) -> Result<(), String> {
    if !isolated(state) {
        return Err("Disconnected reader fixture required".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Private ready directory required")?;
    let nonce =
        std::env::var("SUPER_PLATINUM_CANVAS_READER_NONCE").map_err(|_| "Reader nonce required")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid reader nonce".into());
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Fixture required")?;
    let owner = json!({"version":1,"nonce":nonce,"fixture":fixture});
    let expected = json!({"version":1,"nonce":nonce,"fixture":fixture,"pid":std::process::id()});
    let initial = directory.clone();
    let actual = tokio::task::spawn_blocking(move || -> Result<_, String> {
        if marker(&initial, "reader-ready.json")?.is_some() {
            return Err("Ready marker must be fresh".into());
        }
        marker(&initial, "reader-owner.json")?.ok_or_else(|| "Ready owner missing".into())
    })
    .await
    .map_err(|_| "Ready worker failed")??;
    if actual != owner {
        return Err("Ready owner differs".into());
    }
    eprintln!("canvas-comments-waiting: {expected}");
    for _ in 0..600 {
        let directory = directory.clone();
        let owner = owner.clone();
        let value = tokio::task::spawn_blocking(move || -> Result<_, String> {
            if marker(&directory, "reader-owner.json")?.as_ref() != Some(&owner) {
                return Err("Ready owner changed".into());
            }
            marker(&directory, "reader-ready.json")
        })
        .await
        .map_err(|_| "Ready worker failed")??;
        if let Some(value) = value {
            if value != expected || !isolated(state) {
                return Err("Ready tuple or fixture changed".into());
            }
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Runner normalization readiness timed out".into())
}
