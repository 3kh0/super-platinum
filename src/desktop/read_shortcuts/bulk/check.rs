//! Native bulk-clear controls with synthetic requests and actual captured UI.
use super::*;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let mut result =
            match tokio::time::timeout(Duration::from_secs(150), run(state, &desktop)).await {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => json!({"ok":false,"error":e}),
                Err(_) => json!({"ok":false,"error":"Bulk read checker timeout"}),
            };
        // This fixture shares Escape's real bridge; keep first-action failures
        // reviewable even when the dialog never mounts or the run times out.
        if result["ok"] != true {
            let diagnostics = eval("dioxus.send({ok:true,events:window.superPlatinumReadShortcutCheckDiagnostics?.snapshot()??[]});await dioxus.recv();".into()).await;
            result["bridgeDiagnostics"] = match diagnostics {
                Ok(value) => value,
                Err(error) => json!({"available":false,"error":error}),
            };
            let shell = state.peek();
            result["nativeDiagnostics"] = json!({
                "key":crate::read_shortcuts::key(&shell),
                "isolated":fixture::enabled(&shell),
                "dialogOpen":shell.bulk_read.dialog.is_some(),
                "requestCount":shell.bulk_read.fixture.as_ref().map(|f|f.ledger.len()),
                "pending":shell.bulk_read.fixture.as_ref().map(|f|f.pending.is_some()),
            });
        }
        if let Err(error) = eval("window.superPlatinumReadShortcutCheckDiagnostics?.dispose();dioxus.send({ok:true});await dioxus.recv();".into()).await {
            result["ok"] = json!(false);
            result["diagnosticsCleanupError"] = json!(error);
        }
        eprintln!("bulk-read-check: {result}");
    });
}
async fn eval(script: String) -> Result<Value, String> {
    let mut e = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), e.recv::<Value>()).await;
    let _ = e.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Bulk DOM: {v}")),
        _ => Err("Bulk DOM timeout".into()),
    }
}
async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
async fn click(id: &str) -> Result<(), String> {
    eval(format!(r#"const e=document.getElementById({});const enabled=!!e&&!e.matches(':disabled');if(enabled){{e.focus({{preventScroll:true}});e.click();}}dioxus.send({{ok:enabled}});await dioxus.recv();"#,json!(id))).await.map(|_|())
}
async fn shortcut() -> Result<(), String> {
    frames().await?;
    eval("const e=document.getElementById('channel-composer');if(e){e.focus({preventScroll:true});e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',code:'Escape',shiftKey:true,bubbles:true,cancelable:true}));}dioxus.send({ok:!!e&&!!window.superPlatinumReadShortcuts});await dioxus.recv();".into()).await.map(|_|())
}
async fn wait(s: Signal<ShellState>, test: impl Fn(&ShellState) -> bool) -> Result<(), String> {
    for _ in 0..250 {
        if test(&s.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Bulk state did not settle".into())
}
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn requests(s: &ShellState) -> usize {
    s.bulk_read.fixture.as_ref().unwrap().ledger.len()
}
fn pending(s: &ShellState) -> bool {
    s.bulk_read.fixture.as_ref().unwrap().pending.is_some()
}
fn entry(s: &ShellState) -> Option<&Entry> {
    s.bulk_read.entries.get(&Owner::capture(s)?)
}
fn undo_available(s: &ShellState) -> bool {
    entry(s).and_then(|e| e.notice.as_ref()).is_some_and(|n| {
        n.receipt.is_some() && n.context.current(s) && n.started.elapsed() < n.duration
    })
}
fn uncertain(s: &ShellState) -> bool {
    entry(s)
        .and_then(|e| e.operation.as_ref())
        .is_some_and(|o| o.phase == Phase::Unknown)
}
fn messages(s: &ShellState) -> Value {
    json!(
        s.core.workspaces["T1"]
            .messages
            .iter()
            .map(|(key, bag)| (key, &bag.messages))
            .collect::<BTreeMap<_, _>>()
    )
}
async fn ready() -> Result<(std::path::PathBuf, String, String), String> {
    let (dir, nonce) = fixture::directory()?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing alias")?;
    let expected = json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":name});
    let path = dir.join("ready.json");
    if path.exists() {
        return Err("Readiness marker must be new".into());
    }
    eprintln!("bulk-read-waiting: {expected}");
    for _ in 0..300 {
        let check = path.clone();
        let found = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let meta = match std::fs::symlink_metadata(&check) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("Cannot inspect readiness".into()),
            };
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 {
                return Err("Invalid readiness file".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Readiness reparse point refused".into());
                }
            }
            serde_json::from_slice(&std::fs::read(check).map_err(|_| "Cannot read readiness")?)
                .map(Some)
                .map_err(|_| "Invalid readiness JSON".into())
        })
        .await
        .map_err(|_| "Readiness task failed")??;
        if let Some(found) = found {
            if found != expected {
                return Err("Readiness owner mismatch".into());
            }
            return Ok((dir, nonce, name));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Readiness timed out".into())
}

async fn capture(
    d: &dioxus::desktop::DesktopContext,
    dir: &std::path::Path,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
    s: Signal<ShellState>,
) -> Result<(), String> {
    frames().await?;
    let mut g=eval(r#"const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}};const dialog=document.getElementById('bulk-read-confirm'),status=document.getElementById('bulk-read-status'),composer=document.getElementById('channel-composer'),surface=dialog||status||composer;const targets=dialog?[...dialog.querySelectorAll('button,input')]:status?[...status.querySelectorAll('button')]:[composer];const controls=targets.filter(Boolean).map(e=>{const r=rect(e);return {name:e.id,rect:r,hit:e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)),disabled:e.matches(':disabled'),checked:e.checked??null}});const r=surface?rect(surface):null;const ok=!!r&&r.x>=-.5&&r.y>=-.5&&r.x+r.width<=innerWidth+.5&&r.y+r.height<=innerHeight+.5&&controls.every(c=>c.hit&&c.rect.x>=0&&c.rect.y>=0&&c.rect.x+c.rect.width<=innerWidth+.5&&c.rect.y+c.rect.height<=innerHeight+.5)&&document.documentElement.scrollWidth<=innerWidth+1;dioxus.send({ok,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,surface:r,surfaceKind:dialog?'confirmation':status?'status':'composer',controls,text:dialog?.innerText||status?.innerText||'',focusedControl:document.activeElement?.id||null});await dioxus.recv();"#.into()).await?;
    let names: Vec<_> = g["controls"]
        .as_array()
        .ok_or("Missing controls")?
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    let confirm = matches!(stage, "confirmation" | "confirmation-checked");
    ensure(
        (g["surfaceKind"] == "confirmation") == confirm,
        "Unexpected confirmation surface",
    )?;
    if confirm {
        ensure(
            names
                == [
                    "bulk-read-close",
                    "bulk-read-dont-ask",
                    "bulk-read-cancel",
                    "bulk-read-confirm-submit",
                ],
            "Wrong confirmation controls",
        )?;
        let checked = g["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "bulk-read-dont-ask")
            .unwrap();
        ensure(
            checked["checked"] == json!(stage == "confirmation-checked"),
            "Checkbox changed without explicit input",
        )?;
    }
    ensure(
        names.contains(&"bulk-read-undo")
            == matches!(stage, "cleared" | "expiry-ready" | "owner-ready"),
        "Unexpected Undo control",
    )?;
    ensure(
        names.contains(&"bulk-read-refresh") == matches!(stage, "uncertain" | "refreshed"),
        "Unexpected refresh control",
    )?;
    g["stage"] = json!(stage);
    g["requestCount"] = json!(requests(&s.peek()));
    g["state"] = fixture::snapshot(&s.peek());
    geometry.push(g);
    eprintln!(
        "bulk-read-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) = crate::agent::capture::window(
        Some(
            dir.join(format!("{stage}.png"))
                .to_string_lossy()
                .into_owned(),
        ),
        d,
    )
    .await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "bulk-read-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
async fn run(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (dir, nonce, name) = ready().await?;
    ensure(
        fixture::enabled(&s.peek()) && requests(&s.peek()) == 0,
        "Fixture isolation or initial requests",
    )?;
    eval(include_str!("../check_diagnostics.js").into()).await?;
    let initial = fixture::snapshot(&s.peek());
    let content = messages(&s.peek());
    let reading = s.peek().core.workspaces["T1"].read_preferences.clone();
    let notification = json!(s.peek().core.workspaces["T1"].notifications);
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    capture(d, &dir, "initial", &mut captures, &mut geometry, s).await?;
    shortcut().await?;
    wait(s, |s| s.bulk_read.dialog.is_some()).await?;
    capture(d, &dir, "confirmation", &mut captures, &mut geometry, s).await?;
    ensure(
        requests(&s.peek()) == 0,
        "Opening confirmation sent requests",
    )?;
    click("bulk-read-cancel").await?;
    wait(s, |s| s.bulk_read.dialog.is_none()).await?;
    ensure(
        requests(&s.peek()) == 0 && fixture::snapshot(&s.peek()) == initial,
        "Cancel changed read state",
    )?;
    capture(d, &dir, "cancelled", &mut captures, &mut geometry, s).await?;
    shortcut().await?;
    wait(s, |s| s.bulk_read.dialog.is_some()).await?;
    click("bulk-read-dont-ask").await?;
    wait(s, |s| {
        s.bulk_read.dialog.as_ref().is_some_and(|d| d.dont_ask)
    })
    .await?;
    capture(
        d,
        &dir,
        "confirmation-checked",
        &mut captures,
        &mut geometry,
        s,
    )
    .await?;
    ensure(requests(&s.peek()) == 0, "Checkbox alone sent preference")?;
    click("bulk-read-confirm-submit").await?;
    wait(s, |s| pending(s) && requests(s) == 4).await?;
    ensure(
        fixture::snapshot(&s.peek()) == initial,
        "Sending optimistically cleared counts",
    )?;
    capture(d, &dir, "sending", &mut captures, &mut geometry, s).await?;
    shortcut().await?;
    frames().await?;
    ensure(requests(&s.peek()) == 4, "Pending clear duplicated")?;
    ensure(
        fixture::complete(&mut s.write(), false),
        "Acknowledgement alone changed counts",
    )?;
    ensure(
        undo_available(&s.peek()) && requests(&s.peek()) == 5,
        "Clear receipt or reconciliation missing",
    )?;
    let cleared = fixture::snapshot(&s.peek());
    ensure(cleared != initial, "Authoritative counts did not update")?;
    capture(d, &dir, "cleared", &mut captures, &mut geometry, s).await?;
    click("bulk-read-undo").await?;
    wait(s, |s| pending(s) && requests(s) == 6).await?;
    undo(s);
    ensure(requests(&s.peek()) == 6, "Undo repeated")?;
    capture(d, &dir, "undo-sending", &mut captures, &mut geometry, s).await?;
    ensure(
        fixture::complete(&mut s.write(), false),
        "Undo acknowledgement invented counts",
    )?;
    ensure(
        !undo_available(&s.peek()) && requests(&s.peek()) == 7,
        "Undo created another Undo",
    )?;
    ensure(
        fixture::snapshot(&s.peek()) == initial,
        "Authoritative Undo counts differ",
    )?;
    capture(d, &dir, "undone", &mut captures, &mut geometry, s).await?;
    shortcut().await?;
    wait(s, |s| {
        pending(s) && requests(s) == 8 && s.bulk_read.dialog.is_none()
    })
    .await?;
    ensure(
        fixture::complete(&mut s.write(), false),
        "Second clear optimistic mutation",
    )?;
    capture(d, &dir, "expiry-ready", &mut captures, &mut geometry, s).await?;
    let (started, duration) = {
        let shell = s.peek();
        let n = entry(&shell).unwrap().notice.as_ref().unwrap();
        (n.started, n.duration)
    };
    while started.elapsed() < duration {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let expiry_elapsed = started.elapsed().as_secs_f64();
    poll(&mut s.write());
    undo(s);
    ensure(
        !undo_available(&s.peek()) && requests(&s.peek()) == 9,
        "Expired Undo dispatched",
    )?;
    capture(d, &dir, "expired", &mut captures, &mut geometry, s).await?;
    shortcut().await?;
    wait(s, |s| pending(s) && requests(s) == 10).await?;
    ensure(
        fixture::complete(&mut s.write(), false),
        "Third clear optimistic mutation",
    )?;
    capture(d, &dir, "owner-ready", &mut captures, &mut geometry, s).await?;
    let epoch = s.peek().core.account_epoch;
    s.write().core.account_epoch += 1;
    poll(&mut s.write());
    undo(s);
    ensure(
        !undo_available(&s.peek()) && requests(&s.peek()) == 11,
        "Changed owner used Undo",
    )?;
    capture(d, &dir, "owner-changed", &mut captures, &mut geometry, s).await?;
    shortcut().await?;
    wait(s, |s| pending(s) && requests(s) == 12).await?;
    ensure(
        fixture::complete(&mut s.write(), true) && uncertain(&s.peek()),
        "Lost response did not hold operation",
    )?;
    capture(d, &dir, "uncertain", &mut captures, &mut geometry, s).await?;
    shortcut().await?;
    frames().await?;
    ensure(requests(&s.peek()) == 12, "Unknown clear replayed")?;
    click("bulk-read-refresh").await?;
    wait(s, |s| requests(s) == 13).await?;
    ensure(
        uncertain(&s.peek()),
        "Counts incorrectly resolved uncertain mutation",
    )?;
    shortcut().await?;
    frames().await?;
    ensure(requests(&s.peek()) == 13, "Refreshed unknown replayed")?;
    capture(d, &dir, "refreshed", &mut captures, &mut geometry, s).await?;
    let shell = s.peek();
    let f = shell.bulk_read.fixture.as_ref().unwrap();
    ensure(
        f.valid
            && f.ledger.len() == 13
            && f.reconciliations.len() == 5
            && captures.len() == fixture::STAGES.len(),
        "Ledger/capture mismatch",
    )?;
    ensure(
        messages(&shell) == content
            && shell.core.workspaces["T1"].read_preferences == reading
            && json!(shell.core.workspaces["T1"].notifications) == notification
            && f.raw["unknown_pref"] == json!({"keep":[1,2]})
            && fixture::enabled(&shell),
        "Unrelated state changed or isolation lost",
    )?;
    Ok(
        json!({"ok":true,"fixture":name,"nonce":nonce,"pid":std::process::id(),"noSession":true,"noTransport":true,"syntheticInput":true,"actualPreparedRequests":true,"preparedRequests":13,"viewport":geometry[0]["viewport"],"captures":captures,"geometry":geometry,"ledger":f.ledger,"reconciliations":f.reconciliations,"initialState":initial,"finalState":fixture::snapshot(&shell),"finalRaw":f.raw,"expiry":{"durationSeconds":duration.as_secs_f64(),"elapsedSeconds":expiry_elapsed,"requestCountBefore":9,"requestCountAfter":9},"ownerRotation":{"beforeEpoch":epoch,"afterEpoch":shell.core.account_epoch,"requestCountBefore":11,"requestCountAfter":11},"checks":{"openNoRequest":true,"cancelNoMutation":true,"checkboxNoRequest":true,"independentPreference":true,"noOptimisticCounts":true,"duplicateClearBlocked":true,"oneUseUndo":true,"undoNoNewUndo":true,"expiredUndoRefused":true,"ownerUndoRefused":true,"unknownNoReplay":true,"refreshCannotResolveUnknown":true,"authoritativeCountsApplied":true,"unrelatedStatePreserved":true},"limitations":["Synthetic keyboard and responses; no trusted input or live Slack.","Counts-only native reconciliation; production additionally refreshes feeds/history/replies.","No restart or durable Undo; Undo is memory-only and owner-scoped."]}),
    )
}
