use super::*;
use serde_json::{Value, json};
use std::time::Duration;
const STAGES: [&str; 7] = [
    "initial",
    "busy",
    "canceled",
    "retry-busy",
    "saved",
    "close-busy",
    "closed",
];

async fn file_json(path: PathBuf) -> Option<Value> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes).await.ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
async fn wait_ack(run: &Run, stage: &str) -> Result<(), String> {
    let expected = json!({"version":1,"nonce":run.nonce,"pid":std::process::id(),"fixture":run.alias,"stage":stage});
    for _ in 0..750 {
        if let Some(value) = file_json(run.root.join(format!("ack-{stage}.json"))).await {
            return if value == expected {
                Ok(())
            } else {
                Err("wrong exact capture acknowledgement".into())
            };
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("capture acknowledgement expired".into())
}
// A disappearing entry invalidates the whole sample. Still inspect the rest:
// another I/O error, invalid entry, or over-limit inventory must remain fatal.
fn scan_files(
    root: &std::path::Path,
    mut metadata: impl FnMut(&std::path::Path) -> std::io::Result<std::fs::Metadata>,
) -> Result<Option<Vec<(String, u64)>>, String> {
    let mut rows = Vec::new();
    let mut unsettled = false;
    for (count, entry) in std::fs::read_dir(root)
        .map_err(|_| "download inventory")?
        .enumerate()
    {
        let entry = entry.map_err(|_| "download entry")?;
        if count >= 4 {
            return Err("unexpected download inventory".into());
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or("download filename")?
            .to_owned();
        let meta = match metadata(&entry.path()) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                unsettled = true;
                continue;
            }
            Err(error) => return Err(format!("download metadata ({:?})", error.kind())),
        };
        if !meta.is_file() {
            return Err("unexpected download inventory".into());
        }
        rows.push((name, meta.len()));
    }
    rows.sort();
    Ok((!unsettled).then_some(rows))
}
async fn files(run: &Run) -> Result<Option<Vec<(String, u64)>>, String> {
    let root = run.root.join("downloads");
    tokio::task::spawn_blocking(move || scan_files(&root, |path| std::fs::symlink_metadata(path)))
        .await
        .map_err(|_| "inventory worker")?
}
async fn stage(
    run: &Run,
    name: &str,
    dom: Value,
    state: Signal<ShellState>,
) -> Result<Value, String> {
    let expected_issued = match name {
        "initial" => 0,
        "busy" | "canceled" => 1,
        "retry-busy" | "saved" => 2,
        _ => 3,
    };
    let expected_outcomes: &[(usize, &str)] = match name {
        "initial" | "busy" => &[],
        "canceled" | "retry-busy" => &[(1, "canceled")],
        "saved" | "close-busy" => &[(1, "canceled"), (2, "saved")],
        _ => &[(1, "canceled"), (2, "saved"), (3, "canceled")],
    };
    let busy = matches!(name, "busy" | "retry-busy" | "close-busy");
    let mut accepted = None;
    for _ in 0..250 {
        let Some(rows) = files(run).await? else {
            tokio::time::sleep(Duration::from_millis(40)).await;
            continue;
        };
        let final_file = rows
            .iter()
            .any(|(name, len)| name == "attempt-2.bin" && *len == BYTES);
        let parts = rows
            .iter()
            .filter(|(name, _)| {
                name.starts_with(".super-platinum-download-") && name.ends_with(".part")
            })
            .collect::<Vec<_>>();
        let inventory_ok = if busy {
            parts.len() == 1
                && parts[0].1 >= 65536
                && rows.len() == if name == "close-busy" { 2 } else { 1 }
        } else if matches!(name, "saved" | "closed") {
            final_file && rows.len() == 1
        } else {
            rows.is_empty()
        };
        if inventory_ok
            && run.issued() == expected_issued
            && run.outcomes()?.as_slice() == expected_outcomes
        {
            accepted = Some(rows);
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let rows = accepted.ok_or("actual transfer/outcome/destination did not reach stage")?;
    let shell = state.peek();
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || shell.core.active_account.is_some()
    {
        return Err("fixture acquired a session/transport/account".into());
    }
    // A prior success toast is cleared after its captured stage; canceled attempts
    // must never publish their own success. This is observation, not result injection.
    let toast = shell.toast.as_ref().map(|toast| toast.text.clone());
    if name == "saved" && toast.as_deref() != Some("Saved original-fixture.bin") {
        return Err("success publication absent".into());
    }
    if matches!(name, "canceled" | "closed")
        && toast.as_deref().is_some_and(|t| t.starts_with("Saved "))
    {
        return Err("canceled transfer published success".into());
    }
    Ok(
        json!({"dom":dom,"files":rows,"issued":run.issued(),"outcomes":run.outcomes()?,"noSession":true,"noTransport":true,"toast":toast}),
    )
}
async fn exercise(mut state: Signal<ShellState>) -> Result<Value, String> {
    let run = install(state).await?;
    let ready = json!({"version":1,"nonce":run.nonce,"pid":std::process::id(),"fixture":run.alias});
    eprintln!("original-download-waiting: {ready}");
    wait_ack(&run, "ready").await?;
    let mut eval = document::eval(include_str!("check.js"));
    let mut stages = Vec::new();
    for name in STAGES {
        let dom: Value = eval.recv().await.map_err(|_| "native evaluation ended")?;
        if dom["stage"] != name {
            return Err(format!("unexpected DOM stage: {dom}"));
        }
        let record = stage(&run, name, dom, state).await?;
        eprintln!("original-download-stage: {record}");
        stages.push(record);
        wait_ack(&run, name).await?;
        if name == "saved" {
            state.write().toast = None;
        }
        eval.send(name).map_err(|_| "native acknowledgement")?;
    }
    let result: Value = eval.recv().await.map_err(|_| "terminal evaluation ended")?;
    if result["ok"] != true {
        return Err("terminal DOM assertion".into());
    }
    let _ = eval.send("received");
    Ok(
        json!({"ok":true,"fixture":run.alias,"nonce":run.nonce,"pid":std::process::id(),"stages":stages,
        "actualHandlers":true,"syntheticAdmissionTransport":true,"osSaveDialog":false,"noSession":true,"noTransport":true,
        "bytes":BYTES,"outcomes":run.outcomes()?}),
    )
}
pub(crate) async fn run(state: Signal<ShellState>) {
    let result = match exercise(state).await {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":error}),
    };
    eprintln!("original-download-check: {result}");
}

#[cfg(test)]
mod scan_tests {
    use super::scan_files;
    use std::{
        fs, io,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            for _ in 0..100 {
                let n = NEXT
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                    .unwrap();
                let path = std::env::temp_dir()
                    .join(format!("sp-download-scan-{}-{n}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("isolated scan directory: {error}"),
                }
            }
            panic!("isolated scan directory exhausted");
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn enumerated_part_removed_during_metadata_invalidates_whole_sample() {
        let dir = Directory::new();
        fs::write(dir.0.join("attempt-2.bin"), b"saved").unwrap();
        let part = dir.0.join(".super-platinum-download-test.part");
        fs::write(&part, b"partial").unwrap();
        let mut removed = false;
        let sample = scan_files(&dir.0, |path| {
            if path == part {
                fs::remove_file(path).unwrap();
                removed = true;
            }
            fs::symlink_metadata(path)
        })
        .unwrap();
        assert!(removed, "injected removal must follow actual enumeration");
        assert_eq!(
            sample, None,
            "the saved-file subset is not a complete sample"
        );
        assert_eq!(
            scan_files(&dir.0, |path| fs::symlink_metadata(path)).unwrap(),
            Some(vec![("attempt-2.bin".into(), 5)])
        );
    }

    #[test]
    fn missing_entry_does_not_mask_another_metadata_error() {
        let dir = Directory::new();
        fs::write(dir.0.join("first"), b"a").unwrap();
        fs::write(dir.0.join("second"), b"b").unwrap();
        let mut calls = 0;
        let result = scan_files(&dir.0, |path| {
            calls += 1;
            if calls == 1 {
                fs::remove_file(path).unwrap();
                fs::symlink_metadata(path)
            } else {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
        });
        assert_eq!(calls, 2);
        assert_eq!(result, Err("download metadata (PermissionDenied)".into()));
    }

    #[test]
    fn invalid_inventory_and_entry_limit_remain_errors() {
        let dir = Directory::new();
        assert!(scan_files(&dir.0.join("missing"), |path| fs::symlink_metadata(path)).is_err());
        fs::create_dir(dir.0.join("directory")).unwrap();
        assert_eq!(
            scan_files(&dir.0, |path| fs::symlink_metadata(path)),
            Err("unexpected download inventory".into())
        );
        fs::remove_dir(dir.0.join("directory")).unwrap();
        for n in 0..5 {
            fs::write(dir.0.join(n.to_string()), b"x").unwrap();
        }
        // Even disappearing entries count toward the scan's existing four-entry cap.
        assert_eq!(
            scan_files(&dir.0, |_| Err(io::Error::from(io::ErrorKind::NotFound))),
            Err("unexpected download inventory".into())
        );
    }
}
