//! Exact-gated synthetic multi-external data and runner ownership handshake.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MULTI_EXTERNAL_SELECT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-multi-external-select"
                | "message-multi-external-select-narrow"
                | "app-modal-multi-external-select"
                | "app-modal-multi-external-select-narrow")
        )
}
pub(crate) fn ordinary() -> bool {
    enabled()
        && std::env::var("SUPER_PLATINUM_FIXTURE")
            .is_ok_and(|n| n.starts_with("message-multi-external-select"))
}
pub(crate) fn install(shell: &mut ShellState) {
    super::external_fixture::install(shell);
    let raw = &mut shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages[0]
        .blocks[1]["accessory"];
    let initial = raw
        .as_object_mut()
        .unwrap()
        .remove("initial_option")
        .unwrap();
    raw["type"] = json!("multi_external_select");
    raw["initial_options"] = json!([initial]);
    raw["max_selected_items"] = json!(2);
    super::user_fixture::reproject(shell);
}
pub(crate) fn replace_initial(mut state: Signal<ShellState>) -> Result<(), String> {
    let mut shell = state.write();
    if !ordinary() || shell.core.transport.is_some() || shell.core.session.is_some() {
        return Err("Disconnected fixture required".into());
    }
    let raw = &mut shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages[0]
        .blocks[1]["accessory"];
    raw["initial_options"] = json!([]);
    super::user_fixture::reproject(&mut shell);
    Ok(())
}
fn configuration() -> Result<(PathBuf, Value), String> {
    if !enabled() {
        return Err("Multi external fixture opt-in required".into());
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let nonce =
        std::env::var("SUPER_PLATINUM_MULTI_EXTERNAL_SELECT_NONCE").map_err(|_| "Missing nonce")?;
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_MULTI_EXTERNAL_SELECT_READY_DIR")
            .ok_or("Missing private ready directory")?,
    );
    let captures = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_MULTI_EXTERNAL_SELECT_CAPTURE_DIR")
            .ok_or("Missing capture directory")?,
    );
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !directory.is_absolute()
        || directory.file_name().and_then(|s| s.to_str())
            != Some(format!("multi-external-select-{nonce}").as_str())
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
    eprintln!("multi-external-select-waiting: {expected}");
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
pub(crate) async fn checked(
    d: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    super::user_check::checked(d, script).await
}
pub(crate) async fn click(
    d: &dioxus::desktop::DesktopContext,
    selector: &str,
) -> Result<(), String> {
    super::user_check::click(d, selector).await
}
pub(crate) async fn wait_dom(
    d: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    super::user_check::wait_dom(d, script).await
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
        "multi-external-select-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    checked(d,"(()=>{window.__multiExternalPaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__multiExternalPaint=true));return {ok:true};})()").await?;
    wait_dom(d, "({ok:window.__multiExternalPaint===true})").await?;
    let path = directory.join(format!(
        "{}-{stage}.png",
        owner["fixture"].as_str().unwrap()
    ));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    eprintln!(
        "multi-external-select-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(json!(path.to_string_lossy()))
}
