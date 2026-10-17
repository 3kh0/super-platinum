use super::*;
use std::{path::Path, time::Duration};

const PREFIX: &str = "SUPER_PLATINUM_CANVAS_CREATION_";
fn env(name: &str) -> Result<String, String> {
    std::env::var(format!("{PREFIX}{name}")).map_err(|_| format!("Missing fixture {name}"))
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
fn marker(path: &Path) -> Result<Value, String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "Missing fixture marker")?;
    if !m.is_file() || !no_link(&m) || m.len() > 2048 {
        return Err("Invalid fixture marker".into());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|_| "Read fixture marker")?)
        .map_err(|_| "Malformed fixture marker".into())
}
fn validate_directory(
    path: &Path,
    nonce: &str,
    fixture: &str,
    scenario: &str,
) -> Result<(), String> {
    if !path.is_absolute()
        || path.file_name().and_then(|s| s.to_str())
            != Some(format!("canvas-creation-{nonce}").as_str())
    {
        return Err("Private fixture directory required".into());
    }
    for p in path.ancestors() {
        let m = std::fs::symlink_metadata(p).map_err(|_| "Missing fixture ancestor")?;
        if !m.is_dir() || !no_link(&m) {
            return Err("Linked fixture ancestor refused".into());
        }
    }
    if marker(&path.join("owner.json"))?
        != json!({"version":1,"nonce":nonce,"fixture":fixture,"scenario":scenario})
    {
        return Err("Wrong fixture directory owner".into());
    }
    Ok(())
}
pub(super) fn same_configuration(b: &Binding) -> bool {
    enabled()
        && env("NONCE").ok().as_deref() == Some(&b.nonce)
        && env("SCENARIO").ok().as_deref() == Some(&b.scenario)
        && env("PHASE").ok().as_deref() == Some(&b.phase)
        && env("DIRECTORY").ok().map(PathBuf::from).as_ref() == Some(&b.directory)
        && std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref() == Some(&b.fixture)
}
fn valid_configuration(nonce: &str, scenario: &str, phase: &str) -> bool {
    nonce.len() == 32
        && nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && matches!(
            scenario,
            "editing" | "known-unsent-conflict" | "late-result" | "departure"
        )
        && matches!(phase, "exercise" | "restart")
}
pub(super) async fn prepare(state: Signal<ShellState>) -> Result<Arc<Binding>, String> {
    if !enabled() {
        return Err("Explicit Canvas fixture capability required".into());
    }
    let nonce = env("NONCE")?;
    let scenario = env("SCENARIO")?;
    let phase = env("PHASE")?;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture name")?;
    if !valid_configuration(&nonce, &scenario, &phase) {
        return Err("Invalid exact fixture configuration".into());
    }
    let directory = PathBuf::from(env("DIRECTORY")?);
    let path = directory.join("fixture.sqlite");
    let b = Arc::new(Binding {
        owner: domain::Owner::new(ACCOUNT, "T1", "U00000001")
            .map_err(|_| "Invalid synthetic owner")?,
        path,
        directory,
        nonce,
        fixture,
        scenario,
        phase,
        epoch: state.peek().core.account_epoch,
        mode: AtomicU8::new(2),
        sources: AtomicU64::new(0),
        requests: Mutex::new(vec![]),
        pending: Mutex::new(None),
    });
    if !b.current(&state.peek()) {
        return Err("Disconnected synthetic owner required".into());
    }
    let c = b.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        validate_directory(&c.directory, &c.nonce, &c.fixture, &c.scenario)?;
        let exists = match std::fs::symlink_metadata(&c.path) {
            Ok(m) => {
                if !m.is_file() || !no_link(&m) {
                    return Err("Linked fixture database refused".into());
                }
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err("Inspect fixture database".into()),
        };
        if exists != (c.phase == "restart") {
            return Err("Wrong fresh/restart database phase".into());
        }
        for suffix in ["-wal", "-shm", "-journal"] {
            let side = c.path.with_file_name(format!("fixture.sqlite{suffix}"));
            if let Ok(m) = std::fs::symlink_metadata(side) {
                if !m.is_file() || !no_link(&m) {
                    return Err("Linked SQLite sidecar refused".into());
                }
            }
        }
        let ready = c.directory.join(format!("ready-{}.json", c.phase));
        if ready.exists() {
            return Err("Readiness marker must be fresh".into());
        }
        Ok(())
    })
    .await
    .map_err(|_| "Fixture ownership worker failed")??;
    let expected = json!({"version":1,"nonce":b.nonce,"pid":std::process::id(),"fixture":b.fixture,"scenario":b.scenario,"phase":b.phase});
    eprintln!("canvas-creation-waiting: {expected}");
    let mut ready = false;
    for _ in 0..300 {
        let c = b.clone();
        let e = expected.clone();
        ready = tokio::task::spawn_blocking(move || -> Result<bool, String> {
            validate_directory(&c.directory, &c.nonce, &c.fixture, &c.scenario)?;
            let p = c.directory.join(format!("ready-{}.json", c.phase));
            match std::fs::symlink_metadata(&p) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err("Inspect ready marker".into()),
                Ok(_) => {
                    if marker(&p)? == e {
                        Ok(true)
                    } else {
                        Err("Wrong ready tuple".into())
                    }
                }
            }
        })
        .await
        .map_err(|_| "Ready worker failed")??;
        if ready {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !ready || !b.current(&state.peek()) {
        return Err("Ready timeout or changed fixture".into());
    }
    BINDING
        .set(b.clone())
        .map_err(|_| "Fixture capability already installed")?;
    Ok(b)
}
pub(super) async fn capture(
    b: &Binding,
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<Value, String> {
    if !stage.bytes().all(|v| v.is_ascii_lowercase() || v == b'-')
        || stage.is_empty()
        || !same_configuration(b)
    {
        return Err("Invalid capture authority".into());
    }
    let path = b
        .directory
        .join(format!("{}-{}-{stage}.png", b.fixture, b.phase));
    let target = path.clone();
    tokio::task::spawn_blocking(move || {
        if std::fs::symlink_metadata(target).is_ok() {
            Err("Capture already exists".to_owned())
        } else {
            Ok(())
        }
    })
    .await
    .map_err(|_| "Capture path check failed")??;
    eprintln!(
        "canvas-creation-stage: {}",
        json!({"phase":b.phase,"stage":stage,"edge":"before"})
    );
    let mut paint =
        document::eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send(true)));");
    tokio::time::timeout(Duration::from_secs(3), paint.recv::<bool>())
        .await
        .map_err(|_| "Paint timeout")?
        .map_err(|_| "Paint receipt")?;
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    eprintln!(
        "canvas-creation-stage: {}",
        json!({"phase":b.phase,"stage":stage,"edge":"after"})
    );
    Ok(json!(path.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_fixture_configuration_refuses_cross_phase_and_path_injection() {
        let nonce = "0123456789abcdef0123456789abcdef";
        assert!(valid_configuration(nonce, "editing", "exercise"));
        assert!(valid_configuration(nonce, "late-result", "restart"));
        assert!(valid_configuration(nonce, "departure", "exercise"));
        assert!(valid_configuration(nonce, "departure", "restart"));
        for bad in [
            "../0123456789abcdef0123456789abc",
            "0123456789ABCDEF0123456789abcdef",
            "",
            "true",
        ] {
            assert!(!valid_configuration(bad, "editing", "exercise"));
        }
        assert!(!valid_configuration(
            nonce,
            "editing/../late-result",
            "exercise"
        ));
        assert!(!valid_configuration(nonce, "editing", "resume"));
    }
}
