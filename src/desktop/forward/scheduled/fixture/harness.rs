//! Exact-gated Scheduled forward runner ownership and native capture handshake.
use super::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
fn enabled() -> bool {
    super::enabled()
}
pub(super) fn configuration() -> Result<(PathBuf, Value), String> {
    if !enabled() {
        return Err("Scheduled forward fixture opt-in required".into());
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let nonce =
        std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_NONCE").map_err(|_| "Missing nonce")?;
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_SCHEDULED_FORWARD_READY_DIR")
            .ok_or("Missing private ready directory")?,
    );
    let captures = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_SCHEDULED_FORWARD_CAPTURE_DIR")
            .ok_or("Missing capture directory")?,
    );
    if !matches!(
        std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_PHASE").as_deref(),
        Ok("exercise" | "restart")
    ) || !SCENARIOS.contains(
        &std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_SCENARIO")
            .unwrap_or_default()
            .as_str(),
    ) || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !directory.is_absolute()
        || directory.file_name().and_then(|s| s.to_str())
            != Some(format!("scheduled-forward-{nonce}").as_str())
        || captures != directory
    {
        return Err("Invalid private runner configuration".into());
    }
    Ok((
        directory,
        json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":fixture,"scenario":std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_SCENARIO").map_err(|_|"Missing scenario")?,"phase":std::env::var("SUPER_PLATINUM_SCHEDULED_FORWARD_PHASE").map_err(|_|"Missing phase")?}),
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
        != json!({"version":1,"nonce":expected["nonce"],"fixture":expected["fixture"],"scenario":expected["scenario"]})
    {
        return Err("Wrong private directory owner".into());
    }
    Ok(())
}
pub(crate) async fn wait_runner(state: Signal<ShellState>) -> Result<(PathBuf, Value), String> {
    if state.read().core.transport.is_some() || state.read().core.session.is_some() {
        return Err("Disconnected fixture required".into());
    }
    let (directory, expected) = configuration()?;
    let root = directory.clone();
    let owner = expected.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        validate_directory(&root, &owner)?;
        match std::fs::symlink_metadata(root.join(format!(
            "ready-{}.json",
            owner["phase"].as_str().ok_or("Missing phase")?
        ))) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Runner marker must not predate waiting receipt".into()),
        }
    })
    .await
    .map_err(|_| "Runner ownership task failed")??;
    eprintln!("scheduled-forward-waiting: {expected}");
    for _ in 0..300 {
        let root = directory.clone();
        let owner = expected.clone();
        let ready = tokio::task::spawn_blocking(move || -> Result<bool, String> {
            validate_directory(&root, &owner)?;
            match std::fs::symlink_metadata(root.join(format!(
                "ready-{}.json",
                owner["phase"].as_str().ok_or("Missing phase")?
            ))) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err("Runner marker unavailable".into()),
                Ok(_) => {
                    if read_marker(&root.join(format!(
                        "ready-{}.json",
                        owner["phase"].as_str().ok_or("Missing phase")?
                    )))? == owner
                    {
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
            return Ok((directory, expected));
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
        "scheduled-forward-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let mut evaluation = dioxus::document::eval(
        "requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send(true)));await dioxus.recv();",
    );
    tokio::time::timeout(Duration::from_secs(3), evaluation.recv::<bool>())
        .await
        .map_err(|_| "Paint timed out")?
        .map_err(|_| "Paint receipt missing")?;
    let _ = evaluation.send(());
    let path = directory.join(format!(
        "{}-{}-{}-{stage}.png",
        owner["fixture"].as_str().unwrap(),
        owner["scenario"].as_str().unwrap(),
        owner["phase"].as_str().unwrap()
    ));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    eprintln!(
        "scheduled-forward-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(json!(path.to_string_lossy()))
}

pub(super) fn validate_owned(directory: &Path, nonce: &str, scenario: &str) -> Result<(), Error> {
    let fixture =
        std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| server::refused("fixture"))?;
    validate_directory(
        directory,
        &json!({"nonce":nonce,"scenario":scenario,"fixture":fixture}),
    )
    .map_err(|_| server::refused("directory"))?;
    for name in [
        "server.json",
        "server.next",
        "journal.sqlite",
        "journal.sqlite-wal",
        "journal.sqlite-shm",
    ] {
        match std::fs::symlink_metadata(directory.join(name)) {
            Ok(meta) if meta.is_file() && no_link(&meta) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(server::refused("linked_file")),
        }
    }
    Ok(())
}
pub(super) fn load_server(directory: &Path, restart: bool) -> Result<server::Server, String> {
    let file = directory.join("server.json");
    if !restart {
        if file.exists() || directory.join("journal.sqlite").exists() {
            return Err("Exercise requires a fresh private store".into());
        }
        return Ok(server::Server::default());
    }
    let meta = std::fs::symlink_metadata(&file).map_err(|_| "Restart server missing")?;
    if !meta.is_file() || !no_link(&meta) || meta.len() > 2 * 1024 * 1024 {
        return Err("Invalid restart server".into());
    }
    let bytes = std::fs::read(file).map_err(|_| "Restart server unavailable")?;
    let server: server::Server =
        serde_json::from_slice(&bytes).map_err(|_| "Malformed restart server")?;
    if server.rows.len() != 1 || server.ledger.len() > 64 || server.writes > 16 {
        return Err("Unexpected private server bounds".into());
    }
    Ok(server)
}
pub(super) fn save_server(directory: &Path, server: &server::Server) -> Result<(), Error> {
    use std::io::Write;
    let pending = directory.join("server.next");
    let bytes = serde_json::to_vec(server).map_err(|_| server::refused("server_encode"))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(server::refused("server_bound"));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|_| server::refused("server_create"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| server::refused("server_flush"))?;
    drop(file);
    std::fs::rename(pending, directory.join("server.json"))
        .map_err(|_| server::refused("server_commit"))
}
