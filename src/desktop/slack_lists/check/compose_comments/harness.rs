//! Exact-gated List comments runner ownership and native capture handshake.
use super::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
fn enabled() -> bool {
    fixture::compose::check_enabled()
}
fn configuration() -> Result<(PathBuf, Value), String> {
    if !enabled() {
        return Err("List comments fixture opt-in required".into());
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let nonce =
        std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_NONCE").map_err(|_| "Missing nonce")?;
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_READY_DIR")
            .ok_or("Missing private ready directory")?,
    );
    let captures = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CAPTURE_DIR")
            .ok_or("Missing capture directory")?,
    );
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !directory.is_absolute()
        || directory.file_name().and_then(|s| s.to_str())
            != Some(format!("slack-list-comments-compose-{nonce}").as_str())
        || captures != directory
    {
        return Err("Invalid private runner configuration".into());
    }
    Ok((
        directory,
        json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":fixture}),
    ))
}
fn no_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.file_type().is_symlink()
}
fn read_marker(path: &Path) -> Result<Value, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "Runner marker unavailable")?;
    if !meta.is_file() || !no_link(&meta) || meta.len() > 1024 {
        return Err("Invalid runner marker".into());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|_| "Read runner marker")?)
        .map_err(|_| "Invalid runner marker JSON".into())
}
fn validate_directory(directory: &Path, expected: &Value) -> Result<(), String> {
    for path in directory.ancestors() {
        let meta =
            std::fs::symlink_metadata(path).map_err(|_| "Missing private directory ancestor")?;
        if !meta.is_dir() || !no_link(&meta) {
            return Err("Linked private directory refused".into());
        }
    }
    if read_marker(&directory.join("owner.json"))?
        != json!({"version":1,"nonce":expected["nonce"],"fixture":expected["fixture"]})
    {
        return Err("Wrong private directory owner".into());
    }
    Ok(())
}
pub(crate) async fn wait_runner(state: Signal<ShellState>) -> Result<Value, String> {
    if (state.read().core.transport.is_some() || state.read().core.session.is_some())
        && !fixture::completion::enabled(&state.peek())
        && !fixture::emoji_combined::context_enabled(&state.peek())
    {
        return Err("Disconnected fixture required".into());
    }
    let (directory, expected) = configuration()?;
    let root = directory.clone();
    let owner = expected.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        validate_directory(&root, &owner)?;
        match std::fs::symlink_metadata(root.join("ready.json")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Runner marker must not predate waiting receipt".into()),
        }
    })
    .await
    .map_err(|_| "Runner ownership task failed")??;
    eprintln!("slack-list-comments-compose-waiting: {expected}");
    for _ in 0..300 {
        let root = directory.clone();
        let owner = expected.clone();
        let ready = tokio::task::spawn_blocking(move || -> Result<bool, String> {
            validate_directory(&root, &owner)?;
            match std::fs::symlink_metadata(root.join("ready.json")) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err("Runner marker unavailable".into()),
                Ok(_) => {
                    if read_marker(&root.join("ready.json"))? == owner {
                        Ok(true)
                    } else {
                        Err("Runner readiness owner mismatch".into())
                    }
                }
            }
        })
        .await
        .map_err(|_| "Runner readiness task failed")??;
        if ready {
            return Ok(expected);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Runner readiness timed out".into())
}
pub(crate) async fn capture(
    d: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<Value, String> {
    if stage.is_empty() || !stage.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
        return Err("Invalid capture stage".into());
    }
    let (directory, owner) = configuration()?;
    eprintln!(
        "slack-list-comments-compose-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    checked(d,"(()=>{window.__listHydrationPaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__listHydrationPaint=true));return {ok:true};})()").await?;
    wait_dom(d, "({ok:window.__listHydrationPaint===true})").await?;
    let path = directory.join(format!(
        "{}-{stage}.png",
        owner["fixture"].as_str().unwrap()
    ));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    eprintln!(
        "slack-list-comments-compose-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(json!(path.to_string_lossy()))
}

/// The runner owns a real SQLite transaction; a matching marker acknowledges its state.
pub(super) async fn storage_control(
    state: Signal<ShellState>,
    readiness: &Value,
    sequence: u64,
    action: &str,
) -> Result<Value, String> {
    let limit = match std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref() {
        Ok("storage-recovery") => 6,
        Ok("own-edit-storage") => 8,
        Ok("own-edit-plain-storage" | "own-edit-plain-noop-storage")
            if fixture::plain::storage() =>
        {
            2
        }
        Ok("own-edit-noop-storage") if fixture::own_edit::noop_named() => 2,
        Ok("own-delete-storage") => 4,
        Ok("file-navigation-custody") => 4,
        _ => 0,
    };
    if limit == 0
        || !fixture::compose::enabled(&state.peek())
        || !(1..=limit).contains(&sequence)
        || action != if sequence % 2 == 1 { "hold" } else { "release" }
    {
        return Err("Storage control requires the exact closed recovery case".into());
    }
    let (directory, owner) = configuration()?;
    if &owner != readiness {
        return Err("Storage original readiness owner changed".into());
    }
    let account = state
        .peek()
        .core
        .active_account
        .clone()
        .ok_or("Storage account missing")?;
    let mut request = owner.clone();
    request["sequence"] = json!(sequence);
    request["action"] = json!(action);
    let marker = directory.join(format!("storage-{sequence}.json"));
    let root = directory.clone();
    let expected = owner.clone();
    let path = marker.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        validate_directory(&root, &expected)?;
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Storage acknowledgment predates original request".into()),
        }
    })
    .await
    .map_err(|_| "Storage marker check task failed")??;
    eprintln!("slack-list-comments-compose-storage: {request}");
    for _ in 0..200 {
        let root = directory.clone();
        let expected = owner.clone();
        let path = marker.clone();
        let ack = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            validate_directory(&root, &expected)?;
            let meta = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("Storage acknowledgment unavailable".into()),
                Ok(meta) => meta,
            };
            if !meta.is_file() || !no_link(&meta) || meta.len() > 1024 * 1024 {
                return Err("Invalid storage acknowledgment file".into());
            }
            serde_json::from_slice(
                &std::fs::read(path).map_err(|_| "Storage acknowledgment read failed")?,
            )
            .map(Some)
            .map_err(|_| "Invalid storage acknowledgment JSON".into())
        })
        .await
        .map_err(|_| "Storage acknowledgment task failed")??;
        if let Some(ack) = ack {
            if ["version", "nonce", "pid", "fixture", "sequence", "action"]
                .iter()
                .any(|key| ack[*key] != request[*key])
                || ack["ok"] != true
                || ack["account"] != json!(account)
                || ack["inventories"]
                    .as_array()
                    .is_none_or(|rows| rows.is_empty() || rows.len() > 128)
                || (action == "release" && ack["unchanged"] != true)
                || state.peek().core.active_account.as_ref() != Some(&account)
            {
                return Err(
                    "Storage acknowledgment does not match original request/account".into(),
                );
            }
            return Ok(ack);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Storage acknowledgment timed out".into())
}
