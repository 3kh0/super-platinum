//! Actual About -> metadata -> Activity interactions against the closed endpoint.
use super::*;
use dioxus::desktop::DesktopContext;
use std::time::Duration;
mod channel;
#[path = "../../entry/fixture/check.rs"]
mod execution;
#[path = "../../entry/fixture/replacement.rs"]
mod execution_replacement;
#[path = "../../entry/fixture/retirement.rs"]
mod execution_retirement;
#[path = "../../entry/fixture/surfaces.rs"]
mod execution_surfaces;
mod export;
mod native;
mod picker;
mod refresh;
mod scenario;
mod users;
use native::{capture, click, input, wait_dom};

async fn readiness(name: &str) -> Result<Value, String> {
    let nonce = std::env::var("SUPER_PLATINUM_WORKFLOW_ACTIVITY_NONCE")
        .map_err(|_| "Missing Activity readiness nonce")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid Activity readiness nonce".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_WORKFLOW_ACTIVITY_READY_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute Activity readiness directory required")?;
    if directory.file_name().and_then(|s| s.to_str())
        != Some(format!("workflow-activity-{nonce}").as_str())
    {
        return Err("Wrong Activity readiness directory".into());
    }
    let expected = json!({"version":1,"fixture":name,"nonce":nonce,"pid":std::process::id()});
    let inspect = |directory: std::path::PathBuf| async move {
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let metadata = std::fs::symlink_metadata(&directory)
                .map_err(|_| "Inspect Activity readiness directory")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Invalid Activity readiness directory".into());
            }
            let path = directory.join("ready.json");
            let metadata = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|_| "Inspect Activity readiness marker")?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
                return Err("Invalid Activity readiness marker".into());
            }
            let bytes = std::fs::read(path).map_err(|_| "Read Activity readiness marker")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Parse Activity readiness marker".into())
        })
        .await
        .map_err(|_| "Workflow readiness worker failed")?
    };
    if inspect(directory.clone()).await?.is_some() {
        return Err("Workflow readiness marker must be fresh".into());
    }
    eprintln!("workflow-activity-waiting: {expected}");
    for _ in 0..300 {
        if let Some(actual) = inspect(directory.clone()).await? {
            return if actual == expected {
                Ok(expected)
            } else {
                Err("Wrong Activity readiness owner".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Workflow readiness timed out".into())
}

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !enabled(&state.peek()) {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(180), async {
            if matches!(
                case().as_str(),
                "execution-thread-replacement" | "execution-pin-removal"
            ) {
                execution_replacement::run(state, &desktop, &name).await
            } else if matches!(
                case().as_str(),
                "execution-thread-retirement" | "execution-pins-retirement"
            ) {
                execution_retirement::run(state, &desktop, &name).await
            } else if matches!(
                case().as_str(),
                "execution-thread" | "execution-pins" | "execution-focused"
            ) {
                execution_surfaces::run(state, &desktop, &name).await
            } else if case().starts_with("execution") {
                execution::run(state, &desktop, &name).await
            } else if case().starts_with("export") {
                export::run(state, &desktop, &name).await
            } else if matches!(case().as_str(), "user-refresh" | "user-deactivated") {
                refresh::run(state, &desktop, &name).await
            } else if case().starts_with("user-") {
                users::run(state, &desktop, &name).await
            } else if case() == "channel" {
                channel::run(state, &desktop, &name).await
            } else if case() == "picker" {
                picker::run(state, &desktop, &name).await
            } else {
                scenario::run(state, &desktop, &name).await
            }
        })
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":error,"lastModel":snapshot(&state.peek()),"channelModel":super::channel::snapshot(&state.peek()),"userPolicyModel":super::users::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":"Activity native check timed out","lastModel":snapshot(&state.peek())})
            }
        };
        eprintln!("workflow-activity-check: {result}");
    });
}
async fn wait(
    state: Signal<ShellState>,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..100 {
        let v = snapshot(&state.peek());
        if predicate(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("{label}: {}", snapshot(&state.peek())))
}
fn exact(v: &Value) -> bool {
    v["metadata"]["title"] == TITLE
        && v["metadata"]["description"] == DESCRIPTION
        && v["metadata"]["dirty"] == true
}
