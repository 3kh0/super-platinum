//! Private ownership and bounded runner readiness, independent of process TEMP.
use super::*;
fn ready_tuple(binding: &FixtureBinding) -> Value {
    json!({"version":1,"nonce":binding.nonce,"pid":std::process::id(),
        "scenario":binding.scenario,"phase":binding.phase})
}
fn admits_ready(actual: &Value, expected: &Value) -> bool {
    actual == expected
}
pub(crate) async fn wait_runner(binding: &FixtureBinding) -> Result<(), String> {
    let path = binding
        .path
        .parent()
        .ok_or("Missing private directory")?
        .join(format!("ready-{}.json", binding.phase));
    let expected = ready_tuple(binding);
    let nonce = binding.nonce.clone();
    let database = binding.path.clone();
    // Check absence before advertising readiness; an old marker cannot release
    // a new process, even if a caller reused an otherwise valid directory.
    let check = path.clone();
    tokio::task::spawn_blocking(move || {
        validate_directory(&database, &nonce)?;
        match std::fs::symlink_metadata(check) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Runner readiness marker must be new".to_owned()),
        }
    })
    .await
    .map_err(|_| "Readiness admission task failed")??;
    eprintln!("managed-draft-send-waiting: {expected}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let check = path.clone();
        let expected = expected.clone();
        let ready = tokio::task::spawn_blocking(move || -> Result<bool, String> {
            let metadata = match std::fs::symlink_metadata(&check) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(_) => return Err("Cannot inspect runner readiness marker".into()),
                Ok(metadata) => metadata,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 {
                return Err("Invalid runner readiness marker".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("Reparse readiness marker refused".into());
                }
            }
            let bytes = std::fs::read(check).map_err(|_| "Cannot read readiness marker")?;
            let actual: Value =
                serde_json::from_slice(&bytes).map_err(|_| "Malformed readiness marker")?;
            if !admits_ready(&actual, &expected) {
                return Err("Wrong readiness owner or phase".into());
            }
            Ok(true)
        })
        .await
        .map_err(|_| "Readiness read task failed")??;
        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Runner readiness timed out".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
pub(super) fn configuration() -> Result<(PathBuf, String, String, String), String> {
    if !enabled() {
        return Err("Managed Send fixture opt-in missing".into());
    }
    let path = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_MANAGED_DRAFT_SEND_DB")
            .ok_or("Missing isolated database")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_MANAGED_DRAFT_SEND_NONCE").map_err(|_| "Missing nonce")?;
    let scenario = std::env::var("SUPER_PLATINUM_MANAGED_DRAFT_SEND_SCENARIO")
        .map_err(|_| "Missing scenario")?;
    let phase =
        std::env::var("SUPER_PLATINUM_MANAGED_DRAFT_SEND_PHASE").map_err(|_| "Missing phase")?;
    if !path.is_absolute()
        || path.file_name().and_then(|s| s.to_str()) != Some("managed-draft-send.sqlite")
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !SCENARIOS.contains(&scenario.as_str())
        || !matches!(phase.as_str(), "exercise" | "restart")
    {
        return Err("Invalid isolated fixture configuration".into());
    }
    Ok((path, nonce, scenario, phase))
}
pub(super) fn validate_directory(path: &std::path::Path, nonce: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("Missing private directory")?;
    if !parent
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s == format!("managed-draft-send-{nonce}"))
    {
        return Err("Private directory name required".into());
    }
    for part in parent.ancestors() {
        let meta = std::fs::symlink_metadata(part).map_err(|_| "Missing directory ancestor")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Reparse directory refused".into());
            }
        }
        if meta.file_type().is_symlink() {
            return Err("Symlink directory refused".into());
        }
    }
    let marker_path = parent.join("owner.json");
    let metadata =
        std::fs::symlink_metadata(&marker_path).map_err(|_| "Private ownership marker missing")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 {
        return Err("Invalid ownership marker file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Reparse ownership marker refused".into());
        }
    }
    let marker = std::fs::read(marker_path).map_err(|_| "Private ownership marker missing")?;
    if marker.len() > 1024 {
        return Err("Oversized ownership marker".into());
    }
    let value: Value = serde_json::from_slice(&marker).map_err(|_| "Invalid ownership marker")?;
    if value != json!({"version":1,"nonce":nonce}) {
        return Err("Wrong ownership marker".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_requires_exact_nonce_process_case_and_phase_without_extras() {
        let expected = json!({"version":1,"nonce":"a".repeat(32),"pid":42,"scenario":"remote-channel","phase":"exercise"});
        assert!(admits_ready(&expected, &expected));
        for (key, value) in [
            ("nonce", json!("b".repeat(32))),
            ("pid", json!(43)),
            ("scenario", json!("changed-thread")),
            ("phase", json!("restart")),
            ("version", json!(2)),
            ("extra", json!(true)),
        ] {
            let mut bad = expected.clone();
            bad[key] = value;
            assert!(!admits_ready(&bad, &expected));
        }
    }
}
