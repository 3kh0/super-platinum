use super::*;
pub(super) async fn ready() -> Result<(PathBuf, String, String), String> {
    let nonce =
        std::env::var("SUPER_PLATINUM_EMOJI_LIFECYCLE_NONCE").map_err(|_| "Missing nonce")?;
    ensure(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid nonce",
    )?;
    let dir = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_EMOJI_LIFECYCLE_DIR")
            .ok_or("Missing private directory")?,
    );
    ensure(
        dir.is_absolute()
            && dir.file_name().and_then(|s| s.to_str())
                == Some(&format!("emoji-lifecycle-{nonce}")),
        "Invalid private directory",
    )?;
    safe_file(&dir, true)?;
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let expected = json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":alias});
    let path = dir.join("ready.json");
    ensure(!path.exists(), "Readiness must be new")?;
    eprintln!("emoji-lifecycle-waiting: {expected}");
    for _ in 0..300 {
        let p = path.clone();
        let found = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            if !p.exists() {
                return Ok(None);
            }
            safe_file(&p, false)?;
            let bytes = std::fs::read(p).map_err(|_| "Readiness read failed")?;
            ensure(bytes.len() <= 1024, "Oversized readiness")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Malformed readiness".into())
        })
        .await
        .map_err(|_| "Readiness task failed")??;
        if let Some(found) = found {
            ensure(found == expected, "Readiness owner mismatch")?;
            return Ok((dir, nonce, alias));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Readiness timed out".into())
}
fn safe_file(path: &std::path::Path, dir: bool) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "Missing owned artifact")?;
    ensure(
        !meta.file_type().is_symlink() && if dir { meta.is_dir() } else { meta.is_file() },
        "Invalid owned artifact",
    )?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure(
            meta.file_attributes() & 0x400 == 0,
            "Reparse artifact refused",
        )?;
    }
    Ok(())
}
