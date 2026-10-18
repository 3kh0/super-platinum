//! Actual synthetic UI operations against the exact private SQLite fixture.
use super::*;
use serde_json::{Value, json};
pub(in crate::canvas::comments) fn start(
    state: Signal<ShellState>,
    desktop: dioxus::desktop::DesktopContext,
) {
    if !fixture::requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_COMMENT_RESOLVE_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-comment-resolve-check: {receipt}");
    });
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,"!!document.querySelector('.canvas-comment-open') && !!document.querySelector('.canvas-comment-resolve:not(:disabled)')").await?;
    click(desktop, ".canvas-comment-open").await?;
    wait(desktop,"!!document.getElementById('canvas-comments-all') && document.querySelectorAll('.canvas-comment').length===2").await?;
    let mut layouts = Vec::new();
    stage(state, desktop, "initial", 0, &mut layouts).await?;
    click(desktop, ".canvas-comment-resolve").await?;
    wait(desktop,"document.querySelector('.canvas-comment-resolve:not(:disabled)')?.textContent==='Re-open thread'").await?;
    stage(state, desktop, "resolved", 1, &mut layouts).await?;
    click(desktop, ".canvas-comment-resolve").await?;
    wait(desktop,"document.querySelector('.canvas-comment-resolve:not(:disabled)')?.textContent==='Resolve thread'").await?;
    stage(state, desktop, "reopened", 2, &mut layouts).await?;
    fixture::mode(1);
    click(desktop, ".canvas-comment-resolve").await?;
    wait(desktop,"document.querySelector('#canvas-comments [role=alert]')?.textContent.includes('was not sent')").await?;
    stage(state, desktop, "not-sent", 3, &mut layouts).await?;
    click(desktop, "#canvas-comments-refresh").await?;
    wait(desktop,"!!document.querySelector('.canvas-comment-resolve:not(:disabled)') && !document.querySelector('#canvas-comments [role=alert]')").await?;
    fixture::mode(3);
    click(desktop, ".canvas-comment-resolve").await?;
    wait(desktop,"document.querySelector('#canvas-comments [role=alert]')?.textContent.includes('access changed')").await?;
    stage(state, desktop, "stale", 4, &mut layouts).await?;
    click(desktop, "#canvas-comments-refresh").await?;
    wait(desktop,"document.querySelector('.canvas-comment-resolve:not(:disabled)')?.textContent==='Re-open thread'").await?;
    fixture::mode(2);
    click(desktop, ".canvas-comment-resolve").await?;
    wait(desktop,"document.querySelector('#canvas-comments [role=alert]')?.textContent.includes('unconfirmed') && document.querySelector('.canvas-comment-resolve')?.disabled").await?;
    stage(state, desktop, "uncertain", 5, &mut layouts).await?;
    click(desktop, "#canvas-comments-back").await?;
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-open'",
    )
    .await?;
    click(desktop, "#canvas-comments-open").await?;
    wait(desktop,"document.querySelector('.canvas-comment-resolve')?.disabled && !!document.querySelector('.canvas-comment-open')").await?;
    click(desktop, ".canvas-comment-open").await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===2 && document.querySelector('.canvas-comment-resolve')?.disabled").await?;
    stage(state, desktop, "reopened-uncertain", 6, &mut layouts).await?;
    let disk = {
        let shell = state.peek();
        let owner = Owner::capture(&shell).ok_or("Owner missing")?;
        Disk::capture(&shell, &owner).ok_or("Private storage missing")?
    };
    let scope = disk.scope.clone();
    let receipt = disk
        .work(move |journal| journal.comment(&scope))
        .await
        .map_err(|_| "Receipt read failed")?
        .ok_or("Uncertain receipt was lost")?;
    if receipt.record().desired() || receipt.record().root_ts() != fixture::TS {
        return Err("Wrong durable uncertain intent".into());
    }

    click(desktop, "#canvas-comment-review-open").await?;
    wait(desktop,"!!document.querySelector('.canvas-comment-review') && !document.getElementById('canvas-comment-review-check')?.checked && document.getElementById('canvas-comment-review-confirm')?.disabled && document.querySelector('.canvas-comment-review')?.textContent.includes('currently open')").await?;
    stage(state, desktop, "review", 7, &mut layouts).await?;
    click(desktop, "#canvas-comment-review-check").await?;
    wait(
        desktop,
        "document.getElementById('canvas-comment-review-confirm')?.disabled===false",
    )
    .await?;
    click(desktop, "#canvas-comment-review-confirm").await?;
    wait(desktop,"!document.querySelector('.canvas-comment-review') && !!document.querySelector('.canvas-comment-resolve:not(:disabled)')").await?;
    stage(state, desktop, "reviewed", 8, &mut layouts).await?;
    let scope = disk.scope.clone();
    let retained_history = disk
        .work(move |journal| {
            Ok((
                journal.comment(&scope)?.is_none(),
                journal.comment_review_count(&scope)?,
            ))
        })
        .await
        .map_err(|_| "Review storage read failed")?;
    if retained_history != (true, 1) {
        return Err("Explicit review did not retain history and remove only its lock".into());
    }
    let requests = fixture::ledger();
    let prepared = fixture::prepared();
    if requests.len() != 26 || prepared.len() != 4 {
        return Err(format!(
            "Unexpected request counts {}/{}",
            requests.len(),
            prepared.len()
        ));
    }
    let paths = requests
        .iter()
        .filter_map(|request| {
            request["url"]
                .as_str()
                .and_then(|url| url::Url::parse(url).ok())
                .map(|url| url.path().to_owned())
        })
        .collect::<Vec<_>>();
    let writes = paths
        .iter()
        .filter(|path| path.starts_with("/api/quip.thread."))
        .cloned()
        .collect::<Vec<_>>();
    if writes
        != [
            "/api/quip.thread.archive",
            "/api/quip.thread.unarchive",
            "/api/quip.thread.unarchive",
        ]
    {
        return Err("Unexpected write sequence".into());
    }
    Ok(
        json!({"ok":true,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"syntheticInput":true,"exactRequests":true,"resolved":true,"reopened":true,"knownNotSent":true,"stalePreflightRefused":true,"uncertainNoReplay":true,"durableUncertain":true,"reopenedLocked":true,"focusContinuity":true,"explicitReview":true,"reviewNoReplay":true,"retainedReviewCount":retained_history.1,"requestCount":requests.len(),"preparedCount":prepared.len(),"requests":requests,"prepared":prepared,"layout":layouts.last(),"layouts":layouts,"retainedOperation":receipt.operation_id()}),
    )
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    eval(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e||e.disabled)return {{ok:false}};e.scrollIntoView({{block:'center'}});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(!h||!(e===h||e.contains(h)))return {{ok:false}};e.click();return {{ok:true}};}})()"#,json!(selector))).await?;
    Ok(())
}
async fn wait(desktop: &dioxus::desktop::DesktopContext, condition: &str) -> Result<(), String> {
    for _ in 0..80 {
        if eval_raw(desktop, &format!("({{ok:!!({condition})}})")).await?["ok"] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!("Comment DOM did not settle: {condition}"))
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let value = eval_raw(desktop, script).await?;
    if value["ok"] != true {
        Err(format!("Comment native assertion failed: {value}"))
    } else {
        Ok(value)
    }
}
async fn eval_raw(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Comment evaluation failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Comment evaluation timed out")?
        .ok_or("Comment callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid comment callback")?;
    if let Value::String(value) = value {
        serde_json::from_str(&value).map_err(|_| "Invalid encoded comment callback".into())
    } else {
        Ok(value)
    }
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: usize,
    layouts: &mut Vec<Value>,
) -> Result<(), String> {
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-back'",
    )
    .await?;
    let layout = eval(desktop, include_str!("../layout.js")).await?;
    layouts.push(layout);
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Comment fixture must remain disconnected".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Private capture directory required")?;
    let ack = directory.join(format!("{ordinal}.ack"));
    eprintln!(
        "canvas-comment-resolve-stage: {}",
        json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true})
    );
    for _ in 0..600 {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture acknowledgement task failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture acknowledgement".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture acknowledgement read failed".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("Comment capture timed out at {name}"))
}
