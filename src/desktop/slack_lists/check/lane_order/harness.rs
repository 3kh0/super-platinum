//! Exact-gated List view ordering runner ownership and native capture handshake.
use super::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
fn enabled() -> bool {
    adapter::check_enabled()
}
fn configuration() -> Result<(PathBuf, Value), String> {
    if !enabled() {
        return Err("List view ordering fixture opt-in required".into());
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let nonce =
        std::env::var("SUPER_PLATINUM_LIST_BOARD_LANE_ORDER_NONCE").map_err(|_| "Missing nonce")?;
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_LIST_BOARD_LANE_ORDER_READY_DIR")
            .ok_or("Missing private ready directory")?,
    );
    let captures = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_LIST_BOARD_LANE_ORDER_CAPTURE_DIR")
            .ok_or("Missing capture directory")?,
    );
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !directory.is_absolute()
        || directory.file_name().and_then(|s| s.to_str())
            != Some(format!("slack-list-board-lane-order-{nonce}").as_str())
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
    super_platinum_core::slash_commands::codec::decode(
        &std::fs::read(path).map_err(|_| "Read runner marker")?,
        1024,
    )
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
    if state.read().core.transport.is_some() || state.read().core.session.is_some() {
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
    emit("slack-list-board-lane-order-waiting", expected.clone()).await?;
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
    if !oracle::STAGES.contains(&stage) {
        return Err("Invalid capture stage".into());
    }
    let (directory, owner) = configuration()?;
    emit(
        "slack-list-board-lane-order-stage",
        json!({"stage":stage,"edge":"before"}),
    )
    .await?;
    checked(d,"(()=>{window.__listLaneOrderPaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__listLaneOrderPaint=true));return {ok:true};})()").await?;
    wait_dom(d, "({ok:window.__listLaneOrderPaint===true})").await?;
    let path = directory.join(format!(
        "{}-{stage}.png",
        owner["fixture"].as_str().unwrap()
    ));
    let checked_path = path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        capture_ready(&directory, &owner, &checked_path)
    })
    .await
    .map_err(|_| "Capture ownership task failed")??;
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    emit(
        "slack-list-board-lane-order-stage",
        json!({"stage":stage,"edge":"after"}),
    )
    .await?;
    Ok(json!(path.to_string_lossy()))
}

async fn emit(prefix: &'static str, value: Value) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        std::io::stderr()
            .lock()
            .write_all(format!("{prefix}: {value}\n").as_bytes())
            .map_err(|_| "Lane ordering log write failed".to_owned())
    })
    .await
    .map_err(|_| "Lane ordering log task failed")?
}

fn capture_ready(directory: &Path, owner: &Value, path: &Path) -> Result<(), String> {
    validate_directory(directory, owner)?;
    if path.parent() != Some(directory) || path.extension().and_then(|s| s.to_str()) != Some("png")
    {
        return Err("Capture path escaped owned directory".into());
    }
    if read_marker(&directory.join("ready.json"))? != *owner {
        return Err("Capture readiness owner mismatch".into());
    }
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Capture target already exists or cannot be inspected".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Owned(PathBuf);
    impl Owned {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let root = std::env::temp_dir().join(format!(
                    "lane-order-marker-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&root) {
                    Ok(()) => return Self(root),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create owned test directory: {error}"),
                }
            }
        }
        fn write(&self, name: &str, value: &Value) {
            std::fs::write(self.0.join(name), serde_json::to_vec(value).unwrap()).unwrap();
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            for name in ["owner.json", "ready.json", "initial.png"] {
                let path = self.0.join(name);
                if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
                    let _ = std::fs::remove_dir(path);
                } else {
                    let _ = std::fs::remove_file(path);
                }
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    fn owner() -> Value {
        json!({"version":1,"nonce":"0123456789abcdef0123456789abcdef","pid":123,"fixture":"slack-list-board-lane-order"})
    }
    fn setup(root: &Owned) {
        let expected = owner();
        root.write(
            "owner.json",
            &json!({"version":1,"nonce":expected["nonce"],"fixture":expected["fixture"]}),
        );
        root.write("ready.json", &expected);
    }
    #[test]
    fn capture_refuses_existing_file_directory_or_wrong_owner() {
        let root = Owned::new();
        setup(&root);
        let path = root.0.join("initial.png");
        assert!(capture_ready(&root.0, &owner(), &path).is_ok());
        std::fs::write(&path, b"previous evidence").unwrap();
        assert!(capture_ready(&root.0, &owner(), &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous evidence");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(capture_ready(&root.0, &owner(), &path).is_err());
        std::fs::remove_dir(&path).unwrap();
        let mut wrong = owner();
        wrong["pid"] = json!(124);
        root.write("ready.json", &wrong);
        assert!(capture_ready(&root.0, &owner(), &path).is_err());
        setup(&root);
        root.write(
            "owner.json",
            &json!({"version":1,"nonce":"another","fixture":"slack-list-board-lane-order"}),
        );
        assert!(capture_ready(&root.0, &owner(), &path).is_err());
    }
    #[test]
    fn marker_decoder_refuses_duplicates_oversized_json_and_wrong_json_types() {
        let root = Owned::new();
        setup(&root);
        std::fs::write(root.0.join("ready.json"), br#"{"pid":123,"pid":124}"#).unwrap();
        assert!(read_marker(&root.0.join("ready.json")).is_err());
        std::fs::write(root.0.join("ready.json"), vec![b' '; 1025]).unwrap();
        assert!(read_marker(&root.0.join("ready.json")).is_err());
        let mut wrong = owner();
        wrong["pid"] = json!(true);
        root.write("ready.json", &wrong);
        assert!(capture_ready(&root.0, &owner(), &root.0.join("initial.png")).is_err());
        setup(&root);
        assert!(
            capture_ready(
                &root.0,
                &owner(),
                &root.0.parent().unwrap().join("escape.png")
            )
            .is_err()
        );
    }
}
