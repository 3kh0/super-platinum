use super::*;
const STAGES: [&str; 6] = [
    "initial",
    "canceled",
    "saved",
    "held",
    "refreshed",
    "retired",
];
async fn ack(cap: &Capability, stage: &str) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    let path = cap.root.join(format!("ack-{stage}.json"));
    let expected = json!({"identity":cap.identity(),"stage":stage});
    for _ in 0..4500 {
        match tokio::fs::symlink_metadata(&path).await {
            Ok(m) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if m.file_attributes() & 0x400 != 0 {
                        return Err("Linked cached ACK".into());
                    }
                }
                if !m.is_file() || m.file_type().is_symlink() || m.len() > 4096 {
                    return Err("Invalid cached ACK".into());
                }
                let mut bytes = Vec::new();
                tokio::fs::File::open(path)
                    .await
                    .map_err(|_| "Cached ACK open")?
                    .take(4097)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| "Cached ACK read")?;
                let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Cached ACK JSON")?;
                return if value == expected {
                    Ok(())
                } else {
                    Err("Wrong cached ACK owner/stage".into())
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cached ACK metadata".into()),
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Cached ACK expired".into())
}
fn isolated(
    state: Signal<ShellState>,
    cap: &Arc<Capability>,
    stage: &str,
) -> Result<Value, String> {
    let shell = state.peek();
    let observed = current(&shell)?;
    if !Arc::ptr_eq(observed, cap) {
        return Err("Cached capability changed".into());
    }
    let fresh = matches!(stage, "refreshed" | "retired");
    let viewer = shell.viewer.as_ref().ok_or("Cached viewer closed")?;
    let bytes = shell
        .media
        .ready_download_bytes(&viewer.id)
        .ok_or("Cached viewer bytes")?;
    if (viewer.id != cap.old) != fresh
        || bytes.as_ref()
            != if fresh {
                NEW.as_bytes()
            } else {
                OLD.as_bytes()
            }
        || cap.cancel.load(Ordering::Acquire) != fresh
        || refresh::lease(&shell, &viewer.id).is_none()
        || shell.core.session.as_ref().is_none_or(|s| {
            !s.d_cookie.is_empty() || s.workspaces.values().any(|w| !w.token.is_empty())
        })
    {
        return Err("Cached asset/lease/isolation changed".into());
    }
    let toast = shell.toast.as_ref().map(|t| t.text.as_str());
    if toast
        != if stage == "saved" {
            Some("Saved release-notes.txt")
        } else {
            None
        }
    {
        return Err("Cached toast mismatch".into());
    }
    Ok(
        json!({"old":cap.old.uri(),"current":viewer.id.uri(),"fresh":fresh,"oldCanceled":fresh,"bytes":std::str::from_utf8(&bytes).map_err(|_|"Cached UTF8")?,"syntheticSession":true,"structurallyDisconnected":true,"toast":toast}),
    )
}
async fn exercise(mut state: Signal<ShellState>) -> Result<Value, String> {
    let cap = install(state).await?;
    emit("waiting", cap.identity()).await;
    ack(&cap, "ready").await?;
    let mut eval = document::eval(&format!(
        "{};\n{}",
        include_str!("../toast_check.js"),
        include_str!("check.js")
    ));
    eval.send(json!({"old":OLD,"fresh":NEW}))
        .map_err(|_| "Cached init")?;
    let mut records = Vec::new();
    let mut post_held_deadline = None;
    for stage in STAGES {
        // Only the automatic held -> refreshed transition is short-bounded.
        // Human review/capture ACKs are outside this deadline.
        let deadline = if stage == "refreshed" {
            post_held_deadline.ok_or("Missing cached post-held deadline")?
        } else {
            tokio::time::Instant::now() + Duration::from_secs(180)
        };
        let dom: Value = tokio::time::timeout_at(deadline, eval.recv())
            .await
            .map_err(|_| "Cached DOM deadline")?
            .map_err(|_| "Cached DOM bridge")?;
        if dom["ok"] != true || dom["stage"] != stage || dom["viewport"] != json!([1280, 780]) {
            return Err(format!("Cached DOM failed: {dom}"));
        }
        let deadline = if stage == "refreshed" {
            deadline
        } else {
            tokio::time::Instant::now() + Duration::from_secs(180)
        };
        loop {
            current(&state.peek())?;
            if cap.ledger.lock().map_err(|_| "Cached ledger")?.at(stage) {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("Cached actual outcome deadline".into());
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        let observed = isolated(state, &cap, stage)?;
        let ledger = cap.snapshot()?;
        let record = json!({"ok":true,"identity":cap.identity(),"stage":stage,"dom":dom,"ledger":ledger,"observed":observed});
        emit("stage", record.clone()).await;
        records.push(record);
        ack(&cap, stage).await?;
        isolated(state, &cap, stage)?;
        if cap.snapshot() != Ok(ledger) {
            return Err("Cached ledger changed while held".into());
        }
        if stage == "saved" {
            state.write().toast = None;
        }
        if stage == "refreshed" {
            cap.release(&state.peek())?;
        }
        if stage == "held" {
            post_held_deadline = Some(tokio::time::Instant::now() + Duration::from_secs(9));
        }
        eval.send(stage).map_err(|_| "Cached DOM ACK")?;
    }
    let terminal: Value = tokio::time::timeout(Duration::from_secs(5), eval.recv())
        .await
        .map_err(|_| "Cached terminal deadline")?
        .map_err(|_| "Cached terminal")?;
    if terminal != json!({"ok":true}) {
        return Err("Cached terminal DOM differs".into());
    }
    isolated(state, &cap, "retired")?;
    if cap.release.lock().map_err(|_| "Cached release")?.is_some() {
        return Err("Cached hold survived completion".into());
    }
    Ok(
        json!({"ok":true,"identity":cap.identity(),"stages":records,"ledger":cap.snapshot()?,"actualRfd":true,"programmaticActivation":true,"syntheticSession":true,"structurallyDisconnected":true,"lateCommitExercised":false}),
    )
}
pub(super) async fn run(state: Signal<ShellState>) {
    let result = match exercise(state).await {
        Ok(v) => v,
        Err(e) => {
            // Failure-only evidence; no new successful stage or synthetic outcome.
            // Copy shell/ledger observations before the awaited blocking emitter.
            let diagnostic = {
                let shell = state.peek();
                CAP.get().map(|cap| {
                    let viewer = shell.viewer.as_ref();
                    let bytes = viewer.and_then(|v| shell.media.ready_download_bytes(&v.id));
                    json!({"identity":cap.identity(),"ledger":cap.snapshot().ok(),
                        "capabilityCurrent":current(&shell).is_ok(),
                        "ownerCurrent":cap.owner.current(&shell),
                        "viewer":viewer.map(|v|v.id.uri()),"old":cap.old.uri(),
                        "oldCanceled":cap.cancel.load(Ordering::Acquire),
                        "viewerLeaseCurrent":viewer.is_some_and(|v|refresh::lease(&shell,&v.id).is_some()),
                        "oldBytes":bytes.as_deref()==Some(OLD.as_bytes()),
                        "freshBytes":bytes.as_deref()==Some(NEW.as_bytes())})
                })
            };
            json!({"ok":false,"error":e,"diagnostic":diagnostic})
        }
    };
    emit("check", result).await;
}
