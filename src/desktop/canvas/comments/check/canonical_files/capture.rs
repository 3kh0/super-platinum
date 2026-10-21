use super::*;
pub(super) async fn display(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    target: Option<&str>,
    reads: usize,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-back'",
    )
    .await?;
    eval(
        desktop,
        &include_str!("../file_fallback/scroll.js")
            .replace("__TARGET__", &json!(target).to_string()),
    )
    .await?;
    let script = include_str!("display.js").replace("__TARGET__", &json!(target).to_string());
    let body = eval(desktop, &script).await?;
    let mut layout = eval(desktop, include_str!("layout.js")).await?;
    layout["bodyProof"] = body.clone();
    let proof = data::proof(&state.peek());
    verify(&proof, reads)?;
    let receipt = json!({"ok":true,"stage":name,"ordinal":stages.len(),"noSession":true,"noTransport":true,"proof":proof,"layout":layout});
    emit(
        "canvas-canonical-file-roots",
        name,
        receipt.clone(),
        stages.len(),
    )
    .await?;
    let after_proof = data::proof(&state.peek());
    let after_body = eval(desktop, &script).await?;
    if after_proof != proof || after_body != body {
        let diagnostic = json!({"stage":name,"beforeBody":body,"afterBody":after_body,
            "sourceUnchanged":after_proof==proof,"afterProof":after_proof});
        tokio::task::spawn_blocking(move || {
            eprintln!("canvas-canonical-file-roots-capture-mismatch: {diagnostic}")
        })
        .await
        .map_err(|_| "Capture diagnostic logging failed")?;
        return Err(format!(
            "{name}: original source/target changed during capture"
        ));
    }
    stages.push(receipt);
    Ok(())
}
pub(super) fn verify(proof: &Value, reads: usize) -> Result<(), String> {
    verify_snapshot(proof, reads, true)
}
pub(super) fn verify_snapshot(proof: &Value, reads: usize, visible: bool) -> Result<(), String> {
    if proof["requests"].as_array().map(Vec::len) != Some(reads)
        || proof["responses"].as_array().map(Vec::len) != Some(reads)
        || (visible && proof["retained"]["ownerCurrent"] != true)
        || proof["retained"]["loading"] != false
        || !proof["retained"]["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["matchesOriginal"] == true))
        || !proof["media"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["registered"] == 0))
    {
        return Err("Original source/owner/discarded media proof differs".into());
    }
    Ok(())
}
pub(super) async fn emit(
    prefix: &str,
    name: &str,
    receipt: Value,
    ordinal: usize,
) -> Result<(), String> {
    let prefix = prefix.to_owned();
    tokio::task::spawn_blocking(move || eprintln!("{prefix}-stage: {receipt}"))
        .await
        .map_err(|_| "Stage logging failed")?;
    let dir = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Private ACK directory missing")?;
    for _ in 0..600 {
        let path = dir.join(format!("{ordinal}.ack"));
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "ACK worker failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture ACK read failed".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("{name}: capture ACK timeout"))
}
