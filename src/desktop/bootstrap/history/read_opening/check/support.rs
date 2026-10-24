use super::*;
pub(super) async fn eval(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |v| {
            let _ = tx.send(v);
        })
        .map_err(|_| "Native evaluation failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .map_err(|_| "Evaluation timed out")?
        .ok_or("Callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid callback")?;
    if let Value::String(v) = value {
        serde_json::from_str(&v).map_err(|_| "Invalid encoded callback".into())
    } else {
        Ok(value)
    }
}
pub(super) async fn dom(
    desktop: &dioxus::desktop::DesktopContext,
    call: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..80 {
        last = eval(
            desktop,
            &format!(
                "{};window.superPlatinumUnreadCheck.{call}",
                include_str!("dom.js")
            ),
        )
        .await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!("Unread DOM did not settle: {last}"))
}
pub(super) async fn wait(
    state: Signal<ShellState>,
    predicate: impl Fn(&ShellState) -> bool,
) -> Result<(), String> {
    for _ in 0..160 {
        if predicate(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Unread state did not settle".into())
}
pub(super) async fn held(start: usize) -> Result<usize, String> {
    for _ in 0..160 {
        if let Some(r) = fixture::requests()
            .iter()
            .skip(start)
            .find(|r| r["held"] == true && r["released"] == false && r["outcome"].is_null())
        {
            return Ok(r["id"].as_u64().unwrap() as usize);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Unread response did not hold".into())
}
pub(super) async fn release(
    state: Signal<ShellState>,
    id: usize,
    accepted: bool,
) -> Result<(), String> {
    fixture::release(id)?;
    wait(state, |_| !fixture::requests()[id]["accepted"].is_null()).await?;
    ensure(
        fixture::requests()[id]["accepted"] == accepted,
        "Wrong page authority",
    )
}
pub(super) async fn joined(state: Signal<ShellState>, opening: usize) -> Result<(), String> {
    wait(state, |_| {
        fixture::events().iter().any(|e| {
            e["opening"] == opening && e["kind"] == "join" && e["detail"]["decision"] == "Waiting"
        })
    })
    .await
}
pub(super) async fn stable(
    desktop: &dioxus::desktop::DesktopContext,
    call: &str,
) -> Result<Value, String> {
    dom(desktop, "ready()").await?;
    for _ in 0..80 {
        let before = dom(desktop, call).await?;
        let mut frames = dioxus::document::eval(
            "requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();",
        );
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(2), frames.recv::<Value>()).await;
        let _ = frames.send(());
        if !matches!(result,Ok(Ok(ref v)) if v["ok"]==true) {
            return Err("Unread frame wait failed".into());
        }
        let after = dom(desktop, call).await?;
        if before == after {
            let mut after = after;
            after["stableFrames"] = json!(2);
            return Ok(after);
        }
    }
    Err("Unread geometry changed across frames".into())
}
pub(super) async fn stage(
    state: Signal<ShellState>,
    name: &str,
    ordinal: &mut usize,
    geometry: Value,
) -> Result<(), String> {
    let s = state.peek();
    ensure(
        s.core.session.is_none() && s.core.transport.is_none(),
        "Disconnected scope escaped",
    )?;
    let evidence = json!({"ok":true,"stage":name,"ordinal":*ordinal,"noSession":true,"noTransport":true,"noHttp":true,"geometry":geometry,"navigation":fixture::snapshot(&s),"requests":fixture::requests(),"openings":fixture::openings(),"events":fixture::events(),"noReadAcknowledgement":true});
    drop(s);
    let dir = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Private ACK directory missing")?;
    eprintln!("reconnect-unread-opening-stage: {evidence}");
    let path = dir.join(format!("{}.ack", *ordinal));
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let p = path.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(p))
            .await
            .map_err(|_| "Capture worker failed")?
        {
            Ok(b) if b == b"captured\n" => {
                *ordinal += 1;
                return Ok(());
            }
            Ok(_) => return Err("Malformed capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture ACK failed".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Capture ACK timeout".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
